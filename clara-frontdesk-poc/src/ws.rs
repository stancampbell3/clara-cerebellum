use actix::{Actor, ActorContext, AsyncContext, Handler, Message, StreamHandler};
use actix_web::{web, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::assistant_client::{create_session, send, SendResponse};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct WsQuery {
    /// Bearer JWT from POST /login (static/index.html's sessionStorage).
    token: String,
}

// ─── Internal actor message carrying one completed turn ───────────────────────

#[derive(Message)]
#[rtype(result = "()")]
struct TurnResult {
    session_id: String,
    outcome: Result<SendResponse, String>,
}

// ─── Actor ───────────────────────────────────────────────────────────────────

pub struct FrontDeskActor {
    /// Lazily created on the first message — see handle().
    session_id: Option<String>,
    state: Arc<AppState>,
    /// Bearer JWT for THIS visitor's logged-in identity — replaces the old
    /// shared AppState.bearer_token every connection used to reuse.
    token: String,
}

impl FrontDeskActor {
    fn new(state: Arc<AppState>, token: String) -> Self {
        Self {
            session_id: None,
            state,
            token,
        }
    }
}

impl Actor for FrontDeskActor {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.text(
            json!({"type": "agent", "text": self.state.config.company.greeting}).to_string(),
        );
    }
}

// ─── Incoming WS text → dispatch blocking work ────────────────────────────────

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for FrontDeskActor {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(text)) => {
                let text = text.trim().to_string();
                if text.is_empty() {
                    return;
                }

                let http: Client = self.state.http.clone();
                let base_url = self.state.fiery_pit_url.clone();
                let token = self.token.clone();
                let existing_session_id = self.session_id.clone();

                let addr = ctx.address();

                actix::spawn(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        run_turn(&http, &base_url, &token, existing_session_id, &text)
                    })
                    .await;

                    let turn = match result {
                        Ok(Ok((session_id, resp))) => TurnResult {
                            session_id,
                            outcome: Ok(resp),
                        },
                        Ok(Err((session_id, e))) => TurnResult {
                            session_id,
                            outcome: Err(e.to_string()),
                        },
                        Err(e) => TurnResult {
                            session_id: String::new(),
                            outcome: Err(format!("internal fault: {}", e)),
                        },
                    };
                    addr.do_send(turn);
                });
            }
            Ok(ws::Message::Ping(b)) => ctx.pong(&b),
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
            }
            _ => {}
        }
    }
}

// ─── TurnResult handler — update session_id, send WS frame ────────────────────

impl Handler<TurnResult> for FrontDeskActor {
    type Result = ();

    fn handle(&mut self, turn: TurnResult, ctx: &mut Self::Context) {
        if !turn.session_id.is_empty() {
            self.session_id = Some(turn.session_id);
        }

        let msg = match turn.outcome {
            Ok(resp) => json!({
                "type":            "agent",
                "text":            resp.reply,
                "action_taken":    resp.action_taken,
                "workspace_slug":  resp.workspace_slug,
                "citation_count":  resp.citation_count,
            }),
            Err(e) => {
                log::error!("assistant turn failed: {}", e);
                json!({
                    "type": "error",
                    "text": "I ran into a problem answering that — please try again.",
                })
            }
        };

        ctx.text(msg.to_string());
    }
}

// ─── Blocking work (runs in spawn_blocking) ───────────────────────────────────

/// Ensures a session exists (creating one on first use), then sends `text`.
/// Returns the session_id alongside the result either way, so the actor can
/// remember a session created just before a failed `send` call.
fn run_turn(
    http: &Client,
    base_url: &str,
    token: &str,
    existing_session_id: Option<String>,
    text: &str,
) -> Result<(String, SendResponse), (String, crate::assistant_client::AssistantError)> {
    let session_id = match existing_session_id {
        Some(id) => id,
        None => match create_session(http, base_url, token) {
            Ok(id) => id,
            Err(e) => return Err((String::new(), e)),
        },
    };

    match send(http, base_url, token, &session_id, text) {
        Ok(resp) => Ok((session_id, resp)),
        Err(e) => Err((session_id, e)),
    }
}

// ─── Route handler ────────────────────────────────────────────────────────────

pub async fn ws_index(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<AppState>,
    query: web::Query<WsQuery>,
) -> actix_web::Result<HttpResponse> {
    ws::start(
        FrontDeskActor::new(state.into_inner(), query.into_inner().token),
        &req,
        stream,
    )
}
