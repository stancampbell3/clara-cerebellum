//! Counters: monotonically increasing totals, incremented inline at an
//! existing chokepoint (a ritual-lifecycle transition, a tool dispatch)
//! rather than scattered across call sites.

/// Outcome of a `ToolboxManager::execute_tool` call, matching its existing
/// match arms.
pub enum ToolStatus {
    Success,
    Error,
    NotFound,
}

impl ToolStatus {
    fn as_label(&self) -> &'static str {
        match self {
            ToolStatus::Success => "success",
            ToolStatus::Error => "error",
            ToolStatus::NotFound => "not_found",
        }
    }
}

pub fn ritual_created(domain: &str) {
    metrics_core::counter!("clara_ritual_created_total", "domain" => domain.to_string())
        .increment(1);
}

pub fn ritual_joined(domain: &str, outcome: &str) {
    metrics_core::counter!(
        "clara_ritual_join_total",
        "domain" => domain.to_string(),
        "outcome" => outcome.to_string()
    )
    .increment(1);
}

pub fn ritual_terminated(domain: &str) {
    metrics_core::counter!("clara_ritual_terminated_total", "domain" => domain.to_string())
        .increment(1);
}

/// `kind` is `"terminated"` or `"orphan"`, matching `TopicReapReport`'s two
/// counters. Call once per topic actually reaped.
pub fn ritual_topic_reaped(domain: &str, kind: &str) {
    metrics_core::counter!(
        "clara_ritual_topic_reaped_total",
        "domain" => domain.to_string(),
        "kind" => kind.to_string()
    )
    .increment(1);
}

/// Call once per reap error (a failed topic delete, or a failed topic list
/// during the orphan sweep).
pub fn ritual_topic_reap_error(domain: &str) {
    metrics_core::counter!("clara_ritual_topic_reap_errors_total", "domain" => domain.to_string())
        .increment(1);
}

/// `count` is the number of entries evicted in one reaper sweep.
pub fn deduction_reaped(domain: &str, count: u64) {
    if count > 0 {
        metrics_core::counter!("clara_deduction_reaped_total", "domain" => domain.to_string())
            .increment(count);
    }
}

pub fn tool_call(tool: &str, status: ToolStatus) {
    metrics_core::counter!(
        "clara_tool_calls_total",
        "tool" => tool.to_string(),
        "status" => status.as_label()
    )
    .increment(1);
}
