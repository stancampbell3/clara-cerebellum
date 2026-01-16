pub mod sessions;
pub mod health;
pub mod metrics;
pub mod eval;
pub mod devils;

use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    // Register all routes in a single scope to avoid conflicts
    cfg.service(
        web::scope("")
            // Health routes
            .route("/healthz", web::get().to(health::health))
            .route("/readyz", web::get().to(health::ready))
            .route("/livez", web::get().to(health::live))
            // Metrics route
            .route("/metrics", web::get().to(metrics::metrics))
            // Session routes (CLIPS/LilDaemon)
            .route("/sessions", web::post().to(sessions::create_session))
            .route("/sessions", web::get().to(sessions::list_all_sessions))
            .route("/sessions/user/{user_id}", web::get().to(sessions::list_user_sessions))
            .route("/sessions/{session_id}", web::get().to(sessions::get_session))
            .route("/sessions/{session_id}", web::delete().to(sessions::terminate_session))
            .route("/sessions/{session_id}/evaluate", web::post().to(sessions::eval_session))
            .route("/sessions/{session_id}/save", web::post().to(sessions::save_session))
            .route("/sessions/{session_id}/rules", web::post().to(sessions::load_rules))
            .route("/sessions/{session_id}/facts", web::post().to(sessions::load_facts))
            .route("/sessions/{session_id}/facts", web::get().to(sessions::query_facts))
            .route("/sessions/{session_id}/run", web::post().to(sessions::run_rules))
            // Devils routes (Prolog/LilDevils)
            .route("/devils/sessions", web::post().to(devils::create_prolog_session))
            .route("/devils/sessions", web::get().to(devils::list_prolog_sessions))
            .route("/devils/sessions/{session_id}", web::get().to(devils::get_prolog_session))
            .route("/devils/sessions/{session_id}", web::delete().to(devils::terminate_prolog_session))
            .route("/devils/sessions/{session_id}/query", web::post().to(devils::query_prolog))
            .route("/devils/sessions/{session_id}/consult", web::post().to(devils::consult_prolog))
    );
}
