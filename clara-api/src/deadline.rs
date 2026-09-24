//! Wall-clock deadline resolution for `/deduce` (ritual_properties_spec.md, P1).
//!
//! A request's own `deadline_ms` wins over the server default; the server
//! ceiling then clamps whichever was chosen, so no run is eternal. A value of
//! 0 for either server setting disables that setting.

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeadlinePolicy {
    /// Applied when a request sends no `deadline_ms`. 0 = no default.
    pub default_ms: u64,
    /// Ceiling on any deadline. 0 = no ceiling.
    pub max_ms: u64,
}

impl DeadlinePolicy {
    pub fn from_seconds(default_seconds: u64, max_seconds: u64) -> Self {
        Self {
            default_ms: default_seconds.saturating_mul(1000),
            max_ms: max_seconds.saturating_mul(1000),
        }
    }

    /// No default and no ceiling: runs are unbounded (the pre-deadline behavior).
    pub const fn disabled() -> Self {
        Self { default_ms: 0, max_ms: 0 }
    }

    /// Resolve the deadline for one run. `Err` carries a message for a 400.
    pub fn resolve(&self, requested_ms: Option<u64>) -> Result<Option<Duration>, String> {
        if requested_ms == Some(0) {
            return Err("deadline_ms must be greater than 0 (omit it to use the server default)".into());
        }
        let chosen = requested_ms.unwrap_or(self.default_ms);
        if chosen == 0 {
            return Ok(None);
        }
        let clamped = if self.max_ms > 0 { chosen.min(self.max_ms) } else { chosen };
        Ok(Some(Duration::from_millis(clamped)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> DeadlinePolicy {
        DeadlinePolicy { default_ms: 3_600_000, max_ms: 14_400_000 }
    }

    #[test]
    fn request_value_wins_over_default() {
        assert_eq!(policy().resolve(Some(5_000)).unwrap(), Some(Duration::from_millis(5_000)));
    }

    #[test]
    fn default_applies_when_request_is_silent() {
        assert_eq!(policy().resolve(None).unwrap(), Some(Duration::from_millis(3_600_000)));
    }

    #[test]
    fn ceiling_clamps_a_request_and_the_default() {
        assert_eq!(policy().resolve(Some(99_999_999)).unwrap(), Some(Duration::from_millis(14_400_000)));
        let p = DeadlinePolicy { default_ms: 50_000, max_ms: 10_000 };
        assert_eq!(p.resolve(None).unwrap(), Some(Duration::from_millis(10_000)));
    }

    #[test]
    fn zero_request_is_rejected() {
        assert!(policy().resolve(Some(0)).is_err());
    }

    #[test]
    fn disabled_policy_means_unbounded_unless_the_request_sets_one() {
        let p = DeadlinePolicy::disabled();
        assert_eq!(p.resolve(None).unwrap(), None);
        assert_eq!(p.resolve(Some(7)).unwrap(), Some(Duration::from_millis(7)));
    }

    #[test]
    fn from_seconds_converts_and_zero_disables() {
        assert_eq!(DeadlinePolicy::from_seconds(3600, 14400), policy());
        assert_eq!(DeadlinePolicy::from_seconds(0, 0), DeadlinePolicy::disabled());
    }
}
