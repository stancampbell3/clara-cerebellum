//! Clara Cerebrum Configuration Module
//!
//! This module provides configuration loading, validation, and management for the Clara Cerebrum service.
//! It supports:
//! - TOML-based configuration files
//! - Environment variable interpolation (${VAR_NAME} syntax)
//! - Environment-specific overrides (development.toml, production.toml)
//! - Configuration validation
//!
//! # Example
//!
//! ```no_run
//! use clara_config::ConfigLoader;
//!
//! // Load configuration for current environment
//! let config = ConfigLoader::from_env(None)?;
//!
//! // Use configuration
//! println!("Server listening on {}:{}", config.server.host, config.server.port);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod schema;
pub mod loader;
pub mod defaults;

pub use schema::{AppConfig, ConfigEnvironment};
pub use loader::{ConfigLoader, ConfigError};
