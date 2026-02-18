use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use log::info;
use serde_json::Value;
use std::sync::Arc;

use crate::server::McpServer;

type SharedServer = Arc<McpServer>;

async fn mcp_handler(State(srv): State<SharedServer>, Json(body): Json<Value>) -> Json<Value> {
    Json(srv.handle_json(body).await)
}

async fn health_handler() -> &'static str {
    "ok"
}

pub async fn run(server: SharedServer, port: u16) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/mcp", post(mcp_handler))
        .route("/health", get(health_handler))
        .with_state(server);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    info!("HTTP MCP server listening on 0.0.0.0:{}", port);
    axum::serve(listener, app).await?;
    Ok(())
}
