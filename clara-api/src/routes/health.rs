use actix_web::HttpResponse;
use serde_json::json;
use std::time::UNIX_EPOCH;
use std::time::SystemTime;

/// GET /healthz - Health check
pub async fn health() -> HttpResponse {
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

/// GET /readyz - Readiness check
pub async fn ready() -> HttpResponse {
    HttpResponse::Ok().json(json!({"status": "ready"}))
}

/// GET /livez - Liveness check
pub async fn live() -> HttpResponse {
    let uptime = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    HttpResponse::Ok().json(json!({
        "status": "alive",
        "uptime_seconds": uptime
    }))
}

pub fn configure(cfg: &mut actix_web::web::ServiceConfig) {
    cfg.service(
        actix_web::web::scope("")
            .route("/healthz", actix_web::web::get().to(health))
            .route("/readyz", actix_web::web::get().to(ready))
            .route("/livez", actix_web::web::get().to(live)),
    );
}
