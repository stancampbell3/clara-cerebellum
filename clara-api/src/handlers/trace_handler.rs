//! Trace visualization handlers.
//!
//! Exposes per-phase tableau snapshots recorded during a deduction run,
//! together with colorized DOT graphs that map truth values onto the
//! underlying Prolog rule graph.
//!
//! # Endpoints
//!
//! | Method | Path | Description |
//! |--------|------|-------------|
//! | `GET`  | `/deduce/{id}/trace` | Ordered list of recorded tableau phases |
//! | `GET`  | `/deduce/{id}/trace/{change_id}/dot` | Colorized DOT for one phase |
//! | `GET`  | `/deduce/{id}/trace/{change_id}/entries` | Raw predicate entries for one phase |

use actix_web::{web, HttpResponse};
use clara_cycle::{coloring_from_entries, generate_dot, parse_prolog_rules, DotOptions, PredicateEntry, PrologRule};
use serde_json::json;
use uuid::Uuid;

use crate::handlers::session_handler::AppState;

// ── GET /deduce/{id}/trace ─────────────────────────────────────────────────────

/// Return the ordered list of tableau phase snapshots for a deduction run.
///
/// Each element carries metadata (cycle number, phase name, timestamps) but
/// omits the large `entries_json` blob; use the `/entries` sub-endpoint to
/// retrieve full predicate data for a specific phase.
pub async fn list_trace(
    state: web::Data<AppState>,
    path:  web::Path<Uuid>,
) -> HttpResponse {
    let deduction_id = path.into_inner();

    let store = match &state.coire_store {
        Some(s) => s.clone(),
        None => {
            return HttpResponse::ServiceUnavailable()
                .json(json!({ "error": "persistence not enabled" }));
        }
    };

    match store.query_tableau_changes(deduction_id) {
        Ok(changes) => {
            let steps: Vec<_> = changes
                .iter()
                .map(|c| {
                    json!({
                        "change_id":      c.change_id,
                        "deduction_id":   c.deduction_id,
                        "cycle_num":      c.cycle_num,
                        "phase":          c.phase,
                        "event_origin":   c.event_origin,
                        "event_type":     c.event_type,
                        "recorded_at_ms": c.recorded_at_ms,
                    })
                })
                .collect();
            HttpResponse::Ok().json(json!({ "trace": steps }))
        }
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({ "error": e.to_string() })),
    }
}

// ── GET /deduce/{id}/trace/{change_id}/dot ─────────────────────────────────────

/// Return a colorized DOT graph for one recorded phase.
///
/// The DOT is built from the parsed Prolog rules associated with the
/// deduction's `prolog_source_id`.  Node fill-colors reflect the tableau
/// truth values at the recorded phase:
///
/// - Green (`#d4edda`) — `KnownTrue`
/// - Red (`#f8d7da`) — `KnownFalse`
/// - Amber (`#fff3cd`) — mixed / `KnownUnresolved`
/// - Default — `Unknown`
///
/// The `parsed_rules` artifact is generated once and cached; subsequent
/// requests skip re-parsing.
///
/// Returns `text/plain; charset=utf-8` (raw DOT source).
pub async fn trace_dot(
    state: web::Data<AppState>,
    path:  web::Path<(Uuid, Uuid)>,
) -> HttpResponse {
    let (deduction_id, change_id) = path.into_inner();

    let store = match &state.coire_store {
        Some(s) => s.clone(),
        None => {
            return HttpResponse::ServiceUnavailable()
                .json(json!({ "error": "persistence not enabled" }));
        }
    };

    // 1. Load the requested tableau change.
    let change = match store.get_tableau_change(change_id) {
        Ok(Some(c)) if c.deduction_id == deduction_id => c,
        Ok(Some(_)) => {
            return HttpResponse::NotFound()
                .json(json!({ "error": "change_id does not belong to this deduction" }));
        }
        Ok(None) => {
            return HttpResponse::NotFound()
                .json(json!({ "error": "tableau change not found" }));
        }
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(json!({ "error": e.to_string() }));
        }
    };

    // 2. Find the prolog_source_id from the snapshot.
    let snap = match store.load_snapshot(deduction_id) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return HttpResponse::NotFound()
                .json(json!({ "error": "snapshot not found for this deduction" }));
        }
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(json!({ "error": e.to_string() }));
        }
    };

    let source_id = match snap.prolog_source_id {
        Some(id) => id,
        None => {
            return HttpResponse::UnprocessableEntity()
                .json(json!({
                    "error": "no prolog_source_id on snapshot — \
                              deduction was run without a registered source"
                }));
        }
    };

    // 3. Get or generate the "parsed_rules" artifact (JSON-serialized Vec<PrologRule>).
    let artifact = match store.sources.get_or_create_artifact(
        source_id,
        "parsed_rules",
        None, // inherit source TTL
        |content| {
            let rules = parse_prolog_rules(content);
            serde_json::to_string(&rules).map_err(clara_coire::CoireError::from)
        },
    ) {
        Ok(Some(a)) => a,
        Ok(None) => {
            return HttpResponse::NotFound()
                .json(json!({ "error": "prolog source not found in registry" }));
        }
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(json!({ "error": e.to_string() }));
        }
    };

    // 4. Deserialize rules.
    let rules: Vec<PrologRule> = match serde_json::from_str(&artifact.content) {
        Ok(r) => r,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(json!({ "error": format!("deserialize parsed_rules: {}", e) }));
        }
    };

    // 5. Deserialize tableau entries for this phase.
    let entries: Vec<PredicateEntry> = match serde_json::from_str(&change.entries_json) {
        Ok(e) => e,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(json!({ "error": format!("deserialize entries_json: {}", e) }));
        }
    };

    // 6. Build coloring and generate DOT.
    let coloring = coloring_from_entries(&entries);
    let dot = generate_dot(&rules, Some(&coloring), &DotOptions::default());

    HttpResponse::Ok()
        .content_type("text/plain; charset=utf-8")
        .body(dot)
}

// ── GET /deduce/{id}/trace/{change_id}/entries ─────────────────────────────────

/// Return the raw predicate entries for one recorded phase.
///
/// Each entry is a `PredicateEntry` from the Dagda tableau — it includes the
/// functor, arity, bound variables, truth value, and provenance metadata.
pub async fn trace_entries(
    state: web::Data<AppState>,
    path:  web::Path<(Uuid, Uuid)>,
) -> HttpResponse {
    let (deduction_id, change_id) = path.into_inner();

    let store = match &state.coire_store {
        Some(s) => s.clone(),
        None => {
            return HttpResponse::ServiceUnavailable()
                .json(json!({ "error": "persistence not enabled" }));
        }
    };

    let change = match store.get_tableau_change(change_id) {
        Ok(Some(c)) if c.deduction_id == deduction_id => c,
        Ok(Some(_)) => {
            return HttpResponse::NotFound()
                .json(json!({ "error": "change_id does not belong to this deduction" }));
        }
        Ok(None) => {
            return HttpResponse::NotFound()
                .json(json!({ "error": "tableau change not found" }));
        }
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(json!({ "error": e.to_string() }));
        }
    };

    let entries: Vec<PredicateEntry> = match serde_json::from_str(&change.entries_json) {
        Ok(e) => e,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(json!({ "error": format!("deserialize entries_json: {}", e) }));
        }
    };

    HttpResponse::Ok().json(json!({
        "change_id":      change_id,
        "deduction_id":   deduction_id,
        "cycle_num":      change.cycle_num,
        "phase":          change.phase,
        "recorded_at_ms": change.recorded_at_ms,
        "entries":        entries,
    }))
}
