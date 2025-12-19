// Type conversion utilities between Rust and CLIPS types

use super::bindings::CLIPSValue;
use std::ffi::{CStr, CString};
use libc::c_char;

/// CLIPS type constants (from constant.h)
pub const VOID_TYPE: u16 = 0;
pub const FLOAT_TYPE: u16 = 1;
pub const INTEGER_TYPE: u16 = 2;
pub const SYMBOL_TYPE: u16 = 3;
pub const STRING_TYPE: u16 = 4;
pub const MULTIFIELD_TYPE: u16 = 5;
pub const EXTERNAL_ADDRESS_TYPE: u16 = 6;
pub const FACT_ADDRESS_TYPE: u16 = 7;
pub const INSTANCE_ADDRESS_TYPE: u16 = 8;
pub const INSTANCE_NAME_TYPE: u16 = 9;

/// Convert a CLIPSValue to a Rust String representation
/// This is a simplified conversion for MVP - will be enhanced later
pub unsafe fn clips_value_to_string(value: &CLIPSValue) -> String {
    match value.header.type_ {
        VOID_TYPE => "void".to_string(),
        FLOAT_TYPE => {
            // TODO: Extract actual float value
            "float".to_string()
        }
        INTEGER_TYPE => {
            // TODO: Extract actual integer value
            "integer".to_string()
        }
        SYMBOL_TYPE | STRING_TYPE => {
            // TODO: Extract actual string value
            "string".to_string()
        }
        _ => format!("unknown(type={})", value.header.type_),
    }
}

/// Convert a Rust string to a C string for CLIPS
pub fn string_to_c_string(s: &str) -> Result<CString, String> {
    CString::new(s).map_err(|e| format!("Failed to convert string: {}", e))
}

/// Safely convert a C string to a Rust string
pub unsafe fn c_string_to_string(c_str: *const c_char) -> String {
    if c_str.is_null() {
        return String::new();
    }
    CStr::from_ptr(c_str)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_to_c_string() {
        let result = string_to_c_string("test");
        assert!(result.is_ok());
        let c_string = result.unwrap();
        assert_eq!(c_string.to_str().unwrap(), "test");
    }

    #[test]
    fn test_c_string_to_string() {
        let c_str = CString::new("hello").unwrap();
        unsafe {
            let rust_str = c_string_to_string(c_str.as_ptr());
            assert_eq!(rust_str, "hello");
        }
    }

    #[test]
    fn test_null_c_string() {
        unsafe {
            let result = c_string_to_string(std::ptr::null());
            assert_eq!(result, "");
        }
    }
}
