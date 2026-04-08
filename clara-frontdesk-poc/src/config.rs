use serde::Deserialize;
use std::fs;

#[derive(Debug, Clone, Deserialize)]
pub struct FrontDeskConfig {
    pub company: CompanyConfig,
    pub devilish_supervisor: DevilishSupervisorConfig,
    #[serde(default)]
    pub deduction: DeductionConfig,
    pub server: ServerConfig,
    pub paths: PathsConfig,
}

impl FrontDeskConfig {
    /// System prompt used for the deduction LLM evaluate call.
    pub fn deduction_system_prompt(&self) -> &str {
        &self.devilish_supervisor.prompt
    }

    /// Model used for the deduction LLM evaluate call.
    pub fn deduction_model(&self) -> &str {
        &self.devilish_supervisor.model
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DevilishSupervisorConfig {
    pub prompt: String,
    pub model: String,
}

fn default_persist() -> bool {
    false
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DeductionConfig {
    #[serde(default = "default_persist")]
    pub persist: bool,
}

fn default_model() -> String {
    "qwen-clara:latest".to_string()
}

fn default_patience() -> u32 {
    8
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompanyConfig {
    pub name: String,
    pub agent_name: String,
    pub system_prompt: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_patience")]
    pub patience: u32,
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
