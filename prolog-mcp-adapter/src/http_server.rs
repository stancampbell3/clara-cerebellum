use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Router,
};
use futures_util::stream::{self, StreamExt};
use log::info;
use serde::Deserialize;
use serde_json::Value;
use std::{collections::HashMap, convert::Infallible, sync::Arc};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

use crate::server::McpServer;

#[derive(Clone)]
struct AppState {
    server: Arc<McpServer>,
    clients: Arc<Mutex<HashMap<String, mpsc::Sender<String>>>>,
}

#[derive(Deserialize)]
struct SessionQuery {
    #[serde(rename = "sessionId")]
    session_id: String,
}

async fn sse_handler(
    State(state): State<AppState>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let session_id = Uuid::new_v4().to_string();
    let (tx, rx) = mpsc::channel::<String>(32);
    state.clients.lock().await.insert(session_id.clone(), tx);

    let endpoint_event = stream::once(async move {
        Ok::<Event, Infallible>(
            Event::default()
                .event("endpoint")
                .data(format!("/message?sessionId={}", session_id)),
        )
    });

    let message_stream = ReceiverStream::new(rx).map(|msg| {
        Ok::<Event, Infallible>(Event::default().event("message").data(msg))
    });

    Sse::new(endpoint_event.chain(message_stream)).keep_alive(KeepAlive::default())
}

async fn message_handler(
    Query(params): Query<SessionQuery>,
    State(state): State<AppState>,
    body: String,
) -> Response {
    let request: Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response();
        }
    };

    let response = state.server.handle_json(request).await;
    let response_str = match serde_json::to_string(&response) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Serialization error: {}", e),
            )
                .into_response();
        }
    };

    if let Some(tx) = state.clients.lock().await.get(&params.session_id) {
        let _ = tx.send(response_str).await;
    }

    StatusCode::ACCEPTED.into_response()
}

async fn health_handler() -> &'static str {
    "ok"
}

pub async fn run(server: Arc<McpServer>, port: u16) -> anyhow::Result<()> {
    let state = AppState {
        server,
        clients: Arc::new(Mutex::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/sse", get(sse_handler))
        .route("/message", post(message_handler))
        .route("/health", get(health_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    info!("SSE MCP server listening on 0.0.0.0:{}", port);
    axum::serve(listener, app).await?;
    Ok(())
}
