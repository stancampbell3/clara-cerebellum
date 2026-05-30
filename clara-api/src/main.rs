use clara_api::start_server;
use clara_config::ConfigLoader;
use clara_ritual::{InMemoryBroker, KafkaBridge, RsKafkaClient};
use std::sync::Arc;

fn main() -> std::io::Result<()> {
    // Load .env file if present (silently ignored if missing)
    let _ = dotenvy::dotenv();

    // Initialize logging
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();

    log::info!("Starting Clara Cerebrum API Server");

    // Initialize global Coire (shared event mailbox)
    log::info!("Initializing Coire");
    clara_coire::init_global().expect("Failed to initialize Coire");

    // Initialize global ToolboxManager with default tools
    // NOTE: Must happen BEFORE async runtime starts because splinteredmind tool
    // uses reqwest::blocking::Client which can't be created inside async context
    log::info!("Initializing ToolboxManager");
    clara_toolbox::ToolboxManager::init_global();

    // Initialize Prolog with clara_evaluate/2 foreign predicate
    log::info!("Initializing Prolog (LilDevils)");
    clara_prolog::init_global();

    // Build the Ritual broker BEFORE the actix runtime starts.
    // RsKafkaClient owns a dedicated tokio runtime; constructing it inside
    // an existing async runtime panics ("Cannot start a runtime from within
    // a runtime"). This mirrors the FieryPitClient / ToolboxManager pattern.
    let mut config = ConfigLoader::from_env(None)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other,
            format!("Failed to load config: {}", e)))?;

    // KAFKA_BOOTSTRAP env var wins over config file (used by Docker deployments)
    if let Ok(val) = std::env::var("KAFKA_BOOTSTRAP") {
        if !val.is_empty() {
            config.server.kafka_bootstrap = Some(val);
        }
    }

    let ritual_broker: Arc<dyn KafkaBridge> = if let Some(ref bootstrap) = config.server.kafka_bootstrap {
        let brokers: Vec<String> = bootstrap
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();
        match RsKafkaClient::new(brokers) {
            Ok(client) => {
                log::info!("RitualRegistry: using RsKafkaClient (bootstrap={})", bootstrap);
                Arc::new(client)
            }
            Err(e) => {
                return Err(std::io::Error::new(std::io::ErrorKind::Other,
                    format!("Failed to connect RsKafkaClient to '{}': {}", bootstrap, e)));
            }
        }
    } else {
        log::info!("RitualRegistry: using InMemoryBroker (kafka_bootstrap not configured)");
        Arc::new(InMemoryBroker::new())
    };

    // Start the async runtime and server
    actix_web::rt::System::new().block_on(async {
        start_server("0.0.0.0", 8080, ritual_broker).await
    })
}
