//! FFI callbacks exposed to Prolog code
//!
//! This module provides the `clara_evaluate/2` predicate that Prolog code
//! can call to invoke Rust tools via the ToolboxManager.
//!
//! The core evaluation logic is in clara-toolbox::ffi to avoid duplicate symbols
//! when multiple crates (clara-clips, clara-prolog) are linked together.

use super::bindings::*;
use clara_toolbox::ffi::{evaluate_json_string, free_c_string};
use libc::{c_char, c_int};
use std::ffi::{CStr, CString};

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

        // Convert to Rust string for the shared evaluator
        let input_str = match CStr::from_ptr(input_ptr).to_str() {
            Ok(s) => s,
            Err(e) => {
                log::error!("clara_evaluate/2: Invalid UTF-8 in input: {}", e);
                return 0;
            }
        };

        // Call the shared evaluation function from clara-toolbox
        log::debug!("pl_clara_evaluate/2: Calling evaluate_json_string");
        let result_ptr = evaluate_json_string(input_str);

        if result_ptr.is_null() {
            log::error!("clara_evaluate/2: evaluate_json_string returned null");
            return 0;
        }

        // Unify result with second argument
        let success = PL_unify_string_chars(t1, result_ptr);

        // Free the result string
        free_c_string(result_ptr);

        if success != 0 {
            1
        } else {
            log::error!("clara_evaluate/2: Failed to unify result");
            0
        }
    }
}

/// Track whether clara_evaluate/2 has been registered
static CLARA_EVALUATE_REGISTERED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Register the clara_evaluate/2 predicate with Prolog
///
/// This can be called multiple times safely - subsequent calls will return
/// the cached registration result without re-registering.
///
/// # Returns
/// true if registration succeeded (or was already registered), false otherwise
pub fn register_clara_evaluate() -> bool {
    *CLARA_EVALUATE_REGISTERED.get_or_init(|| {
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clara_toolbox::ToolboxManager;

    #[test]
    fn test_evaluate_json_string_null_input() {
        let result_ptr = evaluate_json_string("");
        assert!(!result_ptr.is_null(), "Should return non-null pointer even with empty input");

        // Clean up
        free_c_string(result_ptr);
    }

    #[test]
    fn test_free_c_string_null() {
        // Should not crash
        free_c_string(std::ptr::null_mut());
    }

    #[test]
    fn test_evaluate_json_string_invalid_json() {
        let result_ptr = evaluate_json_string("not valid json");
        assert!(!result_ptr.is_null());

        unsafe {
            let result_str = CStr::from_ptr(result_ptr).to_str().unwrap();
            assert!(result_str.contains("error"));
            assert!(result_str.contains("Invalid JSON"));
        }

        free_c_string(result_ptr);
    }

    #[test]
    fn test_evaluate_json_string_with_toolbox() {
        // Initialize toolbox with echo tool
        ToolboxManager::init_global();

        let result_ptr = evaluate_json_string(r#"{"tool":"echo","arguments":{"message":"test"}}"#);
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

        free_c_string(result_ptr);
    }
}
