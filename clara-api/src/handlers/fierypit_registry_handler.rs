//! `PUT /fierypits/{id}`, `GET /fierypits`, `DELETE /fierypits/{id}` —
//! FieryPit registration and discovery. See
//! `docs/fierypit_registration_plan.md` for the full design.
//!
//! Registration is intentionally minimal: "who exists, what evaluators can
//! they run" — nothing Kafka-related. Kafka reachability stays the
//! pre-existing, separate operator concern it already is for
//! `/ritual/join`'s `bootstrap_servers`.

use actix_web::{web, HttpRequest, HttpResponse};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::handlers::session_handler::AppState;
use crate::middleware::auth::require_peer_token;

#[derive(Debug, Deserialize)]
pub struct RegisterFieryPitRequest {
    pub base_url: String,
    pub dis_domain: String,
    #[serde(default)]
    pub evaluators: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListFieryPitsQuery {
    pub evaluator: Option<String>,
    pub dis_domain: Option<String>,
}

/// `PUT /fierypits/{id}` — register or heartbeat (idempotent). `id` is a
/// UUID the caller derives itself (lildaemon:
/// `DisClient._fiery_pit_id` = `uuid.uuid5(uuid.NAMESPACE_URL, base_url)`);
/// Dis treats it as an opaque key and performs no derivation or
/// verification of its own.
pub async fn upsert_fiery_pit(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    body: web::Json<RegisterFieryPitRequest>,
) -> HttpResponse {
    if let Err(resp) = require_peer_token(&req) {
        return resp;
    }
    let body = body.into_inner();
    let reg = state
        .fiery_pit_registry
        .upsert(path.into_inner(), body.base_url, body.dis_domain, body.evaluators);
    HttpResponse::Ok().json(json!({
        "fiery_pit_id": reg.fiery_pit_id,
        "status": "registered",
        "ttl_seconds": state.fiery_pit_registry.ttl_seconds(),
    }))
}

/// `GET /fierypits?evaluator=&dis_domain=` — discovery. Excludes stale
/// entries (see `FieryPitRegistry::list`). Peer-token-gated like the
/// mutating endpoints, not left open: once Kafka (and plausibly this
/// port) are LAN-reachable, an unauthenticated discovery endpoint would
/// leak every FieryPit's URL and capability roster to anyone on the LAN.
pub async fn list_fiery_pits(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<ListFieryPitsQuery>,
) -> HttpResponse {
    if let Err(resp) = require_peer_token(&req) {
        return resp;
    }
    let q = query.into_inner();
    let fierypits = state
        .fiery_pit_registry
        .list(q.evaluator.as_deref(), q.dis_domain.as_deref());
    HttpResponse::Ok().json(json!({ "fierypits": fierypits }))
}

/// `DELETE /fierypits/{id}` — best-effort deregister, called on lildaemon
/// graceful shutdown. Not-found is not an error, matching
/// `coire_topics_handler::delete_topic`'s idiom.
pub async fn delete_fiery_pit(req: HttpRequest, state: web::Data<AppState>, path: web::Path<Uuid>) -> HttpResponse {
    if let Err(resp) = require_peer_token(&req) {
        return resp;
    }
    state.fiery_pit_registry.remove(path.into_inner());
    HttpResponse::Ok().json(json!({ "status": "deregistered" }))
}
