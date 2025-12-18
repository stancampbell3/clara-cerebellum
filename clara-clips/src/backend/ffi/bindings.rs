// FFI bindings to CLIPS C API
// This module provides low-level unsafe bindings to the CLIPS C library

use libc::{c_char, c_int, c_void};

/// Opaque Environment structure from CLIPS
#[repr(C)]
pub struct Environment {
    _private: [u8; 0],
}

/// CLIPSValue structure for eval results
#[repr(C)]
pub struct CLIPSValue {
    pub header: ValueHeader,
    pub value: *mut c_void,
}

#[repr(C)]
pub struct ValueHeader {
    pub type_: u16,
    pub _padding: [u8; 6],
}

/// EvalError enum from CLIPS
#[repr(C)]
#[allow(non_camel_case_types)]
pub enum EvalError {
    EE_NO_ERROR = 0,
    EE_PARSING_ERROR = 1,
    EE_PROCESSING_ERROR = 2,
}

extern "C" {
    /// Create a new CLIPS environment
    /// Returns NULL on failure
    pub fn CreateEnvironment() -> *mut Environment;

    /// Destroy a CLIPS environment
    /// Returns true on success
    pub fn DestroyEnvironment(env: *mut Environment) -> bool;

    /// Evaluate a CLIPS expression
    /// Args:
    ///   - env: The CLIPS environment
    ///   - expr: The expression to evaluate (C string)
    ///   - result: Pointer to CLIPSValue to store result
    /// Returns: EvalError status
    pub fn Eval(
        env: *mut Environment,
        expr: *const c_char,
        result: *mut CLIPSValue,
    ) -> EvalError;

    /// Reset the CLIPS environment
    pub fn Reset(env: *mut Environment);

    /// Load a CLIPS file
    /// Returns: Status code (non-zero on error)
    pub fn Load(env: *mut Environment, filename: *const c_char) -> c_int;

    /// Clear the CLIPS environment
    pub fn Clear(env: *mut Environment);

    /// Activate string router to capture output
    pub fn ActivateRouter(env: *mut Environment, router_name: *const c_char) -> bool;

    /// Deactivate string router
    pub fn DeactivateRouter(env: *mut Environment, router_name: *const c_char) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_environment_create_destroy() {
        unsafe {
            let env = CreateEnvironment();
            assert!(!env.is_null(), "CreateEnvironment should not return NULL");
            let destroyed = DestroyEnvironment(env);
            assert!(destroyed, "DestroyEnvironment should return true");
        }
    }
}
