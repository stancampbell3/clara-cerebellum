use clara_api::start_server;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logging
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();

    log::info!("Starting Clara Cerebrum API Server");

    // Start server on localhost:8080
    start_server("127.0.0.1", 8080).await
}
