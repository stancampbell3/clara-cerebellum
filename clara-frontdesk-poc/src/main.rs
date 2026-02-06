mod agent;
mod config;
mod server;
mod ws;

use config::FrontDeskConfig;
use fiery_pit_client::FieryPitClient;
use std::sync::Arc;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let port: u16 = std::env::var("FRONTDESK_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8088);

    let fiery_pit_url = std::env::var("FIERYPIT_URL")
        .unwrap_or_else(|_| "http://localhost:6666".to_string());

    let config = FrontDeskConfig::load_from_env_or_default().unwrap_or_else(|e| {
        log::error!("Failed to load config: {}", e);
        std::process::exit(1);
    });

    log::info!("Clara FrontDesk PoC");
    log::info!("  Company: {}", config.company.name);
    log::info!("  Agent: {}", config.agent.name);
    log::info!("  FieryPit: {}", fiery_pit_url);

    // Create FieryPitClient OUTSIDE the async runtime.
    // reqwest::blocking::Client has an internal tokio runtime that panics
    // if dropped inside an async context.
    let fiery_pit = Arc::new(FieryPitClient::new(&fiery_pit_url));

    // Health check runs in sync context — no spawn_blocking needed
    match fiery_pit.health() {
        Ok(_) => log::info!("Connected to FieryPit at {}", fiery_pit_url),
        Err(e) => log::warn!(
            "Could not connect to FieryPit at {}: {} (will retry on first request)",
            fiery_pit_url, e
        ),
    }

    // Start the actix system. The Arc<FieryPitClient> created above outlives
    // the runtime, so the inner reqwest client is only dropped back here in
    // sync main() after block_on returns.
    let sys = actix_web::rt::System::new();
    sys.block_on(server::run_server(port, config, fiery_pit))
        .unwrap_or_else(|e| {
            log::error!("Server error: {}", e);
            std::process::exit(1);
        });
}
