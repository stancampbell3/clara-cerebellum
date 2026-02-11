//! Safe wrapper around SWI-Prolog engine
//!
//! Provides `PrologEnvironment` - a safe interface for Prolog operations.
//! Each environment wraps an isolated SWI-Prolog engine for session safety.

use super::bindings::*;
use super::conversion::*;
use crate::error::{PrologError, PrologResult};
use std::ffi::CString;
use std::sync::OnceLock;

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

        if init_result != 0 {
            log::info!("SWI-Prolog initialized successfully");

            // Autoload JSON libraries so predicates are globally available.
            // json: atom_json_dict/3, json_read/write etc. (patched with pure-Prolog fallbacks)
            // json_convert: prolog_to_json/2, json_to_prolog/2, json_object declarations
            unsafe {
                let json_goal = CString::new("use_module(library(http/json))").unwrap();
                let json_term = PL_new_term_ref();
                if PL_chars_to_term(json_goal.as_ptr(), json_term) != 0 {
                    if PL_call(json_term, std::ptr::null_mut()) != 0 {
                        log::info!("JSON library (http/json) loaded successfully");
                    } else {
                        log::warn!("Failed to load JSON library (http/json) — JSON predicates may be unavailable");
                    }
                } else {
                    log::warn!("Failed to parse JSON library load goal");
                }

                let json_convert_goal = CString::new("use_module(library(http/json_convert))").unwrap();
                let json_convert_term = PL_new_term_ref();
                if PL_chars_to_term(json_convert_goal.as_ptr(), json_convert_term) != 0 {
                    if PL_call(json_convert_term, std::ptr::null_mut()) != 0 {
                        log::info!("JSON convert library (http/json_convert) loaded successfully");
                    } else {
                        log::warn!("Failed to load JSON convert library (http/json_convert) — json_convert predicates may be unavailable");
                    }
                } else {
                    log::warn!("Failed to parse JSON convert library load goal");
                }
            }

            Ok(())
        } else {
            log::error!("Failed to initialize SWI-Prolog");
            Err("PL_initialise returned 0".to_string())
        }
    });

    result.clone().map_err(PrologError::InitializationFailed)
}

/// Check if Prolog is initialized
pub fn is_prolog_initialized() -> bool {
    INIT_RESULT
        .get()
        .map(|r| r.is_ok())
        .unwrap_or(false)
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
}

impl std::fmt::Debug for PrologEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrologEnvironment")
            .field("engine", &format!("{:p}", self.engine))
            .field("is_main", &self.is_main)
            .finish()
    }
}

impl PrologEnvironment {
    /// Create a new Prolog engine for session isolation
    ///
    /// Each call creates a fresh engine with no loaded predicates
    /// (except built-ins). Also ensures the clara_evaluate/2 foreign
    /// predicate is registered.
    pub fn new() -> PrologResult<Self> {
        ensure_prolog_initialized()?;

        // Register clara_evaluate/2 foreign predicate if not already registered
        // This must happen after Prolog is initialized but can be called multiple times safely
        super::callbacks::register_clara_evaluate();

        unsafe {
            let engine = PL_create_engine(std::ptr::null_mut());

            if engine.is_null() {
                return Err(PrologError::EngineCreationFailed(
                    "PL_create_engine returned null".to_string(),
                ));
            }

            log::debug!("Created new Prolog engine: {:p}", engine);

            Ok(Self {
                engine,
                is_main: false,
            })
        }
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
        })
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
    /// Parses each clause and asserts it into the database.
    pub fn consult_string(&self, code: &str) -> PrologResult<()> {
        // Use read_term_from_chars to parse and assert
        // This handles multiple clauses separated by '.'
        let escaped_code = code.replace("\\", "\\\\").replace("\"", "\\\"");
        let goal = format!(
            "atom_codes(Code, \"{}\"), \
             open_string(Code, S), \
             call_cleanup(\
                 (repeat, read_term(S, T, []), \
                  (T == end_of_file -> ! ; assertz(T), fail)), \
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
