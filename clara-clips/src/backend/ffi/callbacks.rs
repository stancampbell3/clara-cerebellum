// FFI callbacks exposed to CLIPS C code
//
// This module re-exports the shared FFI functions from clara-toolbox.
// The actual implementation is in clara-toolbox::ffi to avoid duplicate symbols
// when multiple crates (clara-clips, clara-prolog) are linked together.

// Re-export the FFI functions from clara-toolbox
// Note: rust_clara_evaluate and rust_free_string are defined with #[no_mangle]
// in clara-toolbox when the "ffi" feature is enabled.

pub use clara_toolbox::ffi::{evaluate_json_string, free_c_string};

#[cfg(test)]
mod tests {
    use super::*;
    use clara_toolbox::ToolboxManager;
    use std::ffi::CStr;

    #[test]
    fn test_evaluate_json_string_basic() {
        // Initialize global toolbox with default tools (including echo)
        ToolboxManager::init_global();

        let result_ptr = evaluate_json_string(r#"{"tool":"echo","arguments":{"message":"hello"}}"#);
        assert!(!result_ptr.is_null(), "Should return non-null pointer");

        unsafe {
            let result_cstr = CStr::from_ptr(result_ptr);
            let result_str = result_cstr.to_str().unwrap();

            assert!(
                result_str.contains("success"),
                "Should contain success status, got: {}",
                result_str
            );
            assert!(
                result_str.contains("echoed"),
                "Should contain echoed field from EchoTool, got: {}",
                result_str
            );
        }

        // Clean up
        free_c_string(result_ptr);
    }

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
    fn test_memory_round_trip() {
        // Allocate
        let result_ptr = evaluate_json_string(r#"{"test":"data"}"#);

        // Use
        unsafe {
            let _ = CStr::from_ptr(result_ptr).to_str().unwrap();
        }

        // Free
        free_c_string(result_ptr);

        // Success if no crash/leak (run with valgrind to verify)
    }
}
