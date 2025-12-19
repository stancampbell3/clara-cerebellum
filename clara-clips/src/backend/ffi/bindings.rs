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

/// Router callback function types
pub type RouterQueryFunction = extern "C" fn(env: *mut Environment, logical_name: *const c_char, context: *mut c_void) -> bool;
pub type RouterWriteFunction = extern "C" fn(env: *mut Environment, logical_name: *const c_char, data: *const c_char, context: *mut c_void);

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

    /// Add a custom router to the environment
    /// Args:
    ///   - env: The CLIPS environment
    ///   - name: Router name (C string)
    ///   - priority: Router priority (higher = checked first)
    ///   - query: Query function (determines if router handles logical name)
    ///   - write: Write function (handles output)
    ///   - read, unread, exit: Other router functions (can be NULL)
    ///   - context: User data passed to callbacks
    /// Returns: true on success
    pub fn AddRouter(
        env: *mut Environment,
        name: *const c_char,
        priority: c_int,
        query: Option<RouterQueryFunction>,
        write: Option<RouterWriteFunction>,
        read: *const c_void,
        unread: *const c_void,
        exit: *const c_void,
        context: *mut c_void,
    ) -> bool;

    /// Remove a router from the environment
    pub fn DeleteRouter(env: *mut Environment, name: *const c_char) -> bool;

    /// Write a CLIPSValue to a logical name (router)
    pub fn WriteCLIPSValue(env: *mut Environment, logical_name: *const c_char, value: *mut CLIPSValue);

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
