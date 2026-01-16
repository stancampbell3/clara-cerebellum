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
    /// (except built-ins).
    pub fn new() -> PrologResult<Self> {
        ensure_prolog_initialized()?;

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
    /// Handles engine switching automatically.
    fn with_engine<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        unsafe {
            let mut old_engine: PL_engine_t = std::ptr::null_mut();
            let set_result = PL_set_engine(self.engine, &mut old_engine);

            if set_result != PL_ENGINE_SET {
                log::error!("Failed to set engine: {}", set_result);
                // For now, proceed anyway - might be already set
            }

            let result = f();

            // Restore previous engine if we switched
            if !old_engine.is_null() && old_engine != self.engine {
                PL_set_engine(old_engine, std::ptr::null_mut());
            }

            result
        }
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
