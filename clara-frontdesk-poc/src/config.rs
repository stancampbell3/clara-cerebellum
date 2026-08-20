use serde::Deserialize;
use std::fs;

#[derive(Debug, Clone, Deserialize)]
pub struct FrontDeskConfig {
    pub company: CompanyConfig,
    pub server: ServerConfig,
    pub paths: PathsConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompanyConfig {
    pub name: String,
    pub agent_name: String,
    pub greeting: String,
}

fn default_interface() -> String {
    "0.0.0.0".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
    #[serde(default = "default_interface")]
    pub interface: String,
}

fn default_service_username() -> String {
    "frontdesk-service".to_string()
}

fn default_service_password() -> String {
    "frontdesk-service-pw".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct PathsConfig {
    /// lildaemon's own base URL — hosts both FieryPit and the assistant
    /// REST API (goat/app/assistant/) this frontend now talks to.
    pub fiery_pit_url: String,
    pub static_path: String,
    /// Shared service account this frontend authenticates as. No
    /// per-visitor identity yet — every WS connection gets its own
    /// assistant session, but all sessions belong to this one account
    /// (matches examples_ritual_*.py's own _fierypit_bearer_token pattern).
    #[serde(default = "default_service_username")]
    pub service_username: String,
    #[serde(default = "default_service_password")]
    pub service_password: String,
}

pub fn load_config() -> FrontDeskConfig {
    let path = std::env::var("FRONTDESK_CONFIG")
        .unwrap_or_else(|_| "clara-frontdesk-poc/config/city_of_dis.toml".to_string());

    let content = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Cannot read config '{}': {}", path, e));

    toml::from_str(&content)
        .unwrap_or_else(|e| panic!("Cannot parse config '{}': {}", path, e))
}
