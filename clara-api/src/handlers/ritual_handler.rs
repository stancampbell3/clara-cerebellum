use actix_web::{web, HttpResponse};
use clara_ritual::{RitualConfig, RitualError, RitualState};
use fiery_pit_client::{FieryPitClient, FieryPitError};
use serde::Deserialize;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::handlers::session_handler::{AppState, CachedToken};

// ---------------------------------------------------------------------------
// GET /ritual — list active Rituals
// ---------------------------------------------------------------------------

/// Return all currently active Rituals.
///
/// Terminated Rituals are excluded. For post-mortem analysis of terminated
/// Rituals, consult application logs or the Coire sessions endpoint.
///
/// Response 200: `{ "rituals": [{ "ritual_id", "name", "state", "topic" }] }`
pub async fn list_rituals(state: web::Data<AppState>) -> HttpResponse {
    let summaries = state.ritual_registry.list_active();
    HttpResponse::Ok().json(serde_json::json!({ "rituals": summaries }))
}

/// Optional query parameters for `GET /ritual/{id}/join`.
#[derive(Debug, Deserialize)]
pub struct JoinQuery {
    /// Stable caller-supplied key (e.g. FieryPit URL or any unique string).
    /// When provided, repeated calls with the same key return the same
    /// `performance_id`, making the join idempotent for that participant.
    /// Omitting the key always generates a fresh `performance_id`.
    pub participant: Option<String>,
}

// ---------------------------------------------------------------------------
// Service-token helpers for FieryPit auto-bootstrap
// ---------------------------------------------------------------------------

/// Try to acquire a service JWT from `url/auth/service-token` using `secret`,
/// cache it, and return the token string.  Logs a warning and returns `None`
/// on failure so callers can fall back gracefully.
fn acquire_and_cache(
    url: &str,
    secret: &str,
    cache: &Arc<Mutex<Option<CachedToken>>>,
) -> Option<String> {
    let client = FieryPitClient::new(url);
    match client.auth_service_token("dis-bootstrap", secret) {
        Ok(resp) => {
            let margin_s: u64 = std::env::var("FIERYPIT_TOKEN_MARGIN_S")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(300);
            let live_s = resp.expires_in.saturating_sub(margin_s);
            let token = resp.access_token.clone();
            *cache.lock().unwrap() = Some(CachedToken {
                token:      resp.access_token,
                expires_at: Instant::now() + Duration::from_secs(live_s),
            });
            Some(token)
        }
        Err(e) => {
            log::warn!("create_ritual: service-token acquisition from {} failed: {}", url, e);
            None
        }
    }
}

/// Return a Bearer token to use when bootstrapping a FieryPit participant.
///
/// Priority:
/// 1. Static `FIERYPIT_SERVICE_KEY` env var (operator-managed, skips cache).
/// 2. Cached token, if it has not yet passed its expiry `Instant`.
/// 3. Fresh acquisition via `POST /auth/service-token` on `fiery_pit_url`
///    using `LILDAEMON_SERVICE_SECRET`.
///
/// Returns `None` when no static key is set, no secret is configured, and
/// acquisition fails — bootstrap will proceed unauthenticated (and likely `401`).
fn get_bootstrap_token(
    fiery_pit_url: &str,
    cache: &Arc<Mutex<Option<CachedToken>>>,
) -> Option<String> {
    if let Ok(key) = std::env::var("FIERYPIT_SERVICE_KEY") {
        if !key.is_empty() {
            return Some(key);
        }
    }
    let secret = std::env::var("LILDAEMON_SERVICE_SECRET")
        .ok()
        .filter(|s| !s.is_empty())?;

    // Return the cached token if it is still fresh.
    {
        let guard = cache.lock().unwrap();
        if let Some(ref tok) = *guard {
            if tok.expires_at > Instant::now() {
                return Some(tok.token.clone());
            }
        }
    }

    acquire_and_cache(fiery_pit_url, &secret, cache)
}

/// Returns `true` when `result` is a `401 Unauthorized` FieryPit error.
fn is_unauthorized(result: &Result<serde_json::Value, FieryPitError>) -> bool {
    matches!(result, Err(FieryPitError::Status(s, _)) if s.as_u16() == 401)
}

// ---------------------------------------------------------------------------
// POST /ritual — create a new Ritual
// ---------------------------------------------------------------------------

/// Create a new Ritual and return its `ritual_id`.
///
/// The caller (typically a FieryPit coordinator) is responsible for calling
/// `POST /ritual/{id}/join` to obtain topic routing info for each participant.
///
/// Body: `{ "name": "...", "participants": ["http://..."] }`
/// Response 201: `{ "ritual_id": "<uuid>" }`
pub async fn create_ritual(
    state: web::Data<AppState>,
    req:   web::Json<RitualConfig>,
) -> HttpResponse {
    let registry          = state.ritual_registry.clone();
    let dis_domain        = state.dis_domain.clone();
    let kafka_bootstrap   = state.kafka_bootstrap.clone();
    let token_cache_arc   = state.fiery_pit_token_cache.clone();
    let config            = req.into_inner();

    // `registry.create()` calls `broker.ensure_topic()` which internally does
    // `runtime.block_on(...)`. That panics when called from an async thread.
    // `web::block` moves the call onto a blocking thread pool thread where
    // `block_on` is safe — same pattern as `CycleController::run()`.
    match web::block(move || {
        let participants = config.participants.clone();
        let ritual_id    = registry.create(config)?;

        // Bootstrap any listed FieryPit participant URLs by calling
        // POST /ritual/join on each one.  We derive the topic the same way
        // RitualRegistry::create() does so we do not need to re-query the
        // registry.  Failures are logged as warnings but do NOT abort the
        // create response — the ritual exists; participants can rejoin later.
        if !participants.is_empty() {
            let topic = match clara_ritual::topic_name(&dis_domain, ritual_id) {
                Ok(t)  => t,
                Err(e) => {
                    log::warn!("create_ritual: could not derive topic for bootstrapping: {}", e);
                    return Ok(ritual_id);
                }
            };
            let bootstrap   = kafka_bootstrap.as_deref().unwrap_or("localhost:9092");
            let token_cache = token_cache_arc.clone();
            for url in &participants {
                // Attempt 1 — use cached / static token.
                let token1 = get_bootstrap_token(url.as_str(), &token_cache);
                let mut c = FieryPitClient::new(url.as_str());
                if let Some(ref t) = token1 { c = c.with_service_key(t.as_str()); }
                let r = c.ritual_join(ritual_id, &topic, bootstrap, &dis_domain, None, false, 30.0);

                // Attempt 2 — on 401, clear cache and acquire a fresh token from
                // this specific participant URL (handles per-instance user stores).
                let final_result = if is_unauthorized(&r) {
                    log::debug!("create_ritual: 401 from {}; refreshing service token", url);
                    *token_cache.lock().unwrap() = None;
                    let secret_opt = std::env::var("LILDAEMON_SERVICE_SECRET")
                        .ok()
                        .filter(|s| !s.is_empty());
                    let token2 = secret_opt
                        .and_then(|sec| acquire_and_cache(url.as_str(), &sec, &token_cache));
                    let mut c2 = FieryPitClient::new(url.as_str());
                    if let Some(ref t) = token2 { c2 = c2.with_service_key(t.as_str()); }
                    c2.ritual_join(ritual_id, &topic, bootstrap, &dis_domain, None, false, 30.0)
                } else {
                    r
                };

                match final_result {
                    Ok(_) => log::info!(
                        "create_ritual: bootstrapped participant {} for ritual {}",
                        url, ritual_id
                    ),
                    Err(e) => log::warn!(
                        "create_ritual: failed to bootstrap participant {}: {}",
                        url, e
                    ),
                }
            }
        }

        Ok::<Uuid, RitualError>(ritual_id)
    }).await {
        Ok(Ok(ritual_id)) => {
            log::info!("Ritual {} created", ritual_id);
            HttpResponse::Created().json(json!({ "ritual_id": ritual_id }))
        }
        Ok(Err(RitualError::InvalidTopicName(msg))) => {
            HttpResponse::BadRequest().json(json!({ "error": msg }))
        }
        Ok(Err(e)) => {
            log::warn!("create_ritual failed: {}", e);
            HttpResponse::InternalServerError().json(json!({ "error": e.to_string() }))
        }
        Err(e) => {
            log::error!("create_ritual blocking task panicked: {}", e);
            HttpResponse::InternalServerError().json(json!({ "error": "internal error" }))
        }
    }
}

// ---------------------------------------------------------------------------
// GET /ritual/{id}/join — join an existing Ritual
// ---------------------------------------------------------------------------

/// Join an existing active Ritual and return routing information.
///
/// Idempotent when a `participant` query parameter is supplied: the same key
/// always returns the same `performance_id`, so a FieryPit peer that
/// reconnects after a transient failure resumes with its original identity.
/// Without a `participant` key a fresh `performance_id` is generated on each
/// call (suitable for CycleController / internal use).
///
/// The returned `topic` is the Kafka topic the caller should subscribe to and
/// publish on. FieryPit evaluators use this information to set up their own
/// `confluent-kafka` producer/consumer directly.
///
/// Query: `?participant=<stable-key>` (optional)
/// Response 200: `{ "ritual_id", "performance_id", "topic", "dis_domain" }`
/// Response 404: Ritual not found.
/// Response 409: Ritual is terminated.
pub async fn join_ritual(
    state: web::Data<AppState>,
    path:  web::Path<Uuid>,
    query: web::Query<JoinQuery>,
) -> HttpResponse {
    let ritual_id       = path.into_inner();
    let participant_key = query.into_inner().participant;
    let registry        = state.ritual_registry.clone();

    // `registry.join()` calls `broker.latest_offset()` which uses
    // `runtime.block_on(...)` — must run on a blocking thread, not an async one.
    match web::block(move || registry.join(ritual_id, participant_key.as_deref())).await {
        Ok(Ok(handle)) => {
            log::info!(
                "Ritual {} joined — performance {}",
                ritual_id, handle.performance_id
            );
            HttpResponse::Ok().json(json!({
                "ritual_id":      handle.ritual_id,
                "performance_id": handle.performance_id,
                "topic":          handle.topic(),
                "dis_domain":     handle.dis_domain,
            }))
        }
        Ok(Err(RitualError::TopicNotFound(_))) => {
            HttpResponse::NotFound().json(json!({ "error": "ritual not found" }))
        }
        Ok(Err(RitualError::BrokerError(msg))) => {
            // join() returns BrokerError when the ritual is terminated.
            HttpResponse::Conflict().json(json!({ "error": msg }))
        }
        Ok(Err(e)) => {
            log::warn!("join_ritual {}: {}", ritual_id, e);
            HttpResponse::InternalServerError().json(json!({ "error": e.to_string() }))
        }
        Err(e) => {
            log::error!("join_ritual blocking task panicked: {}", e);
            HttpResponse::InternalServerError().json(json!({ "error": "internal error" }))
        }
    }
}

// ---------------------------------------------------------------------------
// DELETE /ritual/{id} — terminate a Ritual
// ---------------------------------------------------------------------------

/// Mark a Ritual as terminated.
///
/// Existing `RitualHandle`s held by `CycleController` instances continue to
/// function until the Kafka topic is deleted (Phase 5 admin API). New `join`
/// calls on the terminated Ritual will be rejected.
///
/// Response 200: `{ "ritual_id", "status": "terminated" }`
/// Response 404: Ritual not found.
pub async fn terminate_ritual(
    state: web::Data<AppState>,
    path:  web::Path<Uuid>,
) -> HttpResponse {
    let ritual_id = path.into_inner();

    match state.ritual_registry.terminate(ritual_id) {
        Ok(()) => {
            log::info!("Ritual {} terminated", ritual_id);
            HttpResponse::Ok().json(json!({
                "ritual_id": ritual_id,
                "status":    "terminated",
            }))
        }
        Err(RitualError::TopicNotFound(_)) => {
            HttpResponse::NotFound().json(json!({ "error": "ritual not found" }))
        }
        Err(e) => {
            log::warn!("terminate_ritual {}: {}", ritual_id, e);
            HttpResponse::InternalServerError().json(json!({ "error": e.to_string() }))
        }
    }
}

// ---------------------------------------------------------------------------
// GET /ritual/{id}/status — inspect Ritual state
// ---------------------------------------------------------------------------

/// Return the current state of a Ritual.
///
/// Response 200: `{ "ritual_id", "state": "active" | "terminated" }`
/// Response 404: Ritual not found.
pub async fn ritual_status(
    state: web::Data<AppState>,
    path:  web::Path<Uuid>,
) -> HttpResponse {
    let ritual_id = path.into_inner();

    match state.ritual_registry.get_status(ritual_id) {
        Some(ritual_state) => {
            let state_str = match ritual_state {
                RitualState::Active     => "active",
                RitualState::Terminated => "terminated",
            };
            HttpResponse::Ok().json(json!({
                "ritual_id": ritual_id,
                "state":     state_str,
            }))
        }
        None => HttpResponse::NotFound().json(json!({ "error": "ritual not found" })),
    }
}
