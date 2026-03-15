mod config;
mod deduce;
mod session;
mod state;
mod ws;

use std::sync::Arc;
use std::time::Duration;

use actix_files::Files;
use actix_web::{web, App, HttpServer};
use fiery_pit_client::FieryPitClient;

use config::load_config;
use state::AppState;
use ws::ws_index;

/// Set the evaluator to "kindling", retrying on timeout or connection errors.
///
/// FieryPit may take up to ~2 minutes to be ready on a cold start; rather than
/// panicking immediately we retry a handful of times and log a clear warning if
/// we still cannot reach it, so the server starts and surfaces a useful error
/// on the first `/evaluate` call instead of crashing silently.
fn init_evaluator(fiery_pit: &FieryPitClient) {
    const MAX_RETRIES: u32 = 5;
    const RETRY_DELAY: Duration = Duration::from_secs(10);

    for attempt in 1..=MAX_RETRIES {
        match fiery_pit.set_evaluator("kindling") {
            Ok(resp) => {
                log::info!("FieryPit evaluator set to 'kindling': {}", resp);
                return;
            }
            Err(e) => {
                if attempt < MAX_RETRIES {
                    log::warn!(
                        "set_evaluator attempt {}/{} failed: {}. Retrying in {}s…",
                        attempt,
                        MAX_RETRIES,
                        e,
                        RETRY_DELAY.as_secs()
                    );
                    std::thread::sleep(RETRY_DELAY);
                } else {
                    log::error!(
                        "set_evaluator failed after {} attempts: {}. \
                         Server will start but /evaluate calls will likely fail until \
                         FieryPit is reachable and an evaluator is set.",
                        MAX_RETRIES,
                        e
                    );
                }
            }
        }
    }
}

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

    // Set the KindlingEvaluator before handing the client to any async code.
    init_evaluator(&fiery_pit);

    let state = web::Data::new(AppState {
        fiery_pit,
        clara_api_url: cfg.paths.clara_api_url.clone(),
        clara_pl_path: cfg.paths.clara_pl_path.clone(),
        clara_clp_path: cfg.paths.clara_clp_path.clone(),
        config: Arc::new(cfg.clone()),
    });

    let port = cfg.server.port;
    let static_path = cfg.paths.static_path.clone();

    actix_web::rt::System::new().block_on(async move {
        HttpServer::new(move || {
            App::new()
                .app_data(state.clone())
                .route("/ws", web::get().to(ws_index))
                .service(
                    Files::new("/", &static_path)
                        .index_file("index.html"),
                )
        })
        .bind(("0.0.0.0", port))?
        .run()
        .await
    })
}
