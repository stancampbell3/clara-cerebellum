//! C-callable FFI functions for CLIPS integration with the Coire.
//!
//! These functions are linked directly from CLIPS's `userfunctions.c`
//! and access the global Coire singleton.
//!
//! Gated behind `feature = "ffi"`.

use libc::c_char;
use std::ffi::{CStr, CString};
use uuid::Uuid;

use crate::event::ClaraEvent;

fn global_coire() -> &'static crate::Coire {
    crate::global()
}

/// Helper: convert a `*const c_char` to `&str`, returning None on null/invalid UTF-8.
unsafe fn cstr_to_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok()
}

/// Helper: allocate a C string on the heap. Caller must free with `rust_coire_free_string`.
fn to_c_string(s: &str) -> *mut c_char {
    CString::new(s).unwrap_or_default().into_raw()
}

/// Free a string allocated by the coire bridge functions.
#[no_mangle]
pub extern "C" fn rust_coire_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            drop(CString::from_raw(s));
        }
    }
}

/// Emit an event to the Coire.
/// Returns a heap-allocated C string: `"ok"` on success, or `{"error":"..."}` on failure.
#[no_mangle]
pub extern "C" fn rust_coire_emit(
    session: *const c_char,
    origin: *const c_char,
    payload: *const c_char,
) -> *mut c_char {
    let result = (|| -> Result<String, String> {
        let session_str = unsafe { cstr_to_str(session) }
            .ok_or_else(|| "null session_id".to_string())?;
        let origin_str = unsafe { cstr_to_str(origin) }
            .ok_or_else(|| "null origin".to_string())?;
        let payload_str = unsafe { cstr_to_str(payload) }
            .ok_or_else(|| "null payload".to_string())?;

        let session_id = Uuid::parse_str(session_str)
            .map_err(|e| format!("invalid session_id: {}", e))?;
        let payload_value: serde_json::Value = serde_json::from_str(payload_str)
            .map_err(|e| format!("invalid payload JSON: {}", e))?;

        let event = ClaraEvent::new(session_id, origin_str, payload_value);
        log::info!("Emitting event from CLIPS: {:?}", event);
        global_coire()
            .write_event(&event)
            .map_err(|e| format!("write_event failed: {}", e))?;

        Ok("ok".to_string())
    })();

    match result {
        Ok(s) => to_c_string(&s),
        Err(e) => to_c_string(&format!("{{\"error\":\"{}\"}}", e)),
    }
}

/// Poll all pending events for a session. Marks them processed atomically.
/// Returns a heap-allocated JSON array string.
#[no_mangle]
pub extern "C" fn rust_coire_poll(session: *const c_char) -> *mut c_char {
    let result = (|| -> Result<String, String> {
        let session_str = unsafe { cstr_to_str(session) }
            .ok_or_else(|| "null session_id".to_string())?;
        let session_id = Uuid::parse_str(session_str)
            .map_err(|e| format!("invalid session_id: {}", e))?;

        let events = global_coire()
            .poll_pending(session_id)
            .map_err(|e| format!("poll_pending failed: {}", e))?;

        serde_json::to_string(&events)
            .map_err(|e| format!("JSON serialization failed: {}", e))
    })();

    match result {
        Ok(s) => to_c_string(&s),
        Err(e) => to_c_string(&format!("{{\"error\":\"{}\"}}", e)),
    }
}

/// Mark a single event as processed.
/// Returns `"ok"` or `{"error":"..."}`.
#[no_mangle]
pub extern "C" fn rust_coire_mark(event_id: *const c_char) -> *mut c_char {
    let result = (|| -> Result<String, String> {
        let id_str = unsafe { cstr_to_str(event_id) }
            .ok_or_else(|| "null event_id".to_string())?;
        let eid = Uuid::parse_str(id_str)
            .map_err(|e| format!("invalid event_id: {}", e))?;

        global_coire()
            .mark_processed(eid)
            .map_err(|e| format!("mark_processed failed: {}", e))?;

        Ok("ok".to_string())
    })();

    match result {
        Ok(s) => to_c_string(&s),
        Err(e) => to_c_string(&format!("{{\"error\":\"{}\"}}", e)),
    }
}

/// Count pending events for a session. Returns the count, or -1 on error.
#[no_mangle]
pub extern "C" fn rust_coire_count(session: *const c_char) -> i64 {
    let result = (|| -> Result<i64, String> {
        let session_str = unsafe { cstr_to_str(session) }
            .ok_or_else(|| "null session_id".to_string())?;
        let session_id = Uuid::parse_str(session_str)
            .map_err(|e| format!("invalid session_id: {}", e))?;

        let count = global_coire()
            .count_pending(session_id)
            .map_err(|e| format!("count_pending failed: {}", e))?;

        Ok(count as i64)
    })();

    result.unwrap_or(-1)
}
