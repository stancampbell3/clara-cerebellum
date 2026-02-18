use anyhow::Result;
use env_logger::Env;
use log::info;
use std::env;
use std::sync::Arc;

mod client;
mod http_server;
mod schemas;
mod server;
mod tools;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    info!("Starting Prolog MCP Adapter (LilDevils)");

    let rest_api_url = env::var("REST_API_URL")
        .unwrap_or_else(|_| "http://localhost:8080".to_string());

    let transport = env::var("TRANSPORT").unwrap_or_else(|_| "stdio".to_string());

    let http_port: u16 = env::var("HTTP_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(1968);

    info!("Connecting to REST API at: {}", rest_api_url);
    info!("Transport: {}", transport);

    let server = Arc::new(server::McpServer::new(rest_api_url));
    server.initialize().await?;

    match transport.as_str() {
        "http" => http_server::run(server, http_port).await?,
        _ => server.run_stdio().await?,
    }

    Ok(())
}
