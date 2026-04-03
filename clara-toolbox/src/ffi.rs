//! FFI callbacks for external systems (CLIPS, Prolog, etc.)
//!
//! This module provides the `rust_clara_evaluate` function that can be called from
//! C code to invoke Rust tools via the ToolboxManager.

use crate::{ToolboxManager, ToolRequest, ToolResponse};
use libc::c_char;
use serde_json::json;
use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::CString;
#[cfg(any(feature = "ffi", test))]
use std::ffi::CStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{OnceLock, RwLock};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

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

// ── Dis domain identity ───────────────────────────────────────────────────────
// Set once at startup from application config; stamped onto every new cache
// entry as `domain_id` for future cross-domain gossip via the Coire relay.
static DOMAIN_ID: OnceLock<String> = OnceLock::new();

/// Register the Dis domain ID for this Clara instance.
///
/// Call once during startup (before the async runtime starts) with the value
/// from `config.server.dis_domain_id`.  A second call is ignored with a
/// warning — the first writer wins.
pub fn set_domain_id(id: String) {
    if DOMAIN_ID.set(id.clone()).is_err() {
        log::warn!("set_domain_id: domain ID already set, ignoring '{}'", id);
    }
}

/// Return the configured Dis domain ID for this instance, if set.
pub fn domain_id() -> Option<&'static str> {
    DOMAIN_ID.get().map(String::as_str)
}

// ── Deduction context ─────────────────────────────────────────────────────────
// Holds the UUID of the deduction run currently executing on this thread.
// Set by `CycleController::run()` so that every `evaluate_json_string` call
// during that run tags its cache entry with the correct deduction ID.
//
// `Cell<Option<Uuid>>` is used instead of `RefCell` because `Uuid` is `Copy`
// and we only need single-valued get/set, not borrowing.
thread_local! {
    static CURRENT_DEDUCTION_ID: Cell<Option<Uuid>> = const { Cell::new(None) };
}

/// Set the deduction context for cache-entry attribution on the calling thread.
///
/// Call with `Some(id)` before starting an engine pass and with `None` after.
/// Prefer [`DeductionContextGuard`] over calling this directly — the guard
/// guarantees `None` is restored even if the caller panics.
pub fn set_current_deduction_id(id: Option<Uuid>) {
    CURRENT_DEDUCTION_ID.with(|c| c.set(id));
}

/// Read the deduction ID currently set on this thread, if any.
pub fn current_deduction_id() -> Option<Uuid> {
    CURRENT_DEDUCTION_ID.with(|c| c.get())
}

/// RAII guard that installs a deduction ID on construction and restores `None`
/// on drop — including on unwind.  Obtain via [`deduction_context`].
pub struct DeductionContextGuard;

impl Drop for DeductionContextGuard {
    fn drop(&mut self) {
        set_current_deduction_id(None);
    }
}

/// Install `id` as the current deduction context for this thread and return a
/// guard that will clear it when dropped.
///
/// ```rust,ignore
/// let _ctx = clara_toolbox::ffi::deduction_context(self.deduction_id);
/// // ... engine passes ...
/// // context cleared automatically when `_ctx` goes out of scope
/// ```
pub fn deduction_context(id: Uuid) -> DeductionContextGuard {
    set_current_deduction_id(Some(id));
    DeductionContextGuard
}

// ── Result cache ──────────────────────────────────────────────────────────────

/// Metadata-bearing wrapper for a cached evaluation result.
///
/// The extra fields support TTL eviction by `CarrionPicker` and lay the
/// groundwork for Coire persistence and cross-domain gossip.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// The serialised JSON response returned by the tool/evaluator.
    pub value: String,

    /// Unix epoch milliseconds at the moment this entry was inserted.
    pub created_at_ms: i64,

    /// Byte length of `key + value` — useful for size-aware eviction later.
    pub size_bytes: usize,

    /// The deduction run that first produced this entry, if known.
    /// `None` for entries produced outside a managed deduction (e.g. tests).
    pub deduction_id: Option<Uuid>,

    /// Dis domain identifier of the clara-api instance that produced this
    /// entry.  Reserved for gossip — populated in a later phase.
    pub domain_id: Option<String>,
}

// Key: trimmed input JSON string.  Value: `CacheEntry`.
// RwLock allows concurrent reads (cache hits) without exclusive locking.
fn evaluate_cache() -> &'static RwLock<HashMap<String, CacheEntry>> {
    static CACHE: OnceLock<RwLock<HashMap<String, CacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Clear all cached results.  Call before a test run to ensure a cold cache.
pub fn clear_evaluate_cache() {
    evaluate_cache().write().unwrap().clear();
}

/// Return `(entry_count, total_size_bytes)` for the current cache contents.
pub fn evaluate_cache_stats() -> (usize, usize) {
    let cache = evaluate_cache().read().unwrap();
    let total_bytes = cache.values().map(|e| e.size_bytes).sum();
    (cache.len(), total_bytes)
}

/// Evict all entries whose `created_at_ms` is older than `cutoff_ms`
/// (Unix epoch milliseconds).  Returns the number of entries removed.
pub fn evict_cache_older_than(cutoff_ms: i64) -> usize {
    let mut cache = evaluate_cache().write().unwrap();
    let before = cache.len();
    cache.retain(|_, entry| entry.created_at_ms >= cutoff_ms);
    before - cache.len()
}

/// Evict all entries attributed to `deduction_id`.
/// Returns the number of entries removed.
pub fn evict_cache_by_deduction(deduction_id: Uuid) -> usize {
    let mut cache = evaluate_cache().write().unwrap();
    let before = cache.len();
    cache.retain(|_, entry| entry.deduction_id != Some(deduction_id));
    before - cache.len()
}

// ── EvaluateCacheEviction impl ────────────────────────────────────────────────

/// Zero-sized adapter that implements [`clara_coire::EvaluateCacheEviction`]
/// by delegating to the module-level eviction functions.
///
/// Construct one instance and wrap it in `Arc` to pass into `CarrionPicker`:
///
/// ```rust,ignore
/// let eviction = Arc::new(ToolboxCacheEviction);
/// let picker = CarrionPicker::new(store, coire_ttl, snap_ttl, interval, active)
///     .with_cache_eviction(cache_ttl, eviction);
/// ```
pub struct ToolboxCacheEviction;

impl clara_coire::EvaluateCacheEviction for ToolboxCacheEviction {
    fn evict_older_than(&self, cutoff_ms: i64) -> usize {
        evict_cache_older_than(cutoff_ms)
    }

    fn evict_by_deduction(&self, deduction_id: Uuid) -> usize {
        evict_cache_by_deduction(deduction_id)
    }
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
        if let Some(entry) = cache.get(key) {
            log::debug!("evaluate_json_string: cache hit");
            return CString::new(entry.value.clone())
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
    let entry = CacheEntry {
        size_bytes:    key.len() + response_str.len(),
        value:         response_str.clone(),
        created_at_ms: now_ms(),
        deduction_id:  current_deduction_id(),
        domain_id:     domain_id().map(str::to_string),
    };
    evaluate_cache().write().unwrap().insert(key.to_string(), entry);

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
    use std::sync::Mutex;

    // The evaluate cache is a process-global static.  Tests that assert exact
    // entry counts must not run concurrently with other cache-mutating tests.
    static CACHE_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn setup() -> std::sync::MutexGuard<'static, ()> {
        // Recover from a poisoned mutex: if a previous test panicked while
        // holding the lock, the underlying state (the cache) was cleared by
        // clear_evaluate_cache(), so the invariant we're protecting is still
        // sound. Propagating the poison would just cascade failures.
        let guard = CACHE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        ToolboxManager::init_global();
        clear_evaluate_cache();
        reset_evaluate_call_count();
        guard
    }

    #[test]
    fn test_evaluate_json_string_null_input() {
        let _guard = setup();
        let result_ptr = evaluate_json_string("");
        assert!(!result_ptr.is_null());
        free_c_string(result_ptr);
    }

    #[test]
    fn test_evaluate_json_string_invalid_json() {
        // Does not assert on cache state — no guard needed.
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
        let _guard = setup();
        let result_ptr = evaluate_json_string(r#"{"tool":"echo","arguments":{"message":"test"}}"#);
        assert!(!result_ptr.is_null());
        unsafe {
            let result_str = CStr::from_ptr(result_ptr).to_str().unwrap();
            assert!(result_str.contains("success"), "Expected success, got: {}", result_str);
        }
        free_c_string(result_ptr);
    }

    // ── CacheEntry metadata ──────────────────────────────────────────────────

    #[test]
    fn cache_entry_has_timestamp_and_size() {
        let _guard = setup();
        let before_ms = now_ms();
        let input = r#"{"tool":"echo","arguments":{"message":"ts_test"}}"#;
        let ptr = evaluate_json_string(input);
        free_c_string(ptr);
        let after_ms = now_ms();

        let cache = evaluate_cache().read().unwrap();
        let entry = cache.get(input.trim()).expect("entry should be cached");

        assert!(
            entry.created_at_ms >= before_ms && entry.created_at_ms <= after_ms,
            "timestamp out of range: {}",
            entry.created_at_ms
        );
        assert!(entry.size_bytes > 0, "size_bytes should be non-zero");
        assert!(entry.size_bytes >= input.len(), "size_bytes should include at least the key");
        assert_eq!(entry.deduction_id, None, "no deduction context active — should be None");
        // domain_id reflects the process-global set at startup; verify it matches.
        assert_eq!(
            entry.domain_id.as_deref(),
            domain_id(),
            "entry domain_id must reflect the global domain_id()"
        );
    }

    // ── evaluate_cache_stats ─────────────────────────────────────────────────

    #[test]
    fn cache_stats_reflect_insertions() {
        let _guard = setup();
        let (count0, bytes0) = evaluate_cache_stats();
        assert_eq!(count0, 0);
        assert_eq!(bytes0, 0);

        let ptr = evaluate_json_string(r#"{"tool":"echo","arguments":{"message":"stats"}}"#);
        free_c_string(ptr);

        let (count1, bytes1) = evaluate_cache_stats();
        assert_eq!(count1, 1);
        assert!(bytes1 > 0);
    }

    // ── evict_cache_older_than ───────────────────────────────────────────────

    #[test]
    fn evict_older_than_removes_stale_entries() {
        let _guard = setup();
        let ptr = evaluate_json_string(r#"{"tool":"echo","arguments":{"message":"evict_old"}}"#);
        free_c_string(ptr);

        // Backdate the entry so it appears old.
        {
            let mut cache = evaluate_cache().write().unwrap();
            for entry in cache.values_mut() {
                entry.created_at_ms = 1_000; // epoch + 1 s — definitely stale
            }
        }

        let cutoff = now_ms() - 1_000; // 1 s ago
        let removed = evict_cache_older_than(cutoff);
        assert_eq!(removed, 1, "one stale entry should have been evicted");
        let (count, _) = evaluate_cache_stats();
        assert_eq!(count, 0);
    }

    #[test]
    fn evict_older_than_preserves_fresh_entries() {
        let _guard = setup();
        let ptr = evaluate_json_string(r#"{"tool":"echo","arguments":{"message":"keep_fresh"}}"#);
        free_c_string(ptr);

        // Cutoff 10 minutes in the past — fresh entry should survive.
        let cutoff = now_ms() - 600_000;
        let removed = evict_cache_older_than(cutoff);
        assert_eq!(removed, 0);
        let (count, _) = evaluate_cache_stats();
        assert_eq!(count, 1);
    }

    // ── evict_cache_by_deduction ─────────────────────────────────────────────

    #[test]
    fn evict_by_deduction_removes_matching_entries() {
        let _guard = setup();
        let id = Uuid::new_v4();

        {
            let mut cache = evaluate_cache().write().unwrap();
            cache.insert("key_a".to_string(), CacheEntry {
                value:         "val_a".to_string(),
                created_at_ms: now_ms(),
                size_bytes:    10,
                deduction_id:  Some(id),
                domain_id:     None,
            });
            cache.insert("key_b".to_string(), CacheEntry {
                value:         "val_b".to_string(),
                created_at_ms: now_ms(),
                size_bytes:    10,
                deduction_id:  Some(Uuid::new_v4()), // different deduction
                domain_id:     None,
            });
        }

        let removed = evict_cache_by_deduction(id);
        assert_eq!(removed, 1, "only the matching deduction's entry should be removed");

        let cache = evaluate_cache().read().unwrap();
        assert!(!cache.contains_key("key_a"), "attributed entry should be gone");
        assert!(cache.contains_key("key_b"), "unrelated entry should remain");
    }

    #[test]
    fn evict_by_deduction_ignores_none_entries() {
        let _guard = setup();
        {
            let mut cache = evaluate_cache().write().unwrap();
            cache.insert("key_none".to_string(), CacheEntry {
                value:         "val".to_string(),
                created_at_ms: now_ms(),
                size_bytes:    8,
                deduction_id:  None,
                domain_id:     None,
            });
        }

        let removed = evict_cache_by_deduction(Uuid::new_v4());
        assert_eq!(removed, 0);
        let (count, _) = evaluate_cache_stats();
        assert_eq!(count, 1);
    }

    // ── deduction_context guard ──────────────────────────────────────────────

    #[test]
    fn deduction_context_tags_cache_entries() {
        let _guard = setup();
        let id = Uuid::new_v4();

        {
            let _ctx = deduction_context(id);
            assert_eq!(current_deduction_id(), Some(id));
            let ptr = evaluate_json_string(r#"{"tool":"echo","arguments":{"message":"ctx_tag"}}"#);
            free_c_string(ptr);
        } // guard drops here, restores None

        assert_eq!(current_deduction_id(), None, "context should be cleared after guard drop");

        let cache = evaluate_cache().read().unwrap();
        let entry = cache
            .get(r#"{"tool":"echo","arguments":{"message":"ctx_tag"}}"#)
            .expect("entry should be cached");
        assert_eq!(entry.deduction_id, Some(id), "entry should be attributed to the deduction");
    }

    #[test]
    fn deduction_context_none_when_no_guard() {
        // No setup() — just confirm baseline is None on this thread.
        assert_eq!(current_deduction_id(), None);
    }

    #[test]
    fn set_current_deduction_id_round_trips() {
        let id = Uuid::new_v4();
        set_current_deduction_id(Some(id));
        assert_eq!(current_deduction_id(), Some(id));
        set_current_deduction_id(None);
        assert_eq!(current_deduction_id(), None);
    }

    // ── domain_id global ─────────────────────────────────────────────────────

    #[test]
    fn set_domain_id_does_not_panic_on_repeated_calls() {
        // OnceLock: first writer wins.  Subsequent calls must not panic — they
        // are silently ignored with a log warning.  Test ordering is
        // non-deterministic so we don't assert a specific value, only that the
        // function is safe to call multiple times.
        set_domain_id("domain-alpha".to_string());
        set_domain_id("domain-beta".to_string()); // should not panic
    }

    #[test]
    fn domain_id_is_consistent_across_calls() {
        // Whatever value won the OnceLock race, it must be stable.
        let first  = domain_id();
        let second = domain_id();
        assert_eq!(first, second, "domain_id() must return the same value on every call");
    }

    #[test]
    fn cache_entry_domain_id_matches_global() {
        let _guard = setup();
        // Prime the global (no-op if already set by another test).
        set_domain_id("test-domain".to_string());

        let ptr = evaluate_json_string(r#"{"tool":"echo","arguments":{"message":"domain_tag"}}"#);
        free_c_string(ptr);

        let cache = evaluate_cache().read().unwrap();
        let entry = cache
            .get(r#"{"tool":"echo","arguments":{"message":"domain_tag"}}"#)
            .expect("entry should be cached");

        // The entry's domain_id must match whatever the global currently holds.
        assert_eq!(
            entry.domain_id.as_deref(),
            domain_id(),
            "CacheEntry::domain_id must reflect domain_id()"
        );
    }
}
