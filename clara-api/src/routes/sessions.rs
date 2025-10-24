use actix_web::web;
use crate::handlers;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/sessions")
            .route("", web::post().to(handlers::create_session))
            .route("/{session_id}", web::get().to(handlers::get_session))
            .route("/{session_id}", web::delete().to(handlers::terminate_session))
            .route("/{session_id}/eval", web::post().to(handlers::eval_session))
            .route("/user/{user_id}", web::get().to(handlers::list_user_sessions)),
    );
}
