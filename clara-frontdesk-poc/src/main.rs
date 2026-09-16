mod assistant_client;
mod config;
mod state;
mod ws;

use std::sync::Arc;
use std::time::Duration;

use actix_files::Files;
use actix_multipart::Multipart;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use futures_util::StreamExt;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;

use assistant_client::login_demo;
use config::load_config;
use state::AppState;
use ws::ws_index;

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
}

#[derive(Deserialize)]
struct UploadQuery {
    session_id: String,
}

/// Mirrors lildaemon's own ATTACHMENT_MAX_BYTES (runtime.py) — enforced
/// here too so an oversized upload is rejected before it's ever streamed
/// to the backend.
const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;

/// POST /upload?session_id=... — proxies a dropped chat file to lildaemon's
/// POST /assistant/sessions/{id}/attachments (see assistant_client::
/// upload_attachment). Unlike /login and /ws, the bearer token travels as a
/// real Authorization header here (a plain fetch() can set headers
/// directly, unlike the WS constructor) rather than a query param.
async fn upload(
    state: web::Data<AppState>,
    req: HttpRequest,
    query: web::Query<UploadQuery>,
    mut payload: Multipart,
) -> HttpResponse {
    let token = match req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
    {
        Some(t) => t.to_string(),
        None => return HttpResponse::Unauthorized().json(json!({"error": "missing bearer token"})),
    };

    let mut filename = String::from("upload");
    let mut content_type: Option<String> = None;
    let mut bytes: Vec<u8> = Vec::new();

    while let Some(item) = payload.next().await {
        let mut field = match item {
            Ok(f) => f,
            Err(e) => return HttpResponse::BadRequest().json(json!({"error": e.to_string()})),
        };
        if field.name() != "file" {
            continue;
        }
        if let Some(fname) = field.content_disposition().get_filename() {
            filename = fname.to_string();
        }
        content_type = field.content_type().map(|m| m.to_string());

        while let Some(chunk) = field.next().await {
            let data = match chunk {
                Ok(d) => d,
                Err(e) => return HttpResponse::BadRequest().json(json!({"error": e.to_string()})),
            };
            if bytes.len() + data.len() > MAX_UPLOAD_BYTES {
                return HttpResponse::PayloadTooLarge()
                    .json(json!({"error": format!("file exceeds {MAX_UPLOAD_BYTES} byte limit")}));
            }
            bytes.extend_from_slice(&data);
        }
    }

    if bytes.is_empty() {
        return HttpResponse::BadRequest().json(json!({"error": "no file field in upload"}));
    }

    let http = state.http.clone();
    let base_url = state.fiery_pit_url.clone();
    let session_id = query.session_id.clone();

    let result = tokio::task::spawn_blocking(move || {
        assistant_client::upload_attachment(
            &http,
            &base_url,
            &token,
            &session_id,
            &filename,
            content_type.as_deref(),
            bytes,
        )
    })
    .await
    .unwrap_or_else(|e| {
        Err(assistant_client::AssistantError::Api(format!(
            "internal fault: {e}"
        )))
    });

    match result {
        Ok(resp) => HttpResponse::Ok().json(resp),
        Err(e) => {
            log::error!("upload failed: {}", e);
            HttpResponse::BadGateway().json(json!({"error": e.to_string()}))
        }
    }
}

/// POST /login — proxies the browser's username to lildaemon's
/// no-password /auth/login-demo and hands the resulting JWT back as the
/// per-visitor identity for this tab (see static/index.html, which stores
/// it in sessionStorage and passes it as `?token=` on the WS connection).
async fn login(state: web::Data<AppState>, body: web::Json<LoginRequest>) -> HttpResponse {
    let http = state.http.clone();
    let base_url = state.fiery_pit_url.clone();
    let username = body.username.clone();

    let result = tokio::task::spawn_blocking(move || login_demo(&http, &base_url, &username))
        .await
        .unwrap_or_else(|e| {
            Err(assistant_client::AssistantError::Api(format!(
                "internal fault: {e}"
            )))
        });

    match result {
        Ok(token) => HttpResponse::Ok().json(json!({"token": token})),
        Err(e) => {
            log::error!("login failed: {}", e);
            HttpResponse::BadGateway().json(json!({"error": e.to_string()}))
        }
    }
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

    let state = web::Data::new(AppState {
        http,
        fiery_pit_url: cfg.paths.fiery_pit_url.clone(),
        config: Arc::new(cfg.clone()),
    });

    let port = cfg.server.port;
    let interface = cfg.server.interface.clone();
    let static_path = cfg.paths.static_path.clone();

    actix_web::rt::System::new().block_on(async move {
        HttpServer::new(move || {
            App::new()
                .app_data(state.clone())
                .route("/login", web::post().to(login))
                .route("/upload", web::post().to(upload))
                .route("/ws", web::get().to(ws_index))
                .service(Files::new("/", &static_path).index_file("index.html"))
        })
        .bind((interface.as_str(), port))?
        .run()
        .await
    })
}
