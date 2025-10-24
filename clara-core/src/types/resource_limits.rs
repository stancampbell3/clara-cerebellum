use serde::{Deserialize, Serialize};

/// Resource limit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimitConfig {
    /// Maximum number of facts per session
    pub max_facts: u32,

    /// Maximum number of rules per session
    pub max_rules: u32,

    /// Maximum memory in MB per session
    pub max_memory_mb: u32,

    /// Maximum evaluation queue depth per session
    pub max_eval_queue_depth: u32,
}

impl Default for ResourceLimitConfig {
    fn default() -> Self {
        Self {
            max_facts: 1000,
            max_rules: 500,
            max_memory_mb: 128,
            max_eval_queue_depth: 10,
        }
    }
}

impl ResourceLimitConfig {
    /// Create a new resource limit configuration
    pub fn new(
        max_facts: u32,
        max_rules: u32,
        max_memory_mb: u32,
        max_eval_queue_depth: u32,
    ) -> Self {
        Self {
            max_facts,
            max_rules,
            max_memory_mb,
            max_eval_queue_depth,
        }
    }

    /// Create a strict configuration
    pub fn strict() -> Self {
        Self {
            max_facts: 100,
            max_rules: 50,
            max_memory_mb: 32,
            max_eval_queue_depth: 5,
        }
    }

    /// Create a relaxed configuration
    pub fn relaxed() -> Self {
        Self {
            max_facts: 10000,
            max_rules: 5000,
            max_memory_mb: 512,
            max_eval_queue_depth: 50,
        }
    }

    /// Validate that limits are sensible
    pub fn validate(&self) -> Result<(), String> {
        if self.max_facts == 0 {
            return Err("max_facts must be greater than 0".to_string());
        }
        if self.max_rules == 0 {
            return Err("max_rules must be greater than 0".to_string());
        }
        if self.max_memory_mb == 0 {
            return Err("max_memory_mb must be greater than 0".to_string());
        }
        if self.max_eval_queue_depth == 0 {
            return Err("max_eval_queue_depth must be greater than 0".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ResourceLimitConfig::default();
        assert_eq!(config.max_facts, 1000);
        assert_eq!(config.max_rules, 500);
    }

    #[test]
    fn test_strict_config() {
        let config = ResourceLimitConfig::strict();
        assert!(config.max_facts < ResourceLimitConfig::default().max_facts);
    }

    #[test]
    fn test_validate() {
        let valid = ResourceLimitConfig::default();
        assert!(valid.validate().is_ok());

        let invalid = ResourceLimitConfig::new(0, 100, 100, 10);
        assert!(invalid.validate().is_err());
    }
}
