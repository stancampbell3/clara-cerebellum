use serde::{Deserialize, Serialize};

/// Complete application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub clips: ClipsConfig,
    pub sessions: SessionsConfig,
    pub resources: ResourcesConfig,
    pub security: SecurityConfig,
    pub persistence: PersistenceConfig,
    pub observability: ObservabilityConfig,
    pub auth: AuthConfig,
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub request_timeout_ms: u64,
    pub max_request_body_size: usize,
}

/// CLIPS binary and subprocess configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipsConfig {
    pub binary_path: String,
    pub handshake_timeout_ms: u64,
    pub default_eval_timeout_ms: u64,
    pub sentinel_marker: String,
}

/// Session management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionsConfig {
    pub max_concurrent: usize,
    pub max_per_user: usize,
    pub eviction_policy: String,
    pub default_ttl_seconds: u64,
}

/// Resource limits per session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesConfig {
    pub max_facts_per_session: u32,
    pub max_rules_per_session: u32,
    pub max_memory_mb: u32,
    pub max_eval_queue_depth: u32,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub deny_list: Vec<String>,
    pub allow_list_mode: bool,
    pub allowed_file_paths: Vec<String>,
}

/// Persistence configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistenceConfig {
    pub enabled: bool,
    pub storage_backend: String,
    pub storage_path: String,
    pub compression: String,
    pub encryption: bool,
    /// Path to the Coire persistent store DuckDB file.
    /// When set, `CycleController` will automatically save both engine
    /// mailboxes to this file at the end of every deduction run.
    /// Omit or set to `null` to disable Coire persistence.
    pub coire_store_path: Option<String>,
    /// Time-to-live for CoireStore entries in seconds. Sessions whose newest
    /// event is older than this are deleted by the carrion-picker background
    /// task. Default: 86400 (24 hours). Set to 0 to disable the picker.
    pub coire_store_ttl_seconds: u64,
    /// How often the carrion-picker sweeps the CoireStore, in seconds.
    /// Default: 3600 (1 hour). Ignored when `coire_store_ttl_seconds` is 0.
    pub coire_store_sweep_interval_seconds: u64,
    /// Time-to-live for [`DeductionSnapshot`] entries in seconds.
    /// Snapshots (and their associated Coire events) are deleted by the
    /// carrion-picker after this many seconds have elapsed since creation.
    /// Default: 604800 (7 days). Set to 0 to never expire snapshots via TTL
    /// (they can still be deleted explicitly via `DELETE /deduce/{id}/snapshot`).
    pub deduction_snapshot_ttl_seconds: u64,
}

/// Observability configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    pub log_level: String,
    pub metrics_enabled: bool,
    pub metrics_port: u16,
    pub tracing_enabled: bool,
    pub tracing_endpoint: String,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub token_expiry_seconds: u64,
}

impl AppConfig {
    /// Validate the configuration for consistency and sanity
    pub fn validate(&self) -> Result<(), String> {
        // Server validation
        if self.server.port == 0 {
            return Err("server.port must be non-zero".to_string());
        }
        if self.server.max_request_body_size == 0 {
            return Err("server.max_request_body_size must be non-zero".to_string());
        }

        // CLIPS validation
        if self.clips.binary_path.is_empty() {
            return Err("clips.binary_path must be set".to_string());
        }
        if self.clips.sentinel_marker.is_empty() {
            return Err("clips.sentinel_marker must be set".to_string());
        }

        // Sessions validation
        if self.sessions.max_concurrent == 0 {
            return Err("sessions.max_concurrent must be non-zero".to_string());
        }
        if self.sessions.max_per_user == 0 {
            return Err("sessions.max_per_user must be non-zero".to_string());
        }

        // Resources validation
        if self.resources.max_facts_per_session == 0 {
            return Err("resources.max_facts_per_session must be non-zero".to_string());
        }

        // Auth validation
        if self.auth.jwt_secret.is_empty() {
            return Err("auth.jwt_secret must be set".to_string());
        }

        Ok(())
    }
}

/// Environment-specific configuration overlay
pub struct ConfigEnvironment {
    pub env_name: String,
    pub debug: bool,
}

impl Default for ConfigEnvironment {
    fn default() -> Self {
        Self {
            env_name: std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string()),
            debug: std::env::var("DEBUG").is_ok(),
        }
    }
}
