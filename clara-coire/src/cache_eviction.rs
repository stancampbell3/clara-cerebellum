use uuid::Uuid;

/// Implemented by any in-process evaluation cache that supports TTL and
/// session-scoped eviction.
///
/// `CarrionPicker` holds an optional `Arc<dyn EvaluateCacheEviction>` and
/// calls these methods on each sweep.  The concrete implementation lives in
/// `clara-toolbox` (as `ToolboxCacheEviction`) and is injected at startup by
/// the wiring layer (`clara-api`), keeping `clara-coire` free of a direct
/// dependency on `clara-toolbox`.
pub trait EvaluateCacheEviction: Send + Sync {
    /// Evict all entries whose creation timestamp is older than `cutoff_ms`
    /// (Unix epoch milliseconds).  Returns the number of entries removed.
    fn evict_older_than(&self, cutoff_ms: i64) -> usize;

    /// Evict all entries attributed to `deduction_id`.
    /// Returns the number of entries removed.
    fn evict_by_deduction(&self, deduction_id: Uuid) -> usize;
}
