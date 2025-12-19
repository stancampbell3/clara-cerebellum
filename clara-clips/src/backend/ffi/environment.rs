// Safe Rust wrapper around CLIPS Environment

use super::bindings::{self, CLIPSValue, Environment, EvalError};
use std::ffi::{CStr, CString};
use libc::c_void;

/// Safe wrapper around a CLIPS Environment
pub struct ClipsEnvironment {
    env: *mut Environment,
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
            .finish()
    }
}

impl ClipsEnvironment {
    /// Create a new CLIPS environment
    pub fn new() -> Result<Self, String> {
        unsafe {
            let env = bindings::CreateEnvironment();
            if env.is_null() {
                return Err("Failed to create CLIPS environment".to_string());
            }
            Ok(Self { env })
        }
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
