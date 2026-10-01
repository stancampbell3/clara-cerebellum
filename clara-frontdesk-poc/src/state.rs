use std::sync::Arc;

use reqwest::blocking::Client;

use crate::config::FrontDeskConfig;

pub struct AppState {
    pub http: Client,
    pub fiery_pit_url: String,
    /// Dis (clara-api)'s base URL — the topology view's ritual poller.
    pub dis_base_url: String,
    pub config: Arc<FrontDeskConfig>,
}
