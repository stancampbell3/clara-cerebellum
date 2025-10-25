use anyhow::Result;
use env_logger::Env;
use log::info;
use std::env;

mod server;
mod tools;
mod client;
mod schemas;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    info!("Starting CLIPS MCP Adapter");

    let rest_api_url = env::var("REST_API_URL")
        .unwrap_or_else(|_| "http://localhost:8080".to_string());

    info!("Connecting to REST API at: {}", rest_api_url);

    // Start the MCP server on stdin/stdout
    let server = server::McpServer::new(rest_api_url);
    server.run().await?;

    Ok(())
}
