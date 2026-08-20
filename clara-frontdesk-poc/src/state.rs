use std::sync::Arc;

use reqwest::blocking::Client;

use crate::config::FrontDeskConfig;

pub struct AppState {
    pub http: Client,
    pub fiery_pit_url: String,
    /// Bearer JWT for the shared service account, obtained once at startup.
    pub bearer_token: String,
    pub config: Arc<FrontDeskConfig>,
}
