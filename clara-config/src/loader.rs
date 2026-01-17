use crate::schema::AppConfig;
use std::env;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    TomlParse(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Config not found at {0}")]
    NotFound(String),
}

/// Configuration loader that reads from TOML files and environment variables
pub struct ConfigLoader;

impl ConfigLoader {
    /// Load configuration from a TOML file with environment variable interpolation
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<AppConfig, ConfigError> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path).map_err(|e| {
            ConfigError::NotFound(format!("{}: {}", path.display(), e))
        })?;

        let mut config: AppConfig = toml::from_str(&content)
            .map_err(|e| ConfigError::TomlParse(e.to_string()))?;

        // Interpolate environment variables
        Self::interpolate_env_vars(&mut config)?;

        // Validate configuration
        config.validate().map_err(ConfigError::Validation)?;

        Ok(config)
    }

    /// Load configuration from environment-specific file
    pub fn from_env(env_name: Option<&str>) -> Result<AppConfig, ConfigError> {
        let env_name = if let Some(name) = env_name {
            name.to_string()
        } else {
            env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string())
        };

        // Try environment-specific config first, fall back to default
        let config_path = format!("config/{}.toml", env_name);
        let default_path = "config/default.toml";

        let mut config = Self::from_file(default_path)?;

        // Overlay environment-specific config if it exists and is not default
        if env_name != "development" && Path::new(&config_path).exists() {
            let env_config = Self::from_file(&config_path)?;
            config = Self::merge_configs(config, env_config);
        }

        Ok(config)
    }

    /// Merge two configurations, with the second overriding the first
    fn merge_configs(mut base: AppConfig, overlay: AppConfig) -> AppConfig {
        // For MVP, simple replacement of sections that are explicitly set
        // A more sophisticated merge would preserve unset fields

        // Only override if overlay values differ from defaults
        if overlay.server.host != "0.0.0.0" || overlay.server.port != 8080 {
            base.server = overlay.server;
        }
        if !overlay.clips.binary_path.is_empty() {
            base.clips = overlay.clips;
        }
        if overlay.sessions.max_concurrent != 100 {
            base.sessions = overlay.sessions;
        }
        if overlay.resources.max_facts_per_session != 1000 {
            base.resources = overlay.resources;
        }
        if !overlay.security.deny_list.is_empty() {
            base.security = overlay.security;
        }
        if overlay.persistence.enabled {
            base.persistence = overlay.persistence;
        }
        if overlay.observability.log_level != "info" {
            base.observability = overlay.observability;
        }
        if overlay.auth.jwt_secret != "${JWT_SECRET}" {
            base.auth = overlay.auth;
        }

        base
    }

    /// Interpolate environment variables in configuration values (${VAR_NAME} syntax)
    fn interpolate_env_vars(config: &mut AppConfig) -> Result<(), ConfigError> {
        // Helper function to interpolate a string
        let interpolate = |s: &str| -> Result<String, ConfigError> {
            let mut result = s.to_string();

            // Find all ${...} patterns
            while let Some(start) = result.find("${") {
                if let Some(end) = result[start..].find('}') {
                    let var_name = &result[start + 2..start + end];
                    let var_value = env::var(var_name)
                        .map_err(|_| ConfigError::Validation(
                            format!("Environment variable not found: {}", var_name)
                        ))?;
                    result.replace_range(start..start + end + 1, &var_value);
                } else {
                    break;
                }
            }
            Ok(result)
        };

        // Interpolate JWT secret
        config.auth.jwt_secret = interpolate(&config.auth.jwt_secret)?;

        // Interpolate other potentially variable paths
        config.clips.binary_path = interpolate(&config.clips.binary_path)?;
        config.persistence.storage_path = interpolate(&config.persistence.storage_path)?;

        Ok(())
    }

    /// Create a default configuration (for testing)
    pub fn default_config() -> AppConfig {
        AppConfig {
            server: crate::defaults::default_server_config(),
            clips: crate::defaults::default_clips_config(),
            sessions: crate::defaults::default_sessions_config(),
            resources: crate::defaults::default_resources_config(),
            security: crate::defaults::default_security_config(),
            persistence: crate::defaults::default_persistence_config(),
            observability: crate::defaults::default_observability_config(),
            auth: crate::defaults::default_auth_config(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_var_interpolation() {
        env::set_var("TEST_SECRET", "my-secret");

        let mut config = ConfigLoader::default_config();
        config.auth.jwt_secret = "${TEST_SECRET}".to_string();

        ConfigLoader::interpolate_env_vars(&mut config).unwrap();
        assert_eq!(config.auth.jwt_secret, "my-secret");
    }
}
