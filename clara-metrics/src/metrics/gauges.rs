//! Gauges: point-in-time values, recomputed wholesale at an existing sweep
//! point rather than incremented/decremented piecemeal — avoids drift
//! between the gauge and the authoritative in-memory state it mirrors.

/// The `CycleStatus` buckets a deduction can be in, as the strings this
/// module's `deduction_status` expects. Callers should set every bucket
/// (including a zero count) on each sweep, so a bucket that just emptied
/// doesn't linger at its last nonzero value.
pub const DEDUCTION_STATUSES: [&str; 5] =
    ["running", "converged", "interrupted", "expired", "error"];

pub fn ritual_active(domain: &str, count: usize) {
    metrics_core::gauge!("clara_ritual_active", "domain" => domain.to_string()).set(count as f64);
}

pub fn deduction_status(domain: &str, status: &str, count: usize) {
    metrics_core::gauge!(
        "clara_deduction_status",
        "domain" => domain.to_string(),
        "status" => status.to_string()
    )
    .set(count as f64);
}
