//! FFI callbacks exposed to Prolog code
//!
//! This module provides the `clara_evaluate/2` predicate that Prolog code
//! can call to invoke Rust tools via the ToolboxManager.

use super::bindings::*;
use clara_toolbox::{ToolboxManager, ToolRequest, ToolResponse};
use libc::{c_char, c_int, c_void};
use serde_json::json;
use std::ffi::{CStr, CString};

/// Main callback function for external use (compatible with CLIPS pattern)
///
/// This function receives a JSON string, processes it, and returns a JSON response.
/// Memory allocated for the returned string must be freed by calling rust_free_string.
///
/// # Safety
/// This function is unsafe because it:
/// - Dereferences raw pointers from C
/// - Allocates memory that must be freed by the caller
///
/// # Arguments
/// * `_env` - Pointer to environment (unused, for API compatibility)
/// * `input_json` - C string containing JSON tool request
///
/// # Returns
/// Pointer to C string containing JSON response (must be freed with rust_free_string)
#[no_mangle]
pub extern "C" fn rust_clara_evaluate(
    _env: *mut c_void,
    input_json: *const c_char,
) -> *mut c_char {
    unsafe {
        // Convert C string to Rust string
        let input_str = if input_json.is_null() {
            log::warn!("rust_clara_evaluate called with NULL input");
            ""
        } else {
            match CStr::from_ptr(input_json).to_str() {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Invalid UTF-8 in input: {}", e);
                    ""
                }
            }
        };

        log::debug!("rust_clara_evaluate called with input: {}", input_str);

        // Parse the JSON input
        let json_value: serde_json::Value = match serde_json::from_str(input_str) {
            Ok(val) => val,
            Err(e) => {
                log::error!("Failed to parse JSON: {}", e);
                return CString::new(format!(
                    "{{\"status\":\"error\",\"message\":\"Invalid JSON: {}\"}}",
                    e
                ))
                .unwrap_or_else(|_| CString::new("{}").unwrap())
                .into_raw();
            }
        };

        // Execute via ToolboxManager
        let manager = ToolboxManager::global().lock().unwrap();

        let response = if json_value.get("tool").is_some() {
            // Explicit tool specified - parse as ToolRequest and execute
            match serde_json::from_value::<ToolRequest>(json_value) {
                Ok(request) => manager.execute_tool(&request).unwrap_or_else(|e| {
                    log::error!("Tool execution error: {}", e);
                    ToolResponse::error(format!("{}", e))
                }),
                Err(e) => {
                    log::error!("Failed to parse ToolRequest: {}", e);
                    ToolResponse::error(format!("Invalid tool request: {}", e))
                }
            }
        } else {
            // No tool specified - use default evaluator with entire JSON as arguments
            log::debug!("No tool specified, using default evaluator");
            manager.evaluate(json_value).unwrap_or_else(|e| {
                log::error!("Default evaluator error: {}", e);
                ToolResponse::error(format!("{}", e))
            })
        };

        let response_str = serde_json::to_string(&response).unwrap();

        // Convert Rust string to C string
        match CString::new(response_str) {
            Ok(c_string) => {
                log::debug!("rust_clara_evaluate returning response");
                c_string.into_raw()
            }
            Err(e) => {
                log::error!("Failed to create C string: {}", e);
                // Return error JSON
                let error_response = json!({
                    "status": "error",
                    "message": format!("Failed to create response: {}", e)
                });
                CString::new(error_response.to_string())
                    .unwrap_or_else(|_| CString::new("{}").unwrap())
                    .into_raw()
            }
        }
    }
}

/// Free a string allocated by Rust
///
/// This function MUST be called from C/Prolog to free strings returned by rust_clara_evaluate.
/// Failing to call this will cause memory leaks.
///
/// # Safety
/// This function is unsafe because it:
/// - Takes ownership of a raw pointer and frees it
/// - Must only be called once per pointer
/// - Must only be called with pointers allocated by rust_clara_evaluate
#[no_mangle]
pub extern "C" fn rust_free_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }

    unsafe {
        // Take ownership and drop
        let _ = CString::from_raw(s);
    }
}

/// Foreign predicate implementation for clara_evaluate/2
///
/// Called from Prolog as:
/// ```prolog
/// ?- clara_evaluate('{"tool":"echo","arguments":{"msg":"hi"}}', Result).
/// ```
///
/// # Arguments
/// * `t0` - Input term (should be atom or string containing JSON)
/// * `t1` - Output term (will be unified with result JSON string)
///
/// # Returns
/// * true (non-zero) on success
/// * false (0) on failure
#[no_mangle]
pub extern "C" fn pl_clara_evaluate(t0: term_t, t1: term_t) -> c_int {
    unsafe {
        // Get input string from first argument
        let mut input_ptr: *mut c_char = std::ptr::null_mut();
        let flags = CVT_ATOM | CVT_STRING | BUF_STACK | REP_UTF8;

        if PL_get_chars(t0, &mut input_ptr, flags) == 0 {
            log::error!("clara_evaluate/2: Failed to get input string from term");
            return 0;
        }

        if input_ptr.is_null() {
            log::error!("clara_evaluate/2: Input string is null");
            return 0;
        }

        // Call the evaluation function
        let result_ptr = rust_clara_evaluate(std::ptr::null_mut(), input_ptr);

        if result_ptr.is_null() {
            log::error!("clara_evaluate/2: rust_clara_evaluate returned null");
            return 0;
        }

        // Unify result with second argument
        let success = PL_unify_string_chars(t1, result_ptr);

        // Free the result string
        rust_free_string(result_ptr);

        if success != 0 {
            1
        } else {
            log::error!("clara_evaluate/2: Failed to unify result");
            0
        }
    }
}

/// Register the clara_evaluate/2 predicate with Prolog
///
/// Call this once after Prolog initialization to make clara_evaluate/2
/// available to Prolog code.
///
/// # Returns
/// true if registration succeeded, false otherwise
pub fn register_clara_evaluate() -> bool {
    unsafe {
        let name = match CString::new("clara_evaluate") {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to create predicate name: {}", e);
                return false;
            }
        };

        let result = PL_register_foreign(
            name.as_ptr(),
            2, // arity
            pl_clara_evaluate as pl_function_t,
            0, // flags (deterministic)
        );

        if result != 0 {
            log::info!("Registered clara_evaluate/2 predicate");
            true
        } else {
            log::error!("Failed to register clara_evaluate/2");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_rust_clara_evaluate_null_input() {
        let result_ptr = rust_clara_evaluate(std::ptr::null_mut(), std::ptr::null());
        assert!(!result_ptr.is_null(), "Should return non-null pointer even with null input");

        // Clean up
        rust_free_string(result_ptr);
    }

    #[test]
    fn test_rust_free_string_null() {
        // Should not crash
        rust_free_string(std::ptr::null_mut());
    }

    #[test]
    fn test_rust_clara_evaluate_invalid_json() {
        let input = CString::new("not valid json").unwrap();

        let result_ptr = rust_clara_evaluate(std::ptr::null_mut(), input.as_ptr());
        assert!(!result_ptr.is_null());

        unsafe {
            let result_str = CStr::from_ptr(result_ptr).to_str().unwrap();
            assert!(result_str.contains("error"));
            assert!(result_str.contains("Invalid JSON"));
        }

        rust_free_string(result_ptr);
    }

    #[test]
    fn test_rust_clara_evaluate_with_toolbox() {
        // Initialize toolbox with echo tool
        ToolboxManager::init_global();

        let input = CString::new(r#"{"tool":"echo","arguments":{"message":"test"}}"#).unwrap();

        let result_ptr = rust_clara_evaluate(std::ptr::null_mut(), input.as_ptr());
        assert!(!result_ptr.is_null());

        unsafe {
            let result_str = CStr::from_ptr(result_ptr).to_str().unwrap();
            // Should contain success and echoed message
            assert!(
                result_str.contains("success"),
                "Expected success, got: {}",
                result_str
            );
        }

        rust_free_string(result_ptr);
    }
}
