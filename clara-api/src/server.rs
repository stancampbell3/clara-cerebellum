use actix_web::{web, App, HttpServer};
use clara_session::{SessionManager, ManagerConfig};
use log::info;

use crate::handlers::AppState;
use crate::routes;

/// Start the Actix-web server
pub async fn start_server(
    host: &str,
    port: u16,
) -> std::io::Result<()> {
    let addr = format!("{}:{}", host, port);
    info!("Starting Clara API server on {}", addr);

    // Create session manager with default config
    let session_config = ManagerConfig {
        max_concurrent_sessions: 100,
        max_sessions_per_user: 10,
    };
    let session_manager = SessionManager::new(session_config);

    // Create app state
    let app_state = web::Data::new(AppState {
        session_manager,
    });

    // Create and start server
    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .wrap(actix_web::middleware::Logger::default())
            .configure(routes::configure)
    })
    .bind(&addr)?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_creation() {
        let config = ManagerConfig::default();
        let manager = SessionManager::new(config);
        let state = AppState {
            session_manager: manager,
        };
        // Just verify it can be created
        let _cloned = state.clone();
    }
}
