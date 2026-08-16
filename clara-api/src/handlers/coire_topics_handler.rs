use actix_web::{web, HttpResponse};
use clara_ritual::RitualError;
use serde::Deserialize;
use serde_json::json;

use crate::handlers::session_handler::AppState;

/// Body for `POST /coire/topics`.
#[derive(Debug, Deserialize)]
pub struct CreateCoireTopicRequest {
    pub subject_path: String,
    #[serde(default = "default_num_partitions")]
    pub num_partitions: i32,
    #[serde(default = "default_replication_factor")]
    pub replication_factor: i16,
}

fn default_num_partitions() -> i32 { 1 }
fn default_replication_factor() -> i16 { 1 }

// ---------------------------------------------------------------------------
// POST /coire/topics — create (or idempotently ensure) an ad hoc Coire topic
// ---------------------------------------------------------------------------

/// Create an ad hoc, non-Ritual Coire topic: `{dis_domain}.coire.{subject_path}`.
///
/// Unlike a Ritual, no registry entry or participant list is created — the
/// topic is a freeform channel any agent (Prolog, CLIPS, or a FieryPit
/// Evaluator over HTTP) can publish to or poll once it knows the subject
/// path. Idempotent: creating an existing topic is not an error.
///
/// Body: `{ "subject_path": "...", "num_partitions"?: 1, "replication_factor"?: 1 }`
/// Response 201: `{ "topic", "dis_domain", "bootstrap_servers" }`
pub async fn create_topic(
    state: web::Data<AppState>,
    req:   web::Json<CreateCoireTopicRequest>,
) -> HttpResponse {
    let dis_domain      = state.dis_domain.clone();
    let kafka_bootstrap = state.kafka_bootstrap.clone();
    let body            = req.into_inner();

    // `clara_ritual::adhoc::create_topic` calls `bridge.ensure_topic()`,
    // which internally does `runtime.block_on(...)` for the real Kafka
    // client — must run on a blocking thread, not an async one (same
    // constraint as `ritual_handler::create_ritual`).
    match web::block(move || {
        clara_ritual::adhoc::create_topic(
            clara_ritual::global().as_ref(),
            &dis_domain,
            &body.subject_path,
            body.num_partitions,
            body.replication_factor,
        )
    }).await {
        Ok(Ok(topic)) => {
            log::info!("coire topic {} created", topic);
            HttpResponse::Created().json(json!({
                "topic":             topic,
                "dis_domain":        state.dis_domain,
                "bootstrap_servers": kafka_bootstrap,
            }))
        }
        Ok(Err(RitualError::InvalidTopicName(msg))) => {
            HttpResponse::BadRequest().json(json!({ "error": msg }))
        }
        Ok(Err(e)) => {
            log::warn!("create coire topic failed: {}", e);
            HttpResponse::InternalServerError().json(json!({ "error": e.to_string() }))
        }
        Err(e) => {
            log::error!("create coire topic blocking task panicked: {}", e);
            HttpResponse::InternalServerError().json(json!({ "error": "internal error" }))
        }
    }
}

// ---------------------------------------------------------------------------
// GET /coire/topics — list ad hoc Coire topics in this Dis domain
// ---------------------------------------------------------------------------

/// List the subject paths of every ad hoc Coire topic in this Dis domain.
///
/// Response 200: `{ "topics": ["research.edge-detection", ...] }`
pub async fn list_topics(state: web::Data<AppState>) -> HttpResponse {
    let dis_domain = state.dis_domain.clone();

    match web::block(move || {
        clara_ritual::adhoc::list_topics(clara_ritual::global().as_ref(), &dis_domain)
    }).await {
        Ok(Ok(topics)) => HttpResponse::Ok().json(json!({ "topics": topics })),
        Ok(Err(e)) => {
            log::warn!("list coire topics failed: {}", e);
            HttpResponse::InternalServerError().json(json!({ "error": e.to_string() }))
        }
        Err(e) => {
            log::error!("list coire topics blocking task panicked: {}", e);
            HttpResponse::InternalServerError().json(json!({ "error": "internal error" }))
        }
    }
}

// ---------------------------------------------------------------------------
// DELETE /coire/topics/{subject} — delete an ad hoc Coire topic
// ---------------------------------------------------------------------------

/// Delete an ad hoc Coire topic. Deleting one that doesn't exist is not an
/// error — the caller's desired end state (topic gone) already holds.
///
/// Response 200: `{ "subject_path", "status": "deleted" }`
pub async fn delete_topic(
    state: web::Data<AppState>,
    path:  web::Path<String>,
) -> HttpResponse {
    let subject_path = path.into_inner();
    let dis_domain    = state.dis_domain.clone();
    let subject_for_response = subject_path.clone();

    match web::block(move || {
        clara_ritual::adhoc::delete_topic(clara_ritual::global().as_ref(), &dis_domain, &subject_path)
    }).await {
        Ok(Ok(())) => HttpResponse::Ok().json(json!({
            "subject_path": subject_for_response,
            "status":       "deleted",
        })),
        Ok(Err(e)) => {
            log::warn!("delete coire topic failed: {}", e);
            HttpResponse::InternalServerError().json(json!({ "error": e.to_string() }))
        }
        Err(e) => {
            log::error!("delete coire topic blocking task panicked: {}", e);
            HttpResponse::InternalServerError().json(json!({ "error": "internal error" }))
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_coire_topic_request_defaults() {
        let body: CreateCoireTopicRequest =
            serde_json::from_str(r#"{"subject_path":"research.edge-detection"}"#).unwrap();
        assert_eq!(body.subject_path, "research.edge-detection");
        assert_eq!(body.num_partitions, 1);
        assert_eq!(body.replication_factor, 1);
    }

    #[test]
    fn test_create_coire_topic_request_explicit_values() {
        let body: CreateCoireTopicRequest = serde_json::from_str(
            r#"{"subject_path":"research.foo","num_partitions":3,"replication_factor":2}"#,
        )
        .unwrap();
        assert_eq!(body.num_partitions, 3);
        assert_eq!(body.replication_factor, 2);
    }
}
