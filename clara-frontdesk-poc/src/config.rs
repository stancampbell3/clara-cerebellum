use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct FrontDeskConfig {
    pub company: CompanyConfig,
    pub agent: AgentConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompanyConfig {
    pub name: String,
    pub tagline: String,
    pub industry: String,
    pub services: ServicesConfig,
    pub contact: ContactConfig,
    pub hours: HoursConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServicesConfig {
    pub list: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContactConfig {
    pub email: String,
    pub phone: String,
    pub address: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HoursConfig {
    pub weekdays: String,
    pub weekends: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub greeting: String,
}

impl FrontDeskConfig {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: FrontDeskConfig = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn load_from_env_or_default() -> Result<Self, Box<dyn std::error::Error>> {
        let path = std::env::var("FRONTDESK_CONFIG")
            .unwrap_or_else(|_| "config/company.toml".to_string());
        Self::load(Path::new(&path))
    }

    pub fn formatted_greeting(&self) -> String {
        self.agent
            .greeting
            .replace("{company_name}", &self.company.name)
            .replace("{agent_name}", &self.agent.name)
    }

    pub fn company_context_summary(&self) -> String {
        format!(
            "Company: {} - {}\nIndustry: {}\nServices: {}\nContact: {} | {} | {}\nHours: Weekdays {} | Weekends {}",
            self.company.name,
            self.company.tagline,
            self.company.industry,
            self.company.services.list.join(", "),
            self.company.contact.email,
            self.company.contact.phone,
            self.company.contact.address,
            self.company.hours.weekdays,
            self.company.hours.weekends,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_load() {
        let config = FrontDeskConfig::load(Path::new("config/company.toml")).unwrap();
        assert_eq!(config.company.name, "Infernal Solutions");
        assert_eq!(config.agent.name, "Ember");
        assert!(!config.company.services.list.is_empty());
    }

    #[test]
    fn test_formatted_greeting() {
        let config = FrontDeskConfig::load(Path::new("config/company.toml")).unwrap();
        let greeting = config.formatted_greeting();
        assert!(greeting.contains("Infernal Solutions"));
        assert!(greeting.contains("Ember"));
    }
}
