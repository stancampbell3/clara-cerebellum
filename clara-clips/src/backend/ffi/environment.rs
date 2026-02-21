// Safe Rust wrapper around CLIPS Environment

use super::bindings::{self, CLIPSValue, Environment, EvalError};
use std::ffi::{CStr, CString};
use libc::c_void;
use uuid::Uuid;

/// Safe wrapper around a CLIPS Environment
pub struct ClipsEnvironment {
    env: *mut Environment,
    session_id: Uuid,
}

/// Router callback to determine if we should capture output from this logical name
extern "C" fn capture_query(
    _env: *mut Environment,
    logical_name: *const libc::c_char,
    _context: *mut c_void,
) -> bool {
    unsafe {
        let name = CStr::from_ptr(logical_name).to_str().unwrap_or("");
        // Capture output from stdout, stderr, and general output
        name == "stdout" || name == "stderr" || name == "t" || name == "werror"
    }
}

/// Router callback to capture written output
extern "C" fn capture_write(
    _env: *mut Environment,
    _logical_name: *const libc::c_char,
    data: *const libc::c_char,
    context: *mut c_void,
) {
    unsafe {
        if context.is_null() {
            return;
        }
        let buffer = &mut *(context as *mut String);
        let text = CStr::from_ptr(data).to_str().unwrap_or("");
        buffer.push_str(text);
    }
}

impl std::fmt::Debug for ClipsEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClipsEnvironment")
            .field("env", &format!("{:p}", self.env))
            .field("session_id", &self.session_id)
            .finish()
    }
}

impl ClipsEnvironment {
    /// Create a new CLIPS environment with its own Coire session UUID.
    ///
    /// Automatically loads `the_coire.clp` constructs and seeds the
    /// `?*coire-session-id*` defglobal so that publish functions work
    /// without any additional setup.
    pub fn new() -> Result<Self, String> {
        let session_id = Uuid::new_v4();

        let env = unsafe {
            let e = bindings::CreateEnvironment();
            if e.is_null() {
                return Err("Failed to create CLIPS environment".to_string());
            }
            e
        };

        let mut ce = Self { env, session_id };

        // Load the_coire.clp constructs (defglobal, deftemplate, deffunction)
        ce.load_coire_library()?;

        // Seed the session global so (coire-publish ...) knows which mailbox to use
        ce.eval(&format!("(bind ?*coire-session-id* \"{}\")", session_id))?;

        Ok(ce)
    }

    /// Return this environment's Coire session UUID.
    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    /// Evaluate a CLIPS expression and return the result as a string
    pub fn eval(&mut self, code: &str) -> Result<String, String> {
        unsafe {
            let c_code = CString::new(code)
                .map_err(|e| format!("Invalid code string: {}", e))?;

            // Create output buffer
            let mut output = String::new();
            let output_ptr = &mut output as *mut String as *mut c_void;

            // Register router to capture output
            let router_name = CString::new("rust-capture").unwrap();
            let router_added = bindings::AddRouter(
                self.env,
                router_name.as_ptr(),
                10, // Priority (higher than default routers)
                Some(capture_query),
                Some(capture_write),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                output_ptr,
            );

            if !router_added {
                return Err("Failed to add output capture router".to_string());
            }

            // Evaluate the expression
            let mut result: CLIPSValue = std::mem::zeroed();
            let eval_result = bindings::Eval(self.env, c_code.as_ptr(), &mut result);

            // If eval succeeded and no output was captured, write the result value
            if matches!(eval_result, EvalError::EE_NO_ERROR) && output.is_empty() {
                let stdout_name = CString::new("stdout").unwrap();
                bindings::WriteCLIPSValue(self.env, stdout_name.as_ptr(), &mut result);
            }

            // Clean up router
            bindings::DeleteRouter(self.env, router_name.as_ptr());

            match eval_result {
                EvalError::EE_NO_ERROR => {
                    // Return captured output, or empty string if nothing was captured
                    Ok(output)
                }
                EvalError::EE_PARSING_ERROR => {
                    Err(format!("CLIPS parsing error: {}", output))
                }
                EvalError::EE_PROCESSING_ERROR => {
                    Err(format!("CLIPS processing error: {}", output))
                }
            }
        }
    }

    /// Build (compile) a single CLIPS construct definition into this environment.
    ///
    /// Handles `defglobal`, `deftemplate`, `deffunction`, `defrule`, etc.
    /// Use [`eval`] for expressions like `(assert ...)` or `(run)`.
    pub fn build(&mut self, construct: &str) -> Result<(), String> {
        unsafe {
            let c_str = CString::new(construct)
                .map_err(|e| format!("Invalid construct string: {}", e))?;
            // Build returns BuildError: 0 = BE_NO_ERROR (success), non-zero = failure
            let result = bindings::Build(self.env, c_str.as_ptr());
            if result == 0 {
                Ok(())
            } else {
                let preview = &construct[..construct.len().min(80)];
                Err(format!("CLIPS Build failed (code {}) for: {}", result, preview))
            }
        }
    }

    /// Load the `the_coire.clp` library constructs into this environment.
    ///
    /// Called automatically by [`new`]. Safe to call again after [`clear`]
    /// to restore the event API. Each call re-builds all constructs.
    pub fn load_coire_library(&mut self) -> Result<(), String> {
        let source = include_str!("../../../clp-lib/the_coire.clp");
        for construct in split_clips_constructs(source) {
            self.build(&construct)
                .map_err(|e| format!("load_coire_library: {}", e))?;
        }
        log::debug!("the_coire CLIPS library loaded for session {}", self.session_id);
        Ok(())
    }

    /// Poll the Coire mailbox and dispatch all pending events into this environment.
    ///
    /// Dispatch rules:
    /// - `"assert"` events → `(assert <data>)` — data must be valid CLIPS fact syntax
    /// - `"goal"` events   → `<data>` is eval'd directly as a CLIPS expression
    /// - all other types   → asserted as `(coire-event ...)` template facts, then `(run)`
    ///
    /// Returns the number of events that were pending (and dispatched).
    pub fn consume_coire_events(&mut self) -> Result<usize, String> {
        let before = clara_coire::global()
            .count_pending(self.session_id)
            .map_err(|e| e.to_string())?;

        let events = clara_coire::global()
            .poll_pending(self.session_id)
            .map_err(|e| e.to_string())?;

        for event in events {
            let ev_type = event.payload
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let data = event.payload
                .get("data")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            match ev_type.as_str() {
                "assert" => {
                    if let Err(e) = self.eval(&format!("(assert {})", data)) {
                        log::warn!("consume_coire_events: assert failed: {}", e);
                    }
                }
                "goal" => {
                    if let Err(e) = self.eval(&data) {
                        log::warn!("consume_coire_events: goal eval failed: {}", e);
                    }
                }
                _ => {
                    // Assert as (coire-event ...) template fact for rule-based dispatch
                    let escaped = data.replace('\\', "\\\\").replace('"', "\\\"");
                    let assert_str = format!(
                        r#"(assert (coire-event (event-id "{}") (origin "{}") (ev-type "{}") (data "{}")))"#,
                        event.event_id, event.origin, ev_type, escaped
                    );
                    if let Err(e) = self.eval(&assert_str) {
                        log::warn!("consume_coire_events: coire-event assert failed: {}", e);
                    } else {
                        self.eval("(run)").ok();
                    }
                }
            }
        }

        Ok(before)
    }

    /// Reset the CLIPS environment
    pub fn reset(&mut self) -> Result<(), String> {
        unsafe {
            bindings::Reset(self.env);
            Ok(())
        }
    }

    /// Load a CLIPS file
    pub fn load(&mut self, path: &str) -> Result<(), String> {
        unsafe {
            let c_path = CString::new(path)
                .map_err(|e| format!("Invalid path string: {}", e))?;

            let result = bindings::Load(self.env, c_path.as_ptr());
            if result != 0 {
                Err(format!("Failed to load file: {}", path))
            } else {
                Ok(())
            }
        }
    }

    /// Clear the CLIPS environment
    pub fn clear(&mut self) -> Result<(), String> {
        unsafe {
            bindings::Clear(self.env);
            Ok(())
        }
    }

    /// Get raw environment pointer (for advanced use cases)
    pub fn as_ptr(&self) -> *mut Environment {
        self.env
    }
}

impl Drop for ClipsEnvironment {
    fn drop(&mut self) {
        if !self.env.is_null() {
            unsafe {
                bindings::DestroyEnvironment(self.env);
            }
        }
    }
}

// Environment is not thread-safe by default in CLIPS
// We're assuming single-threaded access per environment (protected by RwLock in SessionManager)
unsafe impl Send for ClipsEnvironment {}
unsafe impl Sync for ClipsEnvironment {}

/// Parse a CLIPS source string into individual top-level construct strings.
///
/// Handles:
/// - `;` line comments
/// - quoted strings (skips content, handles `\"` escapes)
/// - balanced parentheses to find construct boundaries
fn split_clips_constructs(source: &str) -> Vec<String> {
    let mut constructs = Vec::new();
    let mut depth: i32 = 0;
    let mut start: Option<usize> = None;
    let mut in_string = false;
    let mut in_comment = false;
    let bytes = source.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];

        if in_comment {
            if b == b'\n' {
                in_comment = false;
            }
            i += 1;
            continue;
        }

        if in_string {
            match b {
                b'\\' if i + 1 < bytes.len() => {
                    // skip escaped character
                    i += 2;
                    continue;
                }
                b'"' => {
                    in_string = false;
                }
                _ => {}
            }
            i += 1;
            continue;
        }

        match b {
            b';' => {
                in_comment = true;
            }
            b'"' => {
                in_string = true;
            }
            b'(' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start.take() {
                        let construct = source[s..=i].trim().to_string();
                        if !construct.is_empty() {
                            constructs.push(construct);
                        }
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }

    constructs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_destroy() {
        let result = ClipsEnvironment::new();
        assert!(result.is_ok(), "Should create environment successfully");
        // Drop will be called automatically
    }

    #[test]
    fn test_basic_eval() {
        let mut env = ClipsEnvironment::new().expect("Failed to create environment");
        let result = env.eval("(+ 1 2)");
        assert!(result.is_ok(), "Should evaluate simple expression");
    }

    #[test]
    fn test_reset() {
        let mut env = ClipsEnvironment::new().expect("Failed to create environment");
        let result = env.reset();
        assert!(result.is_ok(), "Should reset environment successfully");
    }

    #[test]
    fn test_clear() {
        let mut env = ClipsEnvironment::new().expect("Failed to create environment");
        let result = env.clear();
        assert!(result.is_ok(), "Should clear environment successfully");
    }

    #[test]
    fn test_session_id_set() {
        let env = ClipsEnvironment::new().expect("Failed to create environment");
        let id = env.session_id();
        assert_ne!(id, Uuid::nil(), "Session ID should not be nil");
    }

    #[test]
    fn test_split_clips_constructs() {
        let src = r#"
; a comment
(defglobal ?*foo* = "")
; another comment
(deffunction bar () (+ 1 2))
"#;
        let constructs = split_clips_constructs(src);
        assert_eq!(constructs.len(), 2);
        assert!(constructs[0].starts_with("(defglobal"));
        assert!(constructs[1].starts_with("(deffunction"));
    }

    #[test]
    fn test_clara_evaluate_callback() {
        // Initialize the global ToolboxManager
        clara_toolbox::ToolboxManager::init_global();

        // Test that the clara-evaluate function is registered and callable
        let mut env = ClipsEnvironment::new().expect("Failed to create environment");

        // Call the registered clara-evaluate function with echo tool
        let result = env.eval(r#"(clara-evaluate "{\"tool\":\"echo\",\"arguments\":{\"message\":\"test\"}}")"#);

        assert!(result.is_ok(), "Should evaluate clara-evaluate successfully");
        // The result should contain the JSON response from the Rust callback
        let output = result.unwrap();
        println!("Callback output: {}", output);

        // Output should contain the expression (not the actual return value, that's a TODO)
        // But if the callback is working, it won't error
    }
}
