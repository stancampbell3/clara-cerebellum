use actix_web::{web, App, HttpServer};
use clara_coire::CarrionPicker;
use clara_cycle::CoireStore;
use clara_session::{SessionManager, ManagerConfig};
use clara_config::ConfigLoader;
use log::info;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::Duration;

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

    // Subprocesses are created lazily on first session request, not during startup
    info!("Subprocess pool initialized (lazy creation enabled).");

    // Optionally open the Coire persistent store.
    let coire_store = if let Some(ref path) = config.persistence.coire_store_path {
        match CoireStore::open(path) {
            Ok(store) => {
                info!("Coire persistent store opened at: {}", path);
                Some(store)
            }
            Err(e) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to open Coire store at '{}': {}", path, e),
                ));
            }
        }
    } else {
        info!("Coire persistent store not configured — mailboxes will not be persisted.");
        None
    };

    // Shared set of session UUIDs currently held by running deductions.
    // Read by the carrion-picker to avoid deleting live mailboxes.
    let active_coire_sessions: Arc<RwLock<HashSet<uuid::Uuid>>> =
        Arc::new(RwLock::new(HashSet::new()));

    // Spawn the carrion-picker if a store is open.
    if let Some(ref store) = coire_store {
        let coire_ttl    = config.persistence.coire_store_ttl_seconds;
        let snapshot_ttl = config.persistence.deduction_snapshot_ttl_seconds;
        let interval     = config.persistence.coire_store_sweep_interval_seconds;
        let picker = CarrionPicker::new(
            store.clone(),
            Duration::from_secs(coire_ttl),
            Duration::from_secs(snapshot_ttl),
            Duration::from_secs(interval.max(1)),
            active_coire_sessions.clone(),
        );
        picker.spawn();
        info!(
            "CarrionPicker spawned (coire_ttl={}s, snapshot_ttl={}s, interval={}s)",
            coire_ttl, snapshot_ttl, interval
        );
    }

    let snapshot_ttl_ms =
        (config.persistence.deduction_snapshot_ttl_seconds as i64).saturating_mul(1000);

    // Create app state
    let app_state = web::Data::new(AppState {
        session_manager,
        subprocess_pool,
        deductions: Arc::new(RwLock::new(HashMap::new())),
        coire_store,
        active_coire_sessions,
        snapshot_ttl_ms,
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
            deductions: Arc::new(RwLock::new(HashMap::new())),
            coire_store: None,
            active_coire_sessions: Arc::new(RwLock::new(HashSet::new())),
            snapshot_ttl_ms: 604_800_000,
        };
        // Just verify it can be created
        let _cloned = state.clone();
    }
}
