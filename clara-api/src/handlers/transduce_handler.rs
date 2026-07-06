//! Graph transduction handler.
//!
//! Turns a Cobbler Ritual `graph_layout` JSON into per-node Prolog/CLIPS
//! snippets implementing its edges (source-side `caws_consult/4` helpers,
//! source-side CLIPS reply hooks, target-side assertion-qualifier facts) —
//! see `clara_cycle::transduction::transduce_graph` and
//! docs/deduction_redux.md.
//!
//! # Endpoints
//!
//! | Method | Path | Description |
//! |--------|------|-------------|
//! | `POST` | `/transduce/graph` | Transduce a Ritual graph's edges |

use actix_web::{web, HttpResponse};
use serde::Deserialize;
use serde_json::json;

/// Request body: either the raw `graph_layout` JSON object inline, or the
/// same as a pre-serialized string (matching how lildaemon stores it).
#[derive(Debug, Deserialize)]
pub struct TransduceGraphRequest {
    pub graph: serde_json::Value,
}

// ── POST /transduce/graph ─────────────────────────────────────────────────────

/// Transduce a Ritual graph's edges into per-node source snippets.
///
/// Responses:
/// - `200 OK` — `{ "per_node": { "<node-id>": { "prolog": ..., "clips": ... } } }`
/// - `400 Bad Request` — the graph JSON could not be parsed.
pub async fn transduce_graph(req: web::Json<TransduceGraphRequest>) -> HttpResponse {
    // Accept both an inline JSON object and a JSON-string-wrapped graph
    // (lildaemon persists graph_layout as an opaque string).
    let graph_json = match &req.graph {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };

    match clara_cycle::transduction::transduce_graph(&graph_json) {
        Ok(result) => HttpResponse::Ok().json(result),
        Err(e) => HttpResponse::BadRequest().json(json!({ "error": e })),
    }
}
