//! C-callable FFI functions exposing ad hoc Coire topics to CLIPS.
//!
//! These functions are linked directly from CLIPS's `userfunctions.c`
//! (alongside `clara-coire`'s `rust_coire_*` symbols) and operate on the
//! global `clara_ritual` `KafkaBridge` singleton via [`crate::adhoc`].
//!
//! Gated behind `feature = "ffi"`.

use libc::c_char;
use std::ffi::{CStr, CString};

use crate::adhoc;
use crate::envelope::Routing;

/// Helper: convert a `*const c_char` to `&str`, returning None on null/invalid UTF-8.
unsafe fn cstr_to_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok()
}

/// Helper: allocate a C string on the heap. Caller must free with `rust_ritual_free_string`.
fn to_c_string(s: &str) -> *mut c_char {
    CString::new(s).unwrap_or_default().into_raw()
}

fn err_json(e: impl std::fmt::Display) -> String {
    format!("{{\"error\":\"{}\"}}", e.to_string().replace('"', "'"))
}

/// Free a string allocated by the ritual bridge functions.
#[no_mangle]
pub extern "C" fn rust_ritual_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            drop(CString::from_raw(s));
        }
    }
}

/// Ensure an ad hoc topic exists (1 partition, replication factor 1).
/// Returns `"ok"` or `{"error":"..."}`.
#[no_mangle]
pub extern "C" fn rust_ritual_topic_create(subject_path: *const c_char) -> *mut c_char {
    let result = (|| -> Result<String, String> {
        let subject = unsafe { cstr_to_str(subject_path) }
            .ok_or_else(|| "null subject_path".to_string())?;

        adhoc::create_topic(crate::global().as_ref(), crate::global_dis_domain(), subject, 1, 1)
            .map_err(|e| e.to_string())?;

        Ok("ok".to_string())
    })();

    match result {
        Ok(s) => to_c_string(&s),
        Err(e) => to_c_string(&err_json(e)),
    }
}

/// List every ad hoc topic's subject path in the ambient Dis domain.
/// Returns a heap-allocated JSON array of strings, or `{"error":"..."}`.
#[no_mangle]
pub extern "C" fn rust_ritual_topic_list() -> *mut c_char {
    let result = (|| -> Result<String, String> {
        let subjects = adhoc::list_topics(crate::global().as_ref(), crate::global_dis_domain())
            .map_err(|e| e.to_string())?;
        serde_json::to_string(&subjects).map_err(|e| e.to_string())
    })();

    match result {
        Ok(s) => to_c_string(&s),
        Err(e) => to_c_string(&err_json(e)),
    }
}

/// Delete an ad hoc topic. Deleting one that doesn't exist is not an error.
/// Returns `"ok"` or `{"error":"..."}`.
#[no_mangle]
pub extern "C" fn rust_ritual_topic_delete(subject_path: *const c_char) -> *mut c_char {
    let result = (|| -> Result<String, String> {
        let subject = unsafe { cstr_to_str(subject_path) }
            .ok_or_else(|| "null subject_path".to_string())?;

        adhoc::delete_topic(crate::global().as_ref(), crate::global_dis_domain(), subject)
            .map_err(|e| e.to_string())?;

        Ok("ok".to_string())
    })();

    match result {
        Ok(s) => to_c_string(&s),
        Err(e) => to_c_string(&err_json(e)),
    }
}

/// Publish a JSON payload to an ad hoc topic.
///
/// `options_json` may be `""` for defaults, or a JSON object with any of
/// `label`, `ttl_ms`, `target_node_id`, `source_node_id`, `correlation_id`,
/// `tags` (mirrors the `_caws` routing block `the_coire.clp`'s `caws-offer`
/// already builds). Returns the minted `tephra_id` as a JSON string, or
/// `{"error":"..."}`.
#[no_mangle]
pub extern "C" fn rust_ritual_topic_publish(
    subject_path: *const c_char,
    payload_json: *const c_char,
    options_json: *const c_char,
) -> *mut c_char {
    let result = (|| -> Result<String, String> {
        let subject = unsafe { cstr_to_str(subject_path) }
            .ok_or_else(|| "null subject_path".to_string())?;
        let payload_str = unsafe { cstr_to_str(payload_json) }
            .ok_or_else(|| "null payload".to_string())?;
        let options_str = unsafe { cstr_to_str(options_json) }.unwrap_or("");

        let body: serde_json::Value = serde_json::from_str(payload_str)
            .map_err(|e| format!("invalid payload JSON: {}", e))?;

        let (lbl, ttl_ms, routing) = parse_publish_options(options_str)?;

        let tephra_id = adhoc::publish_topic(
            crate::global().as_ref(),
            crate::global_dis_domain(),
            subject,
            body,
            lbl.as_deref(),
            ttl_ms,
            routing,
        )
        .map_err(|e| e.to_string())?;

        Ok(format!("{{\"tephra_id\":\"{}\"}}", tephra_id))
    })();

    match result {
        Ok(s) => to_c_string(&s),
        Err(e) => to_c_string(&err_json(e)),
    }
}

fn parse_publish_options(options_str: &str) -> Result<(Option<String>, Option<u64>, Routing), String> {
    if options_str.trim().is_empty() {
        return Ok((None, None, Routing::default()));
    }
    let v: serde_json::Value = serde_json::from_str(options_str)
        .map_err(|e| format!("invalid options JSON: {}", e))?;
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

/// Poll an ad hoc topic using an auto-advancing cursor tracked per
/// `(consumer_id, subject_path)` — pass the caller's own Coire session id
/// (`?*coire-session-id*`) as `consumer_id`. Returns a JSON array of
/// envelopes, or `{"error":"..."}`.
#[no_mangle]
pub extern "C" fn rust_ritual_topic_poll(
    consumer_id:  *const c_char,
    subject_path: *const c_char,
) -> *mut c_char {
    let result = (|| -> Result<String, String> {
        let consumer = unsafe { cstr_to_str(consumer_id) }
            .ok_or_else(|| "null consumer_id".to_string())?;
        let subject = unsafe { cstr_to_str(subject_path) }
            .ok_or_else(|| "null subject_path".to_string())?;

        let envelopes = adhoc::poll_topic_cursor(
            crate::global().as_ref(),
            crate::global_dis_domain(),
            consumer,
            subject,
        )
        .map_err(|e| e.to_string())?;

        serde_json::to_string(&envelopes).map_err(|e| e.to_string())
    })();

    match result {
        Ok(s) => to_c_string(&s),
        Err(e) => to_c_string(&err_json(e)),
    }
}

/// Poll an ad hoc topic from an explicit offset (manual control — no cursor
/// tracked). Returns JSON `{"envelopes":[...],"next_offset":N}`, or
/// `{"error":"..."}`.
#[no_mangle]
pub extern "C" fn rust_ritual_topic_poll_from(
    subject_path: *const c_char,
    since_offset: *const c_char,
) -> *mut c_char {
    let result = (|| -> Result<String, String> {
        let subject = unsafe { cstr_to_str(subject_path) }
            .ok_or_else(|| "null subject_path".to_string())?;
        let offset_str = unsafe { cstr_to_str(since_offset) }
            .ok_or_else(|| "null since_offset".to_string())?;
        let since_offset: i64 = offset_str
            .parse()
            .map_err(|e| format!("invalid since_offset: {}", e))?;

        let polled = adhoc::poll_topic_from(
            crate::global().as_ref(),
            crate::global_dis_domain(),
            subject,
            since_offset,
        )
        .map_err(|e| e.to_string())?;

        serde_json::to_string(&polled).map_err(|e| e.to_string())
    })();

    match result {
        Ok(s) => to_c_string(&s),
        Err(e) => to_c_string(&err_json(e)),
    }
}
