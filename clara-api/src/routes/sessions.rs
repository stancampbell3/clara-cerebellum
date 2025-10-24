use actix_web::web;
use crate::handlers;

pub fn configure(cfg: &mut web::ServiceConfig) {
    // Create session (POST /sessions)
    cfg.route("/sessions", web::post().to(handlers::create_session));

    // Get session (GET /sessions/{session_id})
    cfg.route("/sessions/{session_id}", web::get().to(handlers::get_session));

    // Terminate session (DELETE /sessions/{session_id})
    cfg.route("/sessions/{session_id}", web::delete().to(handlers::terminate_session));

    // Eval session (POST /sessions/{session_id}/eval)
    cfg.route("/sessions/{session_id}/eval", web::post().to(handlers::eval_session));

    // List user sessions (GET /sessions/user/{user_id})
    cfg.route("/sessions/user/{user_id}", web::get().to(handlers::list_user_sessions));
}
