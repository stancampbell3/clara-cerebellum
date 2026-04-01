//! FFI callbacks for external systems (CLIPS, Prolog, etc.)
//!
//! This module provides the `rust_clara_evaluate` function that can be called from
//! C code to invoke Rust tools via the ToolboxManager.

use crate::{ToolboxManager, ToolRequest, ToolResponse};
use libc::c_char;
use serde_json::json;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{OnceLock, RwLock};
use std::thread;

// ── Call counter ──────────────────────────────────────────────────────────────
// Counts real tool executions only; cache hits do not increment the counter.
// Reset with `reset_evaluate_call_count` before a test run for a clean baseline.
static EVALUATE_CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Returns the number of actual tool executions since the last reset.
/// Cache hits are not counted.
pub fn get_evaluate_call_count() -> usize {
    EVALUATE_CALL_COUNT.load(Ordering::SeqCst)
}

/// Reset the execution counter to zero.
pub fn reset_evaluate_call_count() {
    EVALUATE_CALL_COUNT.store(0, Ordering::SeqCst);
}

// ── Result cache ──────────────────────────────────────────────────────────────
// Key: trimmed input JSON string.  Value: output JSON string.
// RwLock allows concurrent reads (cache hits) without exclusive locking.
// Size/TTL limits are intentionally omitted — callers use `clear_evaluate_cache`
// to bound growth (once per deduction run is the expected usage pattern).
fn evaluate_cache() -> &'static RwLock<HashMap<String, String>> {
    static CACHE: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Clear all cached results. Call before a test run to ensure a cold cache.
pub fn clear_evaluate_cache() {
    evaluate_cache().write().unwrap().clear();
}

/// Main callback function for external use (compatible with CLIPS and Prolog patterns)
///
/// This function receives a JSON string, processes it, and returns a JSON response.
/// Memory allocated for the returned string must be freed by calling rust_free_string.
///
/// The tool execution is performed in a separate OS thread to avoid conflicts with
/// async runtimes (e.g., Tokio) when tools use blocking HTTP clients.
///
/// # Safety
/// This function is unsafe because it:
/// - Dereferences raw pointers from C
/// - Allocates memory that must be freed by the caller
///
/// # Arguments
/// * `input_json` - C string containing JSON tool request
///
/// # Returns
/// Pointer to C string containing JSON response (must be freed with rust_free_string)
#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rust_clara_evaluate(input_json: *const c_char) -> *mut c_char {
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

        evaluate_json_string(input_str)
    }
}

/// Internal evaluation function that can be called from Rust code
///
/// This is the core evaluation logic, separated out so it can be used
/// by both the C FFI function and Rust callers.
pub fn evaluate_json_string(input_str: &str) -> *mut c_char {
    let key = input_str.trim();

    // 1. Cache hit: return memoised result without executing the tool or
    //    incrementing the counter.
    {
        let cache = evaluate_cache().read().unwrap();
        if let Some(cached) = cache.get(key) {
            log::debug!("evaluate_json_string: cache hit");
            return CString::new(cached.clone())
                .unwrap_or_else(|e| {
                    log::error!("evaluate_json_string: CString from cache failed: {}", e);
                    CString::new("{}").unwrap()
                })
                .into_raw();
        }
    }

    // 2. Cache miss — count this real execution.
    EVALUATE_CALL_COUNT.fetch_add(1, Ordering::SeqCst);
    log::debug!("evaluate_json_string called with input: {}", input_str);

    // Parse the JSON input
    let json_value: serde_json::Value = match serde_json::from_str(input_str) {
        Ok(val) => val,
        Err(e) => {
            log::error!("Failed to parse JSON: {}\n\tin : {}", e, input_str);
            return CString::new(format!(
                "{{\"status\":\"error\",\"message\":\"Invalid JSON: {}\"}}",
                e
            ))
            .unwrap_or_else(|_| CString::new("{}").unwrap())
            .into_raw();
        }
    };

    // Execute via ToolboxManager in a separate thread
    // This is necessary because some tools (like splinteredmind) use reqwest::blocking
    // which cannot run inside a Tokio async context. By spawning a dedicated OS thread,
    // we avoid the "Cannot drop a runtime in a context where blocking is not allowed" panic.
    let response = thread::spawn(move || {
        let manager = ToolboxManager::global().lock().unwrap();

        if json_value.get("tool").is_some() {
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
        }
    })
    .join()
    .unwrap_or_else(|e| {
        log::error!("Tool execution thread panicked: {:?}", e);
        ToolResponse::error("Tool execution failed: thread panicked".to_string())
    });

    let response_str = serde_json::to_string(&response).unwrap();

    // 3. Store result in cache before returning so future identical calls are
    //    served without re-executing the tool.
    evaluate_cache()
        .write()
        .unwrap()
        .insert(key.to_string(), response_str.clone());

    // Convert Rust string to C string
    match CString::new(response_str) {
        Ok(c_string) => {
            log::debug!("evaluate_json_string returning response");
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
#[cfg(feature = "ffi")]
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

/// Safe Rust wrapper for freeing strings returned by evaluate_json_string
pub fn free_c_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }

    unsafe {
        let _ = CString::from_raw(s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluate_json_string_null_input() {
        // Initialize toolbox
        ToolboxManager::init_global();

        let result_ptr = evaluate_json_string("");
        assert!(!result_ptr.is_null());

        // Clean up
        free_c_string(result_ptr);
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
