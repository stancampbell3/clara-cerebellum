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
    pub system_prompt: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PathsConfig {
    pub clara_api_url: String,
    pub fiery_pit_url: String,
    pub clara_pl_path: String,
    pub clara_clp_path: String,
    pub static_path: String,
}

pub fn load_config() -> FrontDeskConfig {
    let path = std::env::var("FRONTDESK_CONFIG")
        .unwrap_or_else(|_| "clara-frontdesk-poc/config/city_of_dis.toml".to_string());

    let content = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Cannot read config '{}': {}", path, e));

    toml::from_str(&content)
        .unwrap_or_else(|e| panic!("Cannot parse config '{}': {}", path, e))
}
