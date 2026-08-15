//! Prolog foreign predicates for ad hoc Coire topics (`clara-ritual`).
//!
//! Registers, into the `the_coire` module, the low-level foreign predicates
//! backing `the_coire.pl`'s `coire_topic_*` predicates:
//! - `ritual_topic_create(+SubjectPath)` — ensure an ad hoc topic exists
//! - `ritual_topic_list(-TopicsJSON)` — list ad hoc topic subject paths
//! - `ritual_topic_delete(+SubjectPath)` — delete an ad hoc topic
//! - `ritual_topic_publish(+SubjectPath, +PayloadJSON, +OptionsJSON, -TephraId)`
//! - `ritual_topic_poll(+ConsumerId, +SubjectPath, -EnvelopesJSON)` — auto cursor
//! - `ritual_topic_poll_from(+SubjectPath, +SinceOffset, -EnvelopesJSON, -NextOffset)`
//!
//! Unlike `coire_bridge.rs` (which wraps the per-session, in-memory `Coire`
//! mailbox), these wrap `clara_ritual::adhoc` — the freeform, non-Ritual
//! Kafka topics segregated by Dis domain and subject path. See
//! `clara-ritual/src/adhoc.rs` for the shared, testable logic.

use super::bindings::*;
use clara_ritual::envelope::Routing;
use libc::{c_char, c_int};
use std::ffi::{CStr, CString};

/// Helper: extract a Rust string from a Prolog term.
unsafe fn term_to_string(t: term_t) -> Option<String> {
    let mut ptr: *mut c_char = std::ptr::null_mut();
    let flags = CVT_ATOM | CVT_STRING | BUF_STACK | REP_UTF8;
    if PL_get_chars(t, &mut ptr, flags) == 0 || ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok().map(|s| s.to_string())
}

unsafe fn unify_string(t: term_t, s: &str) -> c_int {
    let c_str = match CString::new(s) {
        Ok(s) => s,
        Err(e) => {
            log::error!("ritual_bridge: CString creation failed: {}", e);
            return 0;
        }
    };
    PL_unify_string_chars(t, c_str.as_ptr())
}

fn parse_publish_options(options_str: &str) -> Result<(Option<String>, Option<u64>, Routing), String> {
    if options_str.trim().is_empty() {
        return Ok((None, None, Routing::default()));
    }
    let v: serde_json::Value = serde_json::from_str(options_str)
        .map_err(|e| format!("invalid OptionsJSON: {}", e))?;
    let lbl = v.get("label").and_then(|x| x.as_str()).map(|s| s.to_string());
    let ttl_ms = v.get("ttl_ms").and_then(|x| x.as_u64());
    let routing = Routing {
        source_node_id: v.get("source_node_id").and_then(|x| x.as_str()).map(|s| s.to_string()),
        target_node_id: v.get("target_node_id").and_then(|x| x.as_str()).map(|s| s.to_string()),
        correlation_id: v.get("correlation_id").and_then(|x| x.as_str()).and_then(|s| s.parse().ok()),
        topic_path: None,
        tags: v.get("tags").and_then(|x| x.as_array()).map(|a| {
            a.iter().filter_map(|t| t.as_str().map(|s| s.to_string())).collect()
        }),
    };
    Ok((lbl, ttl_ms, routing))
}

/// `ritual_topic_create(+SubjectPath)`
#[no_mangle]
pub extern "C" fn pl_ritual_topic_create(t_subject: term_t) -> c_int {
    let result = (|| -> Result<(), String> {
        let subject = unsafe { term_to_string(t_subject) }.ok_or("failed to read SubjectPath")?;
        clara_ritual::adhoc::create_topic(
            clara_ritual::global().as_ref(),
            clara_ritual::global_dis_domain(),
            &subject,
            1,
            1,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })();

    match result {
        Ok(()) => 1,
        Err(e) => {
            log::error!("ritual_topic_create/1: {}", e);
            0
        }
    }
}

/// `ritual_topic_list(-TopicsJSON)`
#[no_mangle]
pub extern "C" fn pl_ritual_topic_list(t_topics: term_t) -> c_int {
    let result = (|| -> Result<String, String> {
        let subjects = clara_ritual::adhoc::list_topics(
            clara_ritual::global().as_ref(),
            clara_ritual::global_dis_domain(),
        )
        .map_err(|e| e.to_string())?;
        serde_json::to_string(&subjects).map_err(|e| format!("JSON serialization: {}", e))
    })();

    match result {
        Ok(json) => unsafe {
            if unify_string(t_topics, &json) != 0 {
                1
            } else {
                log::error!("ritual_topic_list/1: unification failed");
                0
            }
        },
        Err(e) => {
            log::error!("ritual_topic_list/1: {}", e);
            0
        }
    }
}

/// `ritual_topic_delete(+SubjectPath)`
#[no_mangle]
pub extern "C" fn pl_ritual_topic_delete(t_subject: term_t) -> c_int {
    let result = (|| -> Result<(), String> {
        let subject = unsafe { term_to_string(t_subject) }.ok_or("failed to read SubjectPath")?;
        clara_ritual::adhoc::delete_topic(
            clara_ritual::global().as_ref(),
            clara_ritual::global_dis_domain(),
            &subject,
        )
        .map_err(|e| e.to_string())
    })();

    match result {
        Ok(()) => 1,
        Err(e) => {
            log::error!("ritual_topic_delete/1: {}", e);
            0
        }
    }
}

/// `ritual_topic_publish(+SubjectPath, +PayloadJSON, +OptionsJSON, -TephraId)`
///
/// `OptionsJSON` may be `""` for defaults, or a JSON object with any of
/// `label`, `ttl_ms`, `target_node_id`, `source_node_id`, `correlation_id`,
/// `tags`.
#[no_mangle]
pub extern "C" fn pl_ritual_topic_publish(
    t_subject: term_t,
    t_payload: term_t,
    t_options: term_t,
    t_tephra_id: term_t,
) -> c_int {
    let result = (|| -> Result<String, String> {
        let subject = unsafe { term_to_string(t_subject) }.ok_or("failed to read SubjectPath")?;
        let payload_str = unsafe { term_to_string(t_payload) }.ok_or("failed to read PayloadJSON")?;
        let options_str = unsafe { term_to_string(t_options) }.unwrap_or_default();

        let body: serde_json::Value = serde_json::from_str(&payload_str)
            .map_err(|e| format!("invalid PayloadJSON: {}", e))?;
        let (lbl, ttl_ms, routing) = parse_publish_options(&options_str)?;

        let tephra_id = clara_ritual::adhoc::publish_topic(
            clara_ritual::global().as_ref(),
            clara_ritual::global_dis_domain(),
            &subject,
            body,
            lbl.as_deref(),
            ttl_ms,
            routing,
        )
        .map_err(|e| e.to_string())?;

        Ok(tephra_id.to_string())
    })();

    match result {
        Ok(id) => unsafe {
            if unify_string(t_tephra_id, &id) != 0 {
                1
            } else {
                log::error!("ritual_topic_publish/4: unification failed");
                0
            }
        },
        Err(e) => {
            log::error!("ritual_topic_publish/4: {}", e);
            0
        }
    }
}

/// `ritual_topic_poll(+ConsumerId, +SubjectPath, -EnvelopesJSON)`
///
/// Auto-advancing cursor keyed by `(ConsumerId, SubjectPath)` — pass the
/// caller's own `coire_session/1` id as `ConsumerId`.
#[no_mangle]
pub extern "C" fn pl_ritual_topic_poll(
    t_consumer: term_t,
    t_subject: term_t,
    t_envelopes: term_t,
) -> c_int {
    let result = (|| -> Result<String, String> {
        let consumer = unsafe { term_to_string(t_consumer) }.ok_or("failed to read ConsumerId")?;
        let subject = unsafe { term_to_string(t_subject) }.ok_or("failed to read SubjectPath")?;

        let envelopes = clara_ritual::adhoc::poll_topic_cursor(
            clara_ritual::global().as_ref(),
            clara_ritual::global_dis_domain(),
            &consumer,
            &subject,
        )
        .map_err(|e| e.to_string())?;

        serde_json::to_string(&envelopes).map_err(|e| format!("JSON serialization: {}", e))
    })();

    match result {
        Ok(json) => unsafe {
            if unify_string(t_envelopes, &json) != 0 {
                1
            } else {
                log::error!("ritual_topic_poll/3: unification failed");
                0
            }
        },
        Err(e) => {
            log::error!("ritual_topic_poll/3: {}", e);
            0
        }
    }
}

/// `ritual_topic_poll_from(+SubjectPath, +SinceOffset, -EnvelopesJSON, -NextOffset)`
///
/// Manual/explicit-offset variant — no cursor is tracked.
#[no_mangle]
pub extern "C" fn pl_ritual_topic_poll_from(
    t_subject: term_t,
    t_since_offset: term_t,
    t_envelopes: term_t,
    t_next_offset: term_t,
) -> c_int {
    let result = (|| -> Result<(String, i64), String> {
        let subject = unsafe { term_to_string(t_subject) }.ok_or("failed to read SubjectPath")?;
        let mut since_offset: i64 = 0;
        if unsafe { PL_get_int64(t_since_offset, &mut since_offset) } == 0 {
            return Err("failed to read SinceOffset".to_string());
        }

        let polled = clara_ritual::adhoc::poll_topic_from(
            clara_ritual::global().as_ref(),
            clara_ritual::global_dis_domain(),
            &subject,
            since_offset,
        )
        .map_err(|e| e.to_string())?;

        let json = serde_json::to_string(&polled.envelopes)
            .map_err(|e| format!("JSON serialization: {}", e))?;
        Ok((json, polled.next_offset))
    })();

    match result {
        Ok((json, next_offset)) => unsafe {
            if unify_string(t_envelopes, &json) == 0 {
                log::error!("ritual_topic_poll_from/4: envelopes unification failed");
                return 0;
            }
            if PL_unify_integer(t_next_offset, next_offset) != 0 {
                1
            } else {
                log::error!("ritual_topic_poll_from/4: next_offset unification failed");
                0
            }
        },
        Err(e) => {
            log::error!("ritual_topic_poll_from/4: {}", e);
            0
        }
    }
}

/// Track whether ritual predicates have been registered.
static RITUAL_PREDICATES_REGISTERED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Register all ritual (ad hoc topic) foreign predicates with the Prolog
/// engine. Safe to call multiple times — subsequent calls are no-ops.
pub fn register_ritual_predicates() -> bool {
    *RITUAL_PREDICATES_REGISTERED.get_or_init(|| {
        unsafe {
            let predicates: &[(&str, c_int, *const std::ffi::c_void)] = &[
                ("ritual_topic_create", 1, pl_ritual_topic_create as *const std::ffi::c_void),
                ("ritual_topic_list", 1, pl_ritual_topic_list as *const std::ffi::c_void),
                ("ritual_topic_delete", 1, pl_ritual_topic_delete as *const std::ffi::c_void),
                ("ritual_topic_publish", 4, pl_ritual_topic_publish as *const std::ffi::c_void),
                ("ritual_topic_poll", 3, pl_ritual_topic_poll as *const std::ffi::c_void),
                ("ritual_topic_poll_from", 4, pl_ritual_topic_poll_from as *const std::ffi::c_void),
            ];

            let c_module = match CString::new("the_coire") {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Failed to create module name: {}", e);
                    return false;
                }
            };

            for (name, arity, func) in predicates {
                let c_name = match CString::new(*name) {
                    Ok(s) => s,
                    Err(e) => {
                        log::error!("Failed to create predicate name '{}': {}", name, e);
                        return false;
                    }
                };

                let result = PL_register_foreign_in_module(
                    c_module.as_ptr(),
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

            log::info!("All ritual (ad hoc topic) predicates registered");
            true
        }
    })
}
