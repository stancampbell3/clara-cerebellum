use actix::{Actor, ActorContext, AsyncContext, Handler, Message, StreamHandler};
use actix_web::{web, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::assistant_client::{
    create_session, list_rulesets, send, set_session_ruleset, RulesetInfo, SendResponse,
};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct WsQuery {
    /// Bearer JWT from POST /login (static/index.html's sessionStorage).
    token: String,
}

/// Incoming WS frames are JSON-enveloped by `type` — plain-text chat was
/// replaced once a second message kind (ruleset switching) was needed.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum IncomingMsg {
    Chat { text: String },
    SetRuleset { ruleset_key: String },
}

// ─── Internal actor messages ───────────────────────────────────────────────

#[derive(Message)]
#[rtype(result = "()")]
struct TurnResult {
    session_id: String,
    outcome: Result<SendResponse, String>,
}

/// Session created and rulesets fetched eagerly on connect (not lazily on
/// first message) — the ruleset dropdown needs a session_id to target
/// before the visitor has sent anything.
#[derive(Message)]
#[rtype(result = "()")]
struct SessionReady {
    outcome: Result<(String, Vec<RulesetInfo>), String>,
}

#[derive(Message)]
#[rtype(result = "()")]
struct RulesetSetResult {
    outcome: Result<String, String>,
}

// ─── Actor ───────────────────────────────────────────────────────────────────

pub struct FrontDeskActor {
    /// Set once SessionReady arrives; a chat message that races ahead of
    /// that still falls back to lazy creation in run_turn.
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

        let http: Client = self.state.http.clone();
        let base_url = self.state.fiery_pit_url.clone();
        let token = self.token.clone();
        let addr = ctx.address();

        actix::spawn(async move {
            let result = tokio::task::spawn_blocking(move || init_session(&http, &base_url, &token))
                .await;
            let outcome = match result {
                Ok(Ok(ready)) => Ok(ready),
                Ok(Err(e)) => Err(e.to_string()),
                Err(e) => Err(format!("internal fault: {}", e)),
            };
            addr.do_send(SessionReady { outcome });
        });
    }
}

// ─── Incoming WS text → dispatch blocking work ────────────────────────────────

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for FrontDeskActor {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(raw)) => {
                let raw = raw.trim().to_string();
                if raw.is_empty() {
                    return;
                }
                let parsed: IncomingMsg = match serde_json::from_str(&raw) {
                    Ok(m) => m,
                    Err(e) => {
                        log::warn!("ws: unparseable frame ignored: {} ({})", raw, e);
                        return;
                    }
                };

                match parsed {
                    IncomingMsg::Chat { text } => self.handle_chat(text, ctx),
                    IncomingMsg::SetRuleset { ruleset_key } => {
                        self.handle_set_ruleset(ruleset_key, ctx)
                    }
                }
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

impl FrontDeskActor {
    fn handle_chat(&mut self, text: String, ctx: &mut ws::WebsocketContext<Self>) {
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

    fn handle_set_ruleset(&mut self, ruleset_key: String, ctx: &mut ws::WebsocketContext<Self>) {
        let Some(session_id) = self.session_id.clone() else {
            ctx.text(
                json!({"type": "error", "text": "Session not ready yet — try again in a moment."})
                    .to_string(),
            );
            return;
        };

        let http: Client = self.state.http.clone();
        let base_url = self.state.fiery_pit_url.clone();
        let token = self.token.clone();
        let addr = ctx.address();

        actix::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                set_session_ruleset(&http, &base_url, &token, &session_id, &ruleset_key)
                    .map(|_| ruleset_key)
            })
            .await;

            let outcome = match result {
                Ok(Ok(key)) => Ok(key),
                Ok(Err(e)) => Err(e.to_string()),
                Err(e) => Err(format!("internal fault: {}", e)),
            };
            addr.do_send(RulesetSetResult { outcome });
        });
    }
}

// ─── Message handlers — update state, send WS frames ──────────────────────────

impl Handler<SessionReady> for FrontDeskActor {
    type Result = ();

    fn handle(&mut self, ready: SessionReady, ctx: &mut Self::Context) {
        match ready.outcome {
            Ok((session_id, rulesets)) => {
                self.session_id = Some(session_id.clone());
                ctx.text(json!({"type": "session", "session_id": session_id}).to_string());
                ctx.text(json!({"type": "rulesets", "rulesets": rulesets}).to_string());
            }
            Err(e) => {
                log::error!("session init failed: {}", e);
                // Not fatal — run_turn's lazy fallback still creates a
                // session on the first chat message.
            }
        }
    }
}

impl Handler<RulesetSetResult> for FrontDeskActor {
    type Result = ();

    fn handle(&mut self, result: RulesetSetResult, ctx: &mut Self::Context) {
        let msg = match result.outcome {
            Ok(ruleset_key) => json!({"type": "ruleset_set", "ruleset_key": ruleset_key}),
            Err(e) => {
                log::error!("set_ruleset failed: {}", e);
                json!({"type": "error", "text": "Couldn't switch rulesets — please try again."})
            }
        };
        ctx.text(msg.to_string());
    }
}

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

/// Eagerly create the session and fetch the ruleset list on connect.
fn init_session(
    http: &Client,
    base_url: &str,
    token: &str,
) -> Result<(String, Vec<RulesetInfo>), crate::assistant_client::AssistantError> {
    let session_id = create_session(http, base_url, token)?;
    let rulesets = list_rulesets(http, base_url, token)?;
    Ok((session_id, rulesets))
}

/// Ensures a session exists (creating one on first use — a fallback for a
/// chat message that races ahead of SessionReady), then sends `text`.
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
