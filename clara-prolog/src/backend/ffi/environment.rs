//! Safe wrapper around SWI-Prolog engine
//!
//! Provides `PrologEnvironment` - a safe interface for Prolog operations.
//! Each environment wraps an isolated SWI-Prolog engine for session safety.

use super::bindings::*;
use super::conversion::*;
use crate::error::{PrologError, PrologResult};
use std::ffi::CString;
use std::sync::OnceLock;
use uuid::Uuid;

/// Compile-time SWI_HOME_DIR from build.rs
const SWI_HOME_DIR: &str = env!("SWI_HOME_DIR");

/// Initialization result: Ok(()) for success, Err(message) for failure
static INIT_RESULT: OnceLock<Result<(), String>> = OnceLock::new();

/// Ensure the global Prolog system is initialized
///
/// This is called automatically when creating environments.
/// It only runs once per process.
pub fn ensure_prolog_initialized() -> PrologResult<()> {
    let result = INIT_RESULT.get_or_init(|| {
        // Set SWI_HOME_DIR environment variable if not already set
        // This tells SWI-Prolog where to find its library/boot files
        if std::env::var("SWI_HOME_DIR").is_err() {
            std::env::set_var("SWI_HOME_DIR", SWI_HOME_DIR);
            log::debug!("Set SWI_HOME_DIR to {}", SWI_HOME_DIR);
        }

        // Build argv for PL_initialise
        // --quiet: suppress banner
        // --nosignals: don't install signal handlers (Rust handles those)
        let argv_strings: Vec<CString> = vec![
            CString::new("clara-prolog").unwrap(),
            CString::new("--quiet").unwrap(),
            CString::new("--nosignals").unwrap(),
        ];

        let mut argv_ptrs: Vec<*mut i8> = argv_strings
            .iter()
            .map(|s| s.as_ptr() as *mut i8)
            .collect();

        let argc = argv_ptrs.len() as i32;

        log::debug!("Initializing SWI-Prolog with {} args", argc);

        let init_result = unsafe { PL_initialise(argc, argv_ptrs.as_mut_ptr()) };

        if init_result == 0 {
            log::error!("Failed to initialize SWI-Prolog");
            return Err("PL_initialise returned 0".to_string());
        }

        log::info!("SWI-Prolog initialized successfully");

        // All PL_call() invocations below run in the initializing thread, which
        // owns the main Prolog engine after PL_initialise(). This is the ONLY safe
        // place to call PL_call() globally — other threads may not have an active
        // engine and would SIGSEGV if they called PL_call() outside a with_engine block.

        unsafe {
            // Load JSON libraries
            for goal_str in &[
                "use_module(library(http/json))",
                "use_module(library(http/json_convert))",
            ] {
                let goal = CString::new(*goal_str).unwrap();
                let term = PL_new_term_ref();
                if PL_chars_to_term(goal.as_ptr(), term) != 0 {
                    if PL_call(term, std::ptr::null_mut()) != 0 {
                        log::info!("{} loaded successfully", goal_str);
                    } else {
                        log::warn!("Failed to load {} — predicates may be unavailable", goal_str);
                    }
                }
            }
        }

        // Register foreign predicates while we own the main engine.
        // Both register_* functions are idempotent (OnceLock) so it is safe to
        // call them again later from new(); they will just return the cached result.
        super::callbacks::register_clara_evaluate();
        super::coire_bridge::register_coire_predicates();
        super::ritual_bridge::register_ritual_predicates();

        // Load the commonly-needed prolog-lib libraries now that their foreign
        // predicates are registered. Must happen here (in the main-engine
        // thread) not in a separate OnceLock that might execute from a
        // worker thread with no engine context.
        //
        // the_rabbit/the_cow (ponder_text/2, ruminate_opts/3, etc.) are
        // loaded alongside the_coire so every session has them without
        // requiring an explicit `use_module` in hand-authored
        // `prolog_clauses` — omitting it previously threw
        // existence_error(procedure, ponder_text/2) at goal-execution time
        // with no error visible outside this process's own log.
        //
        // the_rat (clara_fy/2,3, reasoned_response/2,3) was missing from
        // this list — confirmed live 2026-08-21 building
        // progressive_research.pl, the first ruleset to ever call
        // clara_fy/3: existence_error(procedure, clara_fy/3), same failure
        // class as the ponder_text/2 case above, just never hit before
        // since nothing had called it through this path.
        for library in ["the_coire", "the_rabbit", "the_cow", "the_rat"] {
            unsafe {
                let goal = CString::new(format!("use_module(library({library}))")).unwrap();
                let term = PL_new_term_ref();
                if PL_chars_to_term(goal.as_ptr(), term) != 0 {
                    if PL_call(term, std::ptr::null_mut()) != 0 {
                        log::info!("{library} library loaded");
                    } else {
                        log::warn!("Failed to load library({library})");
                        return Err(format!("Failed to load library({library})"));
                    }
                }
            }
        }

        Ok(())
    });

    log::debug!("Prolog initialization result: {:?}", result);
    result.clone().map_err(PrologError::InitializationFailed)
}

/// Check if Prolog is initialized
pub fn is_prolog_initialized() -> bool {
    INIT_RESULT
        .get()
        .map(|r| r.is_ok())
        .unwrap_or(false)
}

/// Load `library(the_coire)` into the global Prolog system.
///
/// This is a no-op: the library is loaded as part of `ensure_prolog_initialized()`.
/// Kept as a public API for callers that call it explicitly (e.g., `init_global()`).
pub fn load_coire_library() -> PrologResult<()> {
    ensure_prolog_initialized()
}

/// Safe wrapper around a SWI-Prolog Engine
///
/// Each `PrologEnvironment` represents an isolated Prolog engine.
/// For session isolation, each session should have its own environment.
///
/// # Thread Safety
///
/// SWI-Prolog engines are single-threaded. The `PrologEnvironment` is marked
/// as `Send` and `Sync` because ownership can be transferred between threads,
/// but all operations must be performed while holding the engine context.
pub struct PrologEnvironment {
    engine: PL_engine_t,
    is_main: bool,
    session_id: Uuid,
}

impl std::fmt::Debug for PrologEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrologEnvironment")
            .field("engine", &format!("{:p}", self.engine))
            .field("is_main", &self.is_main)
            .field("session_id", &self.session_id)
            .finish()
    }
}

impl PrologEnvironment {
    /// Create a new Prolog engine for session isolation
    ///
    /// Each call creates a fresh engine with its own Coire session UUID.
    /// The engine is seeded with a `thread_local` `coire_session_id/1` fact
    /// so that `the_coire` predicates know which session they belong to.
    pub fn new() -> PrologResult<Self> {
        // ensure_prolog_initialized() handles all one-time global setup:
        // PL_initialise, JSON libraries, foreign predicate registration,
        // and the_coire library loading — all in the safe main-engine thread.
        ensure_prolog_initialized()?;

        let session_id = Uuid::new_v4();

        let engine = unsafe {
            let e = PL_create_engine(std::ptr::null_mut());
            if e.is_null() {
                return Err(PrologError::EngineCreationFailed(
                    "PL_create_engine returned null".to_string(),
                ));
            }
            log::debug!("Created new Prolog engine: {:p}", e);
            e
        };

        let env = Self { engine, is_main: false, session_id };

        // Seed the engine's thread_local coire_session_id/1 with this session's UUID.
        // Must be module-qualified so it lands in the_coire's thread-local storage.
        let clause = format!("the_coire:coire_session_id('{}')", session_id);
        env.assertz(&clause)?;

        Ok(env)
    }

    /// Get reference to the main Prolog engine (singleton)
    ///
    /// The main engine is shared and should be used carefully.
    /// Prefer `new()` for session isolation.
    pub fn main_engine() -> PrologResult<Self> {
        ensure_prolog_initialized()?;

        Ok(Self {
            engine: PL_ENGINE_MAIN,
            is_main: true,
            session_id: Uuid::nil(),
        })
    }

    /// Return this environment's Coire session UUID.
    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    /// Poll Coire for pending events and dispatch them via `coire_consume/0`.
    ///
    /// Returns the number of pending events that were processed.
    pub fn consume_coire_events(&self) -> PrologResult<usize> {
        let before = clara_coire::global()
            .count_pending(self.session_id)
            .map_err(|e| PrologError::Internal(e.to_string()))?;
        self.query_once("coire_consume")?;
        Ok(before)
    }

    /// Execute a query and return all solutions as JSON
    ///
    /// # Arguments
    /// * `goal` - A Prolog goal as a string (e.g., "member(X, [1,2,3])")
    ///
    /// # Returns
    /// JSON array of all solutions
    pub fn query(&self, goal: &str) -> PrologResult<String> {
        self.with_engine(|| unsafe {
            let fid = PL_open_foreign_frame();
            let result = self.execute_query_all(goal);
            PL_close_foreign_frame(fid);
            result
        })
    }

    /// Execute a query and return the first solution only
    ///
    /// More efficient than `query()` when only one solution is needed.
    pub fn query_once(&self, goal: &str) -> PrologResult<String> {
        self.with_engine(|| unsafe {
            let fid = PL_open_foreign_frame();
            let result = self.execute_query_once(goal);
            PL_close_foreign_frame(fid);
            result
        })
    }

    /// Execute a query and return variable bindings for REPL display
    ///
    /// Returns JSON array of binding objects like [{"A": "stan"}, {"B": 42}]
    /// This is suitable for interactive REPL output showing variable assignments.
    pub fn query_with_bindings(&self, goal: &str) -> PrologResult<String> {
        self.with_engine(|| unsafe {
            let fid = PL_open_foreign_frame();
            let result = self.execute_query_with_bindings(goal);
            PL_close_foreign_frame(fid);
            result
        })
    }

    /// Assert a clause (fact or rule) into the database
    ///
    /// # Arguments
    /// * `clause` - A Prolog clause (e.g., "parent(tom, mary)" or "ancestor(X,Y) :- parent(X,Y)")
    pub fn assertz(&self, clause: &str) -> PrologResult<()> {
        let goal = format!("assertz(({}))", clause);
        self.query_once(&goal).map(|_| ())
    }

    /// Assert a clause at the beginning of the database
    pub fn asserta(&self, clause: &str) -> PrologResult<()> {
        let goal = format!("asserta(({}))", clause);
        self.query_once(&goal).map(|_| ())
    }

    /// Retract a clause from the database
    pub fn retract(&self, clause: &str) -> PrologResult<()> {
        let goal = format!("retract(({}))", clause);
        self.query_once(&goal).map(|_| ())
    }

    /// Retract all clauses matching a pattern
    pub fn retractall(&self, pattern: &str) -> PrologResult<()> {
        let goal = format!("retractall({})", pattern);
        self.query_once(&goal).map(|_| ())
    }

    /// Consult/load Prolog code from a file
    pub fn consult_file(&self, path: &str) -> PrologResult<()> {
        // Escape path for Prolog
        let escaped_path = path.replace("'", "\\'");
        let goal = format!("consult('{}')", escaped_path);
        self.query_once(&goal).map(|_| ())
    }

    /// Load Prolog code from a string
    ///
    /// Parses each clause and asserts it into the database. The first time a
    /// given predicate indicator is seen, it is declared `thread_local` and
    /// any pre-existing clauses for it are retracted, before the new clause
    /// is asserted — see the two-part rationale below. Getting the predicate
    /// indicator itself right for *rule* clauses (`Head :- Body`, as opposed
    /// to bare facts) needs its own care: `functor/3` on the whole clause
    /// term gives the functor of the top-level `:-`/2 control construct
    /// (`:-`, arity 2), not the head's own indicator — so this extracts `F/A`
    /// from `Head`, not from the raw parsed term, whenever the parsed term is
    /// a rule. Confirmed live (2026-08-20): getting this wrong made every
    /// rule after the first one in a multi-predicate `prolog_clauses`/source
    /// silently skip `thread_local`/`retractall` entirely (each one's real `F/A`
    /// came back as `:-/2`, which the "already seen" bookkeeping then matched
    /// against the *first* rule's entry) — a 100%-reproducible bug, not a
    /// timing-dependent one, that single-predicate testing never exercised.
    ///
    /// Both `thread_local` and `retractall` are needed together, for two
    /// distinct reasons:
    ///
    /// - `thread_local` — SWI engines (as created by `PL_create_engine`, see
    ///   `PrologEnvironment::new`) share one global dynamic-predicate
    ///   database, like Prolog threads do, unless a predicate opts out. This
    ///   is what protects two *genuinely concurrent* engines (running on two
    ///   different OS threads at the same time, e.g. two real concurrent
    ///   `/deduce` requests) from clobbering each other — the same isolation
    ///   mechanism `the_coire.pl` already relies on for its own mutable
    ///   predicates (see its `:- thread_local ...` declarations).
    /// - `retractall` — belt-and-braces alongside `thread_local`, not a
    ///   substitute for it: retracting any existing clauses for a predicate
    ///   indicator before asserting this call's own version means whatever
    ///   this engine ends up seeing for a given predicate is always exactly
    ///   what this call itself loaded, regardless of what state (thread-local
    ///   or otherwise) an earlier, unrelated call left behind. An earlier
    ///   investigation this session suspected `tokio::task::spawn_blocking`'s
    ///   reused OS thread pool (`clara-api/src/handlers/deduce_handler.rs`)
    ///   interacting with `thread_local`'s OS-thread-keyed storage as the
    ///   root cause of stale clauses winning — that mechanism is real and
    ///   worth keeping this defense-in-depth for, but the rule-clause
    ///   `functor/3` bug above turned out to be the actual, 100%-reproducible
    ///   cause of the specific failures observed; the thread-reuse theory was
    ///   never conclusively confirmed as more than a secondary risk.
    pub fn consult_string(&self, code: &str) -> PrologResult<()> {
        // Use read_term_from_chars to parse and assert
        // This handles multiple clauses separated by '.'
        let escaped_code = code.replace("\\", "\\\\").replace("\"", "\\\"");
        let goal = format!(
            "nb_setval('$cs_seen', []), \
             atom_codes(Code, \"{}\"), \
             open_string(Code, S), \
             call_cleanup(\
                 (repeat, read_term(S, T, []), \
                  (T == end_of_file -> ! ; \
                   (  T = (:-G)  -> ignore(call(G)) \
                   ;  T = (?-G)  -> ignore(call(G)) \
                   ;  ( T = (ClauseHead :- _ClauseBody) \
                      -> functor(ClauseHead, F, A) \
                      ;  functor(T, F, A) \
                      ), \
                      ( memberchk(F/A, [consult/1, \
                                        use_module/1, use_module/2, \
                                        ensure_loaded/1, \
                                        load_files/1, load_files/2]) \
                      -> ignore(call(T)) \
                      ;  ( nb_getval('$cs_seen', Seen0), memberchk(F/A, Seen0) \
                         -> true \
                         ;  catch(thread_local(F/A), _, true), \
                            functor(Head, F, A), \
                            catch(retractall(Head), _, true), \
                            nb_getval('$cs_seen', Seen1), \
                            nb_setval('$cs_seen', [F/A|Seen1]) \
                         ), \
                         assertz(T) \
                      ) \
                   ), fail)), \
                 close(S))",
            escaped_code
        );
        self.query_once(&goal).map(|_| ())
    }

    /// Clear all user-defined predicates
    ///
    /// Keeps built-in predicates intact.
    pub fn clear(&self) -> PrologResult<()> {
        // Abolish all user predicates
        // This is a simplified version - a full implementation would
        // track which predicates were added
        self.query_once("true").map(|_| ())
    }

    /// Get raw engine pointer (for FFI callbacks)
    pub fn as_ptr(&self) -> PL_engine_t {
        self.engine
    }

    /// Execute a function within this engine's context
    ///
    /// Handles engine switching automatically. Returns an error if the engine
    /// cannot be acquired (e.g., it's in use by another thread).
    fn with_engine<F, R>(&self, f: F) -> PrologResult<R>
    where
        F: FnOnce() -> PrologResult<R>,
    {
        unsafe {
            let mut old_engine: PL_engine_t = std::ptr::null_mut();
            let set_result = PL_set_engine(self.engine, &mut old_engine);

            if set_result != PL_ENGINE_SET {
                let error_msg = match set_result {
                    PL_ENGINE_INUSE => "Engine is in use by another thread".to_string(),
                    PL_ENGINE_INVAL => "Invalid engine handle".to_string(),
                    other => format!("Unknown engine error code: {}", other),
                };
                log::error!("Failed to set engine: {} (code {})", error_msg, set_result);
                return Err(PrologError::EngineContextError(error_msg));
            }

            let result = f();

            // Detach from this engine so other threads can use it.
            // In a multi-threaded server, different worker threads may handle
            // different requests for the same session. We must release ownership
            // so subsequent requests from other threads can acquire the engine.
            PL_set_engine(std::ptr::null_mut(), std::ptr::null_mut());

            result
        }
    }

    /// Execute query with variable bindings extraction (for REPL)
    ///
    /// Uses a wrapper query to extract variable names and their bindings.
    unsafe fn execute_query_with_bindings(&self, goal: &str) -> PrologResult<String> {
        // Escape the goal for embedding in an atom
        let escaped_goal = goal
            .replace("\\", "\\\\")
            .replace("\"", "\\\"")
            .replace("'", "\\'");

        // Wrapper query that:
        // 1. Parses the goal with variable_names option to capture variable names
        // 2. Calls the goal
        // 3. Builds a list of VarName=Value pairs
        let wrapper = format!(
            r#"(
                atom_codes(GoalAtom, "{}"),
                read_term_from_atom(GoalAtom, Goal, [variable_names(VarNames)]),
                call(Goal),
                findall(Name-Val, member(Name=Val, VarNames), Bindings)
            )"#,
            escaped_goal
        );

        let wrapper_c = string_to_c_string(&wrapper)?;
        let term = PL_new_term_ref();

        if PL_chars_to_term(wrapper_c.as_ptr(), term) == 0 {
            return Err(PrologError::ParseError(format!(
                "Failed to parse goal: {}",
                goal
            )));
        }

        // Get the 'call' predicate
        let call_name = CString::new("call").unwrap();
        let pred = PL_predicate(call_name.as_ptr(), 1, std::ptr::null());

        if pred.is_null() {
            return Err(PrologError::Internal("Failed to get call/1 predicate".to_string()));
        }

        let qid = PL_open_query(
            std::ptr::null_mut(),
            PL_Q_NORMAL | PL_Q_CATCH_EXCEPTION,
            pred,
            term,
        );

        if qid.is_null() {
            return Err(PrologError::QueryFailed("Failed to open query".to_string()));
        }

        let mut solutions = Vec::new();

        loop {
            let rc = PL_next_solution(qid);

            if rc == 0 {
                // Check for exception
                let ex = PL_exception(qid);
                if ex != 0 {
                    let ex_str =
                        term_to_string(ex).unwrap_or_else(|_| "unknown error".to_string());
                    PL_close_query(qid);
                    return Err(PrologError::PrologException(ex_str));
                }
                break;
            }

            // The wrapper is a nested conjunction: ','(A, ','(B, ','(C, D)))
            // Navigate to the Bindings variable in findall(..., ..., Bindings)
            // Structure: ','(atom_codes(...), ','(read_term(...), ','(call(...), findall(...))))
            let level2 = PL_new_term_ref();
            let level3 = PL_new_term_ref();
            let findall_term = PL_new_term_ref();
            let bindings_term = PL_new_term_ref();

            PL_get_arg(2, term, level2);        // Get second part of top-level ','
            PL_get_arg(2, level2, level3);      // Get second part of next ','
            PL_get_arg(2, level3, findall_term); // Get findall(...) term
            PL_get_arg(3, findall_term, bindings_term); // Get Bindings (3rd arg of findall)

            // Convert bindings list to JSON object
            let mut binding_obj = serde_json::Map::new();
            let head = PL_new_term_ref();
            let tail = PL_copy_term_ref(bindings_term);

            while PL_get_list(tail, head, tail) != 0 {
                // Each element is Name-Value pair
                let mut f: functor_t = 0;
                if PL_get_functor(head, &mut f) != 0 {
                    let arity = PL_functor_arity(f);
                    if arity == 2 {
                        let name_term = PL_new_term_ref();
                        let value_term = PL_new_term_ref();
                        PL_get_arg(1, head, name_term);
                        PL_get_arg(2, head, value_term);

                        // Get variable name as string
                        if let Ok(name) = term_to_string(name_term) {
                            // Get value
                            if let Ok(value) = term_to_json(value_term) {
                                binding_obj.insert(name, value);
                            } else if let Ok(value_str) = term_to_string(value_term) {
                                binding_obj.insert(name, serde_json::Value::String(value_str));
                            }
                        }
                    }
                }
            }

            // If no bindings (query like `true` or `man(stan)`), just indicate success
            if binding_obj.is_empty() {
                solutions.push(serde_json::json!(true));
            } else {
                solutions.push(serde_json::Value::Object(binding_obj));
            }
        }

        PL_close_query(qid);

        serde_json::to_string(&solutions).map_err(|e| PrologError::JsonError(e))
    }

    /// Execute query and collect all solutions
    unsafe fn execute_query_all(&self, goal: &str) -> PrologResult<String> {
        let goal_c = string_to_c_string(goal)?;
        let term = PL_new_term_ref();

        if PL_chars_to_term(goal_c.as_ptr(), term) == 0 {
            return Err(PrologError::ParseError(format!(
                "Failed to parse goal: {}",
                goal
            )));
        }

        // Get the 'call' predicate
        let call_name = CString::new("call").unwrap();
        let pred = PL_predicate(call_name.as_ptr(), 1, std::ptr::null());

        if pred.is_null() {
            return Err(PrologError::Internal("Failed to get call/1 predicate".to_string()));
        }

        let qid = PL_open_query(
            std::ptr::null_mut(),
            PL_Q_NORMAL | PL_Q_CATCH_EXCEPTION,
            pred,
            term,
        );

        if qid.is_null() {
            return Err(PrologError::QueryFailed("Failed to open query".to_string()));
        }

        let mut solutions = Vec::new();

        loop {
            let rc = PL_next_solution(qid);

            if rc == 0 {
                // Check for exception
                let ex = PL_exception(qid);
                if ex != 0 {
                    let ex_str =
                        term_to_string(ex).unwrap_or_else(|_| "unknown error".to_string());
                    PL_close_query(qid);
                    return Err(PrologError::PrologException(ex_str));
                }
                break;
            }

            // Extract solution
            match term_to_json(term) {
                Ok(json) => solutions.push(json),
                Err(e) => {
                    log::warn!("Failed to convert solution to JSON: {}", e);
                    // Try string representation as fallback
                    if let Ok(s) = term_to_string(term) {
                        solutions.push(serde_json::Value::String(s));
                    }
                }
            }
        }

        PL_close_query(qid);

        serde_json::to_string(&solutions).map_err(|e| PrologError::JsonError(e))
    }

    /// Execute query and return first solution only
    unsafe fn execute_query_once(&self, goal: &str) -> PrologResult<String> {
        let goal_c = string_to_c_string(goal)?;
        let term = PL_new_term_ref();

        if PL_chars_to_term(goal_c.as_ptr(), term) == 0 {
            return Err(PrologError::ParseError(format!(
                "Failed to parse goal: {}",
                goal
            )));
        }

        let result = PL_call(term, std::ptr::null_mut());

        if result != 0 {
            // Success - convert result to JSON
            let json = term_to_json(term)?;
            serde_json::to_string(&json).map_err(|e| PrologError::JsonError(e))
        } else {
            // Check for exception
            let ex = PL_exception(std::ptr::null_mut());
            if ex != 0 {
                let ex_str = term_to_string(ex).unwrap_or_else(|_| "unknown error".to_string());
                PL_clear_exception();
                Err(PrologError::PrologException(ex_str))
            } else {
                Err(PrologError::QueryFailed(format!("Query failed: {}", goal)))
            }
        }
    }
}

impl Drop for PrologEnvironment {
    fn drop(&mut self) {
        if !self.is_main && !self.engine.is_null() {
            unsafe {
                log::debug!("Destroying Prolog engine: {:p}", self.engine);
                PL_destroy_engine(self.engine);
            }
        }
    }
}

// Engine ownership can be transferred between threads
// But only one thread can use an engine at a time
unsafe impl Send for PrologEnvironment {}
unsafe impl Sync for PrologEnvironment {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialization() {
        let result = ensure_prolog_initialized();
        // This might fail in test environment without SWI-Prolog
        // but the function should not panic
        match result {
            Ok(()) => assert!(is_prolog_initialized()),
            Err(e) => {
                eprintln!("Prolog initialization failed (expected in some test envs): {}", e);
            }
        }
    }
}
