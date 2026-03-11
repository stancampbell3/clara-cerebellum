use std::sync::Arc;

use fiery_pit_client::FieryPitClient;

use crate::config::FrontDeskConfig;

pub struct AppState {
    pub fiery_pit: FieryPitClient,
    pub clara_api_url: String,
    pub clara_pl_path: String,
    pub clara_clp_path: String,
    pub config: Arc<FrontDeskConfig>,
}
