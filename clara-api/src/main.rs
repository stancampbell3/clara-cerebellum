use clara_api::start_server;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logging
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();

    log::info!("Starting Clara Cerebrum API Server");

    // Initialize global ToolboxManager with default tools
    log::info!("Initializing ToolboxManager");
    clara_toolbox::ToolboxManager::init_global();

    // Start server on localhost:8080
    start_server("0.0.0.0", 8080).await
}
