//! Histograms: durations recorded once, at the point a call or run ends.

use std::time::Duration;

pub fn tool_duration(tool: &str, elapsed: Duration) {
    metrics_core::histogram!("clara_tool_duration_seconds", "tool" => tool.to_string())
        .record(elapsed.as_secs_f64());
}

pub fn deduction_duration(domain: &str, elapsed: Duration) {
    metrics_core::histogram!("clara_deduction_duration_seconds", "domain" => domain.to_string())
        .record(elapsed.as_secs_f64());
}
