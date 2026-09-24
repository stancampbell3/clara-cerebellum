//! Source registry handlers.
//!
//! Manages content-addressed Prolog/CLIPS source files and their derived
//! artifacts (DOT graphs, parsed rule JSON, etc.).  Sources are deduplicated
//! by `(SHA-256, source_type)` — uploading the same content twice returns the
//! existing `source_id`.
//!
//! # Endpoints
//!
//! | Method   | Path | Description |
//! |----------|------|-------------|
//! | `POST`   | `/source` | Register a new source |
//! | `GET`    | `/source/{id}` | Retrieve source metadata + content |
//! | `GET`    | `/source/{id}/artifact/{type}` | Get (or generate) a derived artifact |
//! | `DELETE` | `/source/{id}` | Delete a source and all its artifacts |
//! | `POST`   | `/source/check-modules` | Check `prolog-module` sources for missing ids and predicate conflicts |

use actix_web::{web, HttpResponse};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::handlers::session_handler::AppState;
use crate::models::RegisterSourceRequest;
use crate::module_sources::{self, ModuleError};

// ── POST /source ───────────────────────────────────────────────────────────────

/// Register a new Prolog or CLIPS source.
///
/// Responses:
/// - `201 Created` — new source registered; body contains `source_id` and
///   `is_new: true`.
/// - `200 OK` — identical content already registered; returns the existing
///   `source_id` with `is_new: false`.
pub async fn register_source(
    state: web::Data<AppState>,
    req:   web::Json<RegisterSourceRequest>,
) -> HttpResponse {
    let store = match &state.coire_store {
        Some(s) => s.clone(),
        None => {
            return HttpResponse::ServiceUnavailable()
                .json(json!({ "error": "persistence not enabled" }));
        }
    };

    let expires_at_ms = req.ttl_ms.map(|ttl| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
            + ttl
    });

    match store.sources.register(
        &req.source_type,
        req.label.as_deref(),
        &req.content,
        expires_at_ms,
    ) {
        Ok((source_id, is_new)) => {
            let status = if is_new {
                actix_web::http::StatusCode::CREATED
            } else {
                actix_web::http::StatusCode::OK
            };
            HttpResponse::build(status).json(json!({
                "source_id": source_id,
                "is_new":    is_new,
            }))
        }
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({ "error": e.to_string() })),
    }
}

// ── GET /source/{id} ──────────────────────────────────────────────────────────

/// Retrieve a registered source by ID.
///
/// Returns the full `SourceEntry` including content.
pub async fn get_source(
    state: web::Data<AppState>,
    path:  web::Path<Uuid>,
) -> HttpResponse {
    let source_id = path.into_inner();

    let store = match &state.coire_store {
        Some(s) => s.clone(),
        None => {
            return HttpResponse::ServiceUnavailable()
                .json(json!({ "error": "persistence not enabled" }));
        }
    };

    match store.sources.get(source_id) {
        Ok(Some(entry)) => HttpResponse::Ok().json(entry),
        Ok(None) => HttpResponse::NotFound()
            .json(json!({ "error": "source not found" })),
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({ "error": e.to_string() })),
    }
}

// ── GET /source/{id}/artifact/{type} ──────────────────────────────────────────

/// Retrieve (or lazily generate) a derived artifact for a source.
///
/// Supported `artifact_type` values:
///
/// - `"parsed_rules"` — JSON-serialized `Vec<PrologRule>` (Prolog sources only).
/// - `"dot"` — uncolored DOT graph (Prolog sources only).
///
/// The generator runs at most once per `(source_id, artifact_type)` pair and
/// is cached in the `source_artifacts` table.
///
/// Returns `text/plain` for DOT, `application/json` for all others.
pub async fn get_source_artifact(
    state: web::Data<AppState>,
    path:  web::Path<(Uuid, String)>,
) -> HttpResponse {
    let (source_id, artifact_type) = path.into_inner();

    let store = match &state.coire_store {
        Some(s) => s.clone(),
        None => {
            return HttpResponse::ServiceUnavailable()
                .json(json!({ "error": "persistence not enabled" }));
        }
    };

    let result = match artifact_type.as_str() {
        "parsed_rules" => {
            store.sources.get_or_create_artifact(
                source_id,
                "parsed_rules",
                None,
                |content| {
                    let rules = clara_cycle::parse_prolog_rules(content);
                    serde_json::to_string(&rules).map_err(clara_coire::CoireError::from)
                },
            )
        }
        "dot" => {
            store.sources.get_or_create_artifact(
                source_id,
                "dot",
                None,
                |content| {
                    use clara_cycle::{DotOptions, generate_dot, parse_prolog_rules};
                    let rules = parse_prolog_rules(content);
                    Ok(generate_dot(&rules, None, &DotOptions::default()))
                },
            )
        }
        other => {
            return HttpResponse::BadRequest().json(json!({
                "error": format!("unsupported artifact_type '{}'", other),
                "supported": ["parsed_rules", "dot"],
            }));
        }
    };

    match result {
        Ok(Some(artifact)) => {
            if artifact_type == "dot" {
                HttpResponse::Ok()
                    .content_type("text/plain; charset=utf-8")
                    .body(artifact.content)
            } else {
                // Return the raw JSON string as application/json so clients can
                // parse it directly without an extra unwrapping layer.
                HttpResponse::Ok()
                    .content_type("application/json")
                    .body(artifact.content)
            }
        }
        Ok(None) => HttpResponse::NotFound()
            .json(json!({ "error": "source not found" })),
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({ "error": e.to_string() })),
    }
}

// ── DELETE /source/{id} ───────────────────────────────────────────────────────

/// Delete a registered source and all its cached artifacts.
pub async fn delete_source(
    state: web::Data<AppState>,
    path:  web::Path<Uuid>,
) -> HttpResponse {
    let source_id = path.into_inner();

    let store = match &state.coire_store {
        Some(s) => s.clone(),
        None => {
            return HttpResponse::ServiceUnavailable()
                .json(json!({ "error": "persistence not enabled" }));
        }
    };

    // Verify existence before deleting so we can return 404 vs 200.
    match store.sources.get_meta(source_id) {
        Ok(None) => {
            return HttpResponse::NotFound()
                .json(json!({ "error": "source not found" }));
        }
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(json!({ "error": e.to_string() }));
        }
        Ok(Some(_)) => {}
    }

    match store.sources.delete(source_id) {
        Ok(()) => HttpResponse::Ok().json(json!({
            "source_id": source_id,
            "status":    "deleted",
        })),
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({ "error": e.to_string() })),
    }
}

// ── Query params ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ArtifactQuery {
    /// Optional TTL override in milliseconds for the generated artifact.
    pub ttl_ms: Option<i64>,
}

// ── POST /source/check-modules ─────────────────────────────────────────────────

/// Body of `POST /source/check-modules`.
#[derive(Debug, Deserialize)]
pub struct CheckModulesRequest {
    /// The node source the modules will load ahead of (a registered id), if any.
    #[serde(default)]
    pub prolog_source_id: Option<Uuid>,
    /// Inline node clauses, used when `prolog_source_id` is absent.
    #[serde(default)]
    pub prolog_clauses: Vec<String>,
    /// `prolog-module` source ids, in load order.
    #[serde(default)]
    pub prolog_module_source_ids: Vec<Uuid>,
}

/// Dry-run the module dependency check a `/deduce` performs before loading: is every module registered with type
/// `prolog-module`, and does any predicate get defined by more than one source (or by a module and a compiled-in
/// overlay export)? Nothing is loaded and no deduction starts, so a caller can fail early, e.g. at activation.
///
/// Always `200` with `{"ok": bool, "conflicts": [{"predicate", "sources"}], "errors": [string]}`; `503` when the
/// source registry is not configured.
pub async fn check_modules(
    state: web::Data<AppState>,
    req:   web::Json<CheckModulesRequest>,
) -> HttpResponse {
    let Some(store) = state.coire_store.clone() else {
        return HttpResponse::ServiceUnavailable().json(json!({ "error": "persistence not enabled" }));
    };
    let req = req.into_inner();

    let outcome = web::block(move || -> Result<Vec<module_sources::Conflict>, ModuleError> {
        let modules = module_sources::resolve_module_sources(&req.prolog_module_source_ids, Some(&store))?;
        let node_content = match req.prolog_source_id {
            Some(id) => match store.sources.get(id) {
                Ok(Some(entry)) => entry.content,
                Ok(None) => return Err(ModuleError::Missing(id)),
                Err(e) => return Err(ModuleError::Lookup(e.to_string())),
            },
            None => req.prolog_clauses.join("\n"),
        };
        let env = clara_prolog::PrologEnvironment::new().map_err(|e| ModuleError::Inspect(e.to_string()))?;
        module_sources::check_modules(&env, "node source", &node_content, &modules)
    })
    .await;

    match outcome {
        Ok(Ok(conflicts)) => HttpResponse::Ok().json(json!({
            "ok": conflicts.is_empty(),
            "conflicts": conflicts,
            "errors": [],
        })),
        Ok(Err(e)) => HttpResponse::Ok().json(json!({
            "ok": false,
            "conflicts": match &e { ModuleError::Conflicts(c) => json!(c), _ => json!([]) },
            "errors": [e.to_string()],
        })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}
