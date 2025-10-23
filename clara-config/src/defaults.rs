use crate::schema::*;

pub fn default_server_config() -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".to_string(),
        port: 8080,
        request_timeout_ms: 30000,
        max_request_body_size: 1048576, // 1MB
    }
}

pub fn default_clips_config() -> ClipsConfig {
    ClipsConfig {
        binary_path: "./clips/binaries/clips".to_string(),
        handshake_timeout_ms: 5000,
        default_eval_timeout_ms: 2000,
        sentinel_marker: "__END__".to_string(),
    }
}

pub fn default_sessions_config() -> SessionsConfig {
    SessionsConfig {
        max_concurrent: 100,
        max_per_user: 10,
        eviction_policy: "lru".to_string(),
        default_ttl_seconds: 3600,
    }
}

pub fn default_resources_config() -> ResourcesConfig {
    ResourcesConfig {
        max_facts_per_session: 1000,
        max_rules_per_session: 500,
        max_memory_mb: 128,
        max_eval_queue_depth: 10,
    }
}

pub fn default_security_config() -> SecurityConfig {
    SecurityConfig {
        deny_list: vec![
            "system".to_string(),
            "load".to_string(),
            "save".to_string(),
            "open".to_string(),
            "close".to_string(),
        ],
        allow_list_mode: false,
        allowed_file_paths: vec!["./clips/rules".to_string()],
    }
}

pub fn default_persistence_config() -> PersistenceConfig {
    PersistenceConfig {
        enabled: false,
        storage_backend: "filesystem".to_string(),
        storage_path: "./data/sessions".to_string(),
        compression: "gzip".to_string(),
        encryption: true,
    }
}

pub fn default_observability_config() -> ObservabilityConfig {
    ObservabilityConfig {
        log_level: "info".to_string(),
        metrics_enabled: true,
        metrics_port: 9090,
        tracing_enabled: true,
        tracing_endpoint: "http://localhost:4317".to_string(),
    }
}

pub fn default_auth_config() -> AuthConfig {
    AuthConfig {
        jwt_secret: "${JWT_SECRET}".to_string(),
        token_expiry_seconds: 3600,
    }
}
