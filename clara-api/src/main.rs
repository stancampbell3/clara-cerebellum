use clara_api::start_server;

fn main() -> std::io::Result<()> {
    // Initialize logging
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();

    log::info!("Starting Clara Cerebrum API Server");

    // Initialize global ToolboxManager with default tools
    // NOTE: Must happen BEFORE async runtime starts because splinteredmind tool
    // uses reqwest::blocking::Client which can't be created inside async context
    log::info!("Initializing ToolboxManager");
    clara_toolbox::ToolboxManager::init_global();

    // Initialize Prolog with clara_evaluate/2 foreign predicate
    log::info!("Initializing Prolog (LilDevils)");
    clara_prolog::init_global();

    // Start the async runtime and server
    actix_web::rt::System::new().block_on(async {
        start_server("0.0.0.0", 8080).await
    })
}
