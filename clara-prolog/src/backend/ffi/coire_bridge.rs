//! Prolog foreign predicates for Coire integration.
//!
//! Registers the following predicates:
//! - `coire_emit(+SessionId, +Origin, +PayloadJSON)` — write an event
//! - `coire_poll(+SessionId, -EventsJSON)` — read & mark pending events
//! - `coire_mark(+EventId)` — mark a single event as processed
//! - `coire_count(+SessionId, -Count)` — count pending events

use super::bindings::*;
use libc::{c_char, c_int};
use std::ffi::{CStr, CString};
use uuid::Uuid;

use clara_coire::ClaraEvent;

fn global_coire() -> &'static clara_coire::Coire {
    clara_coire::global()
}

/// Helper: extract a Rust string from a Prolog term.
/// Returns None on failure.
unsafe fn term_to_string(t: term_t) -> Option<String> {
    let mut ptr: *mut c_char = std::ptr::null_mut();
    let flags = CVT_ATOM | CVT_STRING | BUF_STACK | REP_UTF8;
    if PL_get_chars(t, &mut ptr, flags) == 0 || ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok().map(|s| s.to_string())
}

/// `coire_emit(+SessionId, +Origin, +PayloadJSON)`
#[no_mangle]
pub extern "C" fn pl_coire_emit(t_session: term_t, t_origin: term_t, t_payload: term_t) -> c_int {
    let result = (|| -> Result<(), String> {
        let session_str = unsafe { term_to_string(t_session) }
            .ok_or("failed to read SessionId")?;
        let origin_str = unsafe { term_to_string(t_origin) }
            .ok_or("failed to read Origin")?;
        let payload_str = unsafe { term_to_string(t_payload) }
            .ok_or("failed to read PayloadJSON")?;

        let session_id = Uuid::parse_str(&session_str)
            .map_err(|e| format!("invalid session_id: {}", e))?;
        let payload: serde_json::Value = serde_json::from_str(&payload_str)
            .map_err(|e| format!("invalid payload JSON: {}", e))?;

        let event = ClaraEvent::new(session_id, &origin_str, payload);
        global_coire()
            .write_event(&event)
            .map_err(|e| format!("write_event: {}", e))?;

        Ok(())
    })();

    match result {
        Ok(()) => 1,
        Err(e) => {
            log::error!("coire_emit/3: {}", e);
            0
        }
    }
}

/// `coire_poll(+SessionId, -EventsJSON)`
#[no_mangle]
pub extern "C" fn pl_coire_poll(t_session: term_t, t_events: term_t) -> c_int {
    let result = (|| -> Result<String, String> {
        let session_str = unsafe { term_to_string(t_session) }
            .ok_or("failed to read SessionId")?;
        let session_id = Uuid::parse_str(&session_str)
            .map_err(|e| format!("invalid session_id: {}", e))?;

        let events = global_coire()
            .poll_pending(session_id)
            .map_err(|e| format!("poll_pending: {}", e))?;

        serde_json::to_string(&events)
            .map_err(|e| format!("JSON serialization: {}", e))
    })();

    match result {
        Ok(json) => {
            let c_str = match CString::new(json) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("coire_poll/2: CString creation failed: {}", e);
                    return 0;
                }
            };
            unsafe {
                if PL_unify_string_chars(t_events, c_str.as_ptr()) != 0 {
                    1
                } else {
                    log::error!("coire_poll/2: unification failed");
                    0
                }
            }
        }
        Err(e) => {
            log::error!("coire_poll/2: {}", e);
            0
        }
    }
}

/// `coire_mark(+EventId)`
#[no_mangle]
pub extern "C" fn pl_coire_mark(t_event_id: term_t) -> c_int {
    let result = (|| -> Result<(), String> {
        let id_str = unsafe { term_to_string(t_event_id) }
            .ok_or("failed to read EventId")?;
        let event_id = Uuid::parse_str(&id_str)
            .map_err(|e| format!("invalid event_id: {}", e))?;

        global_coire()
            .mark_processed(event_id)
            .map_err(|e| format!("mark_processed: {}", e))?;

        Ok(())
    })();

    match result {
        Ok(()) => 1,
        Err(e) => {
            log::error!("coire_mark/1: {}", e);
            0
        }
    }
}

/// `coire_count(+SessionId, -Count)`
#[no_mangle]
pub extern "C" fn pl_coire_count(t_session: term_t, t_count: term_t) -> c_int {
    let result = (|| -> Result<i64, String> {
        let session_str = unsafe { term_to_string(t_session) }
            .ok_or("failed to read SessionId")?;
        let session_id = Uuid::parse_str(&session_str)
            .map_err(|e| format!("invalid session_id: {}", e))?;

        let count = global_coire()
            .count_pending(session_id)
            .map_err(|e| format!("count_pending: {}", e))?;

        Ok(count as i64)
    })();

    match result {
        Ok(count) => unsafe {
            if PL_unify_integer(t_count, count) != 0 {
                1
            } else {
                log::error!("coire_count/2: unification failed");
                0
            }
        },
        Err(e) => {
            log::error!("coire_count/2: {}", e);
            0
        }
    }
}

/// Track whether coire predicates have been registered.
static COIRE_PREDICATES_REGISTERED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Register all coire foreign predicates with the Prolog engine.
/// Safe to call multiple times — subsequent calls are no-ops.
pub fn register_coire_predicates() -> bool {
    *COIRE_PREDICATES_REGISTERED.get_or_init(|| {
        unsafe {
            let predicates: &[(&str, c_int, *const std::ffi::c_void)] = &[
                ("coire_emit", 3, pl_coire_emit as *const std::ffi::c_void),
                ("coire_poll", 2, pl_coire_poll as *const std::ffi::c_void),
                ("coire_mark", 1, pl_coire_mark as *const std::ffi::c_void),
                ("coire_count", 2, pl_coire_count as *const std::ffi::c_void),
            ];

            for (name, arity, func) in predicates {
                let c_name = match CString::new(*name) {
                    Ok(s) => s,
                    Err(e) => {
                        log::error!("Failed to create predicate name '{}': {}", name, e);
                        return false;
                    }
                };

                let result = PL_register_foreign(
                    c_name.as_ptr(),
                    *arity,
                    *func as pl_function_t,
                    0, // deterministic
                );

                if result != 0 {
                    log::info!("Registered {}/{}", name, arity);
                } else {
                    log::error!("Failed to register {}/{}", name, arity);
                    return false;
                }
            }

            log::info!("All coire predicates registered");
            true
        }
    })
}
