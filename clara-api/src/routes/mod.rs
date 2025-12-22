pub mod sessions;
pub mod health;
pub mod metrics;
pub mod eval;

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
            // Session routes
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
    );
}
