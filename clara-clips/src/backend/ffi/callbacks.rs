// FFI callbacks exposed to CLIPS C code

use clara_toolbox::{ToolboxManager, ToolRequest, ToolResponse};
use libc::{c_char, c_void};
use serde_json::json;
use std::ffi::{CStr, CString};

/// Main callback function invoked from CLIPS when (clara-evaluate ...) is called
///
/// This function receives a JSON string from CLIPS, processes it, and returns a JSON response.
/// Memory allocated for the returned string must be freed by calling rust_free_string.
///
/// # Safety
/// This function is unsafe because it:
/// - Dereferences raw pointers from C
/// - Allocates memory that must be freed by the caller
///
/// # Arguments
/// * `_env` - Pointer to CLIPS environment (unused for now)
/// * `input_json` - C string containing JSON tool request
///
/// # Returns
/// Pointer to C string containing JSON response (must be freed with rust_free_string)
#[no_mangle]
pub extern "C" fn rust_clara_evaluate(
    _env: *mut c_void,
    input_json: *const c_char,
) -> *mut c_char {
    // For Phase 2: Return hardcoded JSON response for testing
    // In Phase 3, this will be wired to ToolboxManager

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

        // Parse the JSON request
        let request: ToolRequest = match serde_json::from_str(input_str) {
            Ok(req) => req,
            Err(e) => {
                log::error!("Failed to parse tool request: {}", e);
                return CString::new(format!(
                    "{{\"status\":\"error\",\"message\":\"Invalid JSON: {}\"}}",
                    e
                ))
                .unwrap_or_else(|_| CString::new("{}").unwrap())
                .into_raw();
            }
        };

        // Execute the tool via ToolboxManager
        let manager = ToolboxManager::global().lock().unwrap();
        let response = manager.execute_tool(&request).unwrap_or_else(|e| {
            log::error!("Tool execution error: {}", e);
            ToolResponse::error(format!("{}", e))
        });

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
/// This function MUST be called from C to free strings returned by rust_clara_evaluate.
/// Failing to call this will cause memory leaks.
///
/// # Safety
/// This function is unsafe because it:
/// - Takes ownership of a raw pointer and frees it
/// - Must only be called once per pointer
/// - Must only be called with pointers allocated by rust_clara_evaluate
///
/// # Arguments
/// * `s` - Pointer to C string allocated by rust_clara_evaluate
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_rust_clara_evaluate_basic() {
        let input = CString::new(r#"{"tool":"echo","arguments":{"message":"hello"}}"#).unwrap();

        unsafe {
            let result_ptr = rust_clara_evaluate(std::ptr::null_mut(), input.as_ptr());
            assert!(!result_ptr.is_null(), "Should return non-null pointer");

            let result_cstr = CStr::from_ptr(result_ptr);
            let result_str = result_cstr.to_str().unwrap();

            assert!(result_str.contains("success"), "Should contain success status");
            assert!(result_str.contains("phase_2_testing"), "Should indicate phase 2");

            // Clean up
            rust_free_string(result_ptr);
        }
    }

    #[test]
    fn test_rust_clara_evaluate_null_input() {
        unsafe {
            let result_ptr = rust_clara_evaluate(std::ptr::null_mut(), std::ptr::null());
            assert!(!result_ptr.is_null(), "Should return non-null pointer even with null input");

            // Clean up
            rust_free_string(result_ptr);
        }
    }

    #[test]
    fn test_rust_free_string_null() {
        // Should not crash
        rust_free_string(std::ptr::null_mut());
    }

    #[test]
    fn test_memory_round_trip() {
        let input = CString::new(r#"{"test":"data"}"#).unwrap();

        unsafe {
            // Allocate
            let result_ptr = rust_clara_evaluate(std::ptr::null_mut(), input.as_ptr());

            // Use
            let _ = CStr::from_ptr(result_ptr).to_str().unwrap();

            // Free
            rust_free_string(result_ptr);

            // Success if no crash/leak (run with valgrind to verify)
        }
    }
}
