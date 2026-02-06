use crate::config::FrontDeskConfig;
use crate::ws;
use actix_files::NamedFile;
use actix_web::{web, App, HttpResponse, HttpServer};
use fiery_pit_client::FieryPitClient;
use std::path::PathBuf;
use std::sync::Arc;

async fn index() -> actix_web::Result<NamedFile> {
    let path: PathBuf = PathBuf::from("static/index.html");
    Ok(NamedFile::open(path)?)
}

async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}

pub async fn run_server(
    port: u16,
    config: FrontDeskConfig,
    fiery_pit: Arc<FieryPitClient>,
) -> std::io::Result<()> {
    log::info!(
        "Starting Clara FrontDesk PoC on port {} (company: {})",
        port,
        config.company.name
    );
    log::info!("Chat UI: http://localhost:{}", port);
    log::info!("WebSocket: ws://localhost:{}/ws", port);

    let config_data = config.clone();
    let fiery_pit_data = fiery_pit.clone();

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(fiery_pit_data.clone()))
            .app_data(web::Data::new(config_data.clone()))
            .route("/", web::get().to(index))
            .route("/ws", web::get().to(ws::ws_handler))
            .route("/health", web::get().to(health))
            .service(actix_files::Files::new("/static", "static").show_files_listing())
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}
