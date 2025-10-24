use actix_web::{web, App, HttpServer};
use clara_session::{SessionManager, ManagerConfig};
use clara_config::ConfigLoader;
use log::info;

use crate::handlers::AppState;
use crate::routes;
use crate::subprocess::SubprocessPool;

/// Start the Actix-web server
pub async fn start_server(
    host: &str,
    port: u16,
) -> std::io::Result<()> {
    let addr = format!("{}:{}", host, port);
    info!("Starting Clara API server on {}", addr);

    // Load configuration
    let config = ConfigLoader::from_env(None)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to load config: {}", e)))?;
    
    info!("Using CLIPS binary at: {}", config.clips.binary_path);

    // Create session manager with config from file
    let session_config = ManagerConfig {
        max_concurrent_sessions: config.sessions.max_concurrent,
        max_sessions_per_user: config.sessions.max_per_user,
    };
    let session_manager = SessionManager::new(session_config);

    // Create subprocess pool with configured paths
    let subprocess_pool = SubprocessPool::new(
        config.clips.binary_path.clone(),
        config.clips.sentinel_marker.clone(),
    );
    
    // Preload the subprocess pool to ensure the subprocess is ready
    info!("Preloading CLIPS subprocess pool...");
    subprocess_pool.ensure_subprocess("preload").map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to preload subprocess: {}", e))
    })?;
    info!("CLIPS subprocess pool preloaded.");

    // Create app state
    let app_state = web::Data::new(AppState {
        session_manager,
        subprocess_pool,
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
        let pool = SubprocessPool::new(
            "./clips".to_string(),
            "__END__".to_string(),
        );
        let state = AppState {
            session_manager: manager,
            subprocess_pool: pool,
        };
        // Just verify it can be created
        let _cloned = state.clone();
    }
}
