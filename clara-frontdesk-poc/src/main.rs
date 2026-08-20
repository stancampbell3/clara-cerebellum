mod assistant_client;
mod config;
mod state;
mod ws;

use std::sync::Arc;
use std::time::Duration;

use actix_files::Files;
use actix_web::{web, App, HttpServer};
use reqwest::blocking::Client;

use assistant_client::register_and_login;
use config::load_config;
use state::AppState;
use ws::ws_index;

/// Authenticate as the shared service account, retrying on connection
/// errors — lildaemon may take up to ~2 minutes to be ready on a cold
/// start (same rationale as the old init_evaluator retry loop this
/// replaces).
fn authenticate(http: &Client, base_url: &str, username: &str, password: &str) -> String {
    const MAX_RETRIES: u32 = 5;
    const RETRY_DELAY: Duration = Duration::from_secs(10);

    for attempt in 1..=MAX_RETRIES {
        match register_and_login(http, base_url, username, password) {
            Ok(token) => {
                log::info!("Authenticated to lildaemon as '{}'", username);
                return token;
            }
            Err(e) => {
                if attempt < MAX_RETRIES {
                    log::warn!(
                        "authenticate attempt {}/{} failed: {}. Retrying in {}s…",
                        attempt,
                        MAX_RETRIES,
                        e,
                        RETRY_DELAY.as_secs()
                    );
                    std::thread::sleep(RETRY_DELAY);
                } else {
                    panic!(
                        "authenticate failed after {} attempts against {}: {}",
                        MAX_RETRIES, base_url, e
                    );
                }
            }
        }
    }
    unreachable!()
}

fn main() -> std::io::Result<()> {
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();

    let cfg = load_config();

    log::info!(
        "{} — {} — starting on {}:{}",
        cfg.company.name,
        cfg.company.agent_name,
        cfg.server.interface,
        cfg.server.port
    );

    // Blocking reqwest client — created before the actix runtime, same
    // constraint the old FieryPitClient usage documented (a blocking
    // client dropped inside tokio can panic). Explicit long timeout:
    // reqwest's default is too short for a knowledge_query turn, which can
    // chain a research leg (up to ~200s) and an answer leg (up to ~180s)
    // sequentially inside one /send call — confirmed live, the default
    // timeout fired mid-request ("operation timed out") well before
    // lildaemon's own 31s-and-counting turn had even finished.
    let http = Client::builder()
        .timeout(Duration::from_secs(420))
        .build()
        .expect("failed to build reqwest client");
    let bearer_token = authenticate(
        &http,
        &cfg.paths.fiery_pit_url,
        &cfg.paths.service_username,
        &cfg.paths.service_password,
    );

    let state = web::Data::new(AppState {
        http,
        fiery_pit_url: cfg.paths.fiery_pit_url.clone(),
        bearer_token,
        config: Arc::new(cfg.clone()),
    });

    let port = cfg.server.port;
    let interface = cfg.server.interface.clone();
    let static_path = cfg.paths.static_path.clone();

    actix_web::rt::System::new().block_on(async move {
        HttpServer::new(move || {
            App::new()
                .app_data(state.clone())
                .route("/ws", web::get().to(ws_index))
                .service(Files::new("/", &static_path).index_file("index.html"))
        })
        .bind((interface.as_str(), port))?
        .run()
        .await
    })
}
