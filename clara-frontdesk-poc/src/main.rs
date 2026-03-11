mod config;
mod deduce;
mod session;
mod state;
mod ws;

use std::sync::Arc;

use actix_files::Files;
use actix_web::{web, App, HttpServer};
use fiery_pit_client::FieryPitClient;

use config::load_config;
use state::AppState;
use ws::ws_index;

fn main() -> std::io::Result<()> {
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();

    let cfg = load_config();

    // Validate paths exist before starting.
    for path in [&cfg.paths.clara_pl_path, &cfg.paths.clara_clp_path] {
        if path.contains("CHANGE_ME") {
            log::warn!(
                "Path '{}' still contains CHANGE_ME — update config/city_of_dis.toml",
                path
            );
        }
    }

    log::info!(
        "City of Dis Front Desk — {} — starting on port {}",
        cfg.company.name,
        cfg.server.port
    );

    // FieryPitClient uses blocking reqwest — must be created BEFORE the actix runtime.
    let fiery_pit = FieryPitClient::new(&cfg.paths.fiery_pit_url);

    let state = web::Data::new(AppState {
        fiery_pit,
        clara_api_url: cfg.paths.clara_api_url.clone(),
        clara_pl_path: cfg.paths.clara_pl_path.clone(),
        clara_clp_path: cfg.paths.clara_clp_path.clone(),
        config: Arc::new(cfg.clone()),
    });

    let port = cfg.server.port;

    actix_web::rt::System::new().block_on(async move {
        HttpServer::new(move || {
            App::new()
                .app_data(state.clone())
                .route("/ws", web::get().to(ws_index))
                .service(
                    Files::new("/", "clara-frontdesk-poc/static")
                        .index_file("index.html"),
                )
        })
        .bind(("0.0.0.0", port))?
        .run()
        .await
    })
}
