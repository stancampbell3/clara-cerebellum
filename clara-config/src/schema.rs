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
