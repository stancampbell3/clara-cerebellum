use std::sync::Arc;
use std::time::Duration;

use actix::{Actor, ActorContext, AsyncContext, Handler, Message, StreamHandler};
use actix_web::{web, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;

use crate::assistant_client::{
    create_session, list_pending_research, list_rulesets, send, set_session_ruleset,
    PendingResearchInfo, RulesetInfo, SendResponse,
};
use crate::state::AppState;

/// Poll interval for GET /assistant/sessions/{id}/pending-research — how
/// often a "research update" alert can appear after a deferred_query
/// reply. Independent of, and much shorter than, lildaemon's own
/// PENDING_RESEARCH_POLL_INTERVAL_SECONDS (how often IT advances a
/// request's state) — this just checks for anything already marked ready.
const PENDING_RESEARCH_TICK: Duration = Duration::from_secs(5);

#[derive(Deserialize)]
pub struct WsQuery {
    /// Bearer JWT from POST /login (static/index.html's sessionStorage).
    token: String,
    /// Set on reconnect if the browser remembered a prior session_id
    /// (same sessionStorage entry as the token) — lets that session's
    /// pending-research alerts actually reach it again after a page
    /// reload. Absent on a brand-new tab, which gets a fresh session.
    session_id: Option<String>,
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

/// Session created (or resumed) and rulesets fetched eagerly on connect
/// (not lazily on first message) — the ruleset dropdown needs a
/// session_id to target before the visitor has sent anything.
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

#[derive(Message)]
#[rtype(result = "()")]
struct PendingResearchTick {
    outcome: Result<Vec<PendingResearchInfo>, String>,
}

// ─── Actor ───────────────────────────────────────────────────────────────────

pub struct FrontDeskActor {
    /// Set once SessionReady arrives (or immediately, if resumed from a
    /// query-param session_id) — a chat message that races ahead of that
    /// still falls back to lazy creation in run_turn.
    session_id: Option<String>,
    state: Arc<AppState>,
    /// Bearer JWT for THIS visitor's logged-in identity — replaces the old
    /// shared AppState.bearer_token every connection used to reuse.
    token: String,
    /// From the WS query string, if the browser remembered a prior
    /// session — consumed once in started(), then cleared.
    resume_session_id: Option<String>,
}

impl FrontDeskActor {
    fn new(state: Arc<AppState>, token: String, resume_session_id: Option<String>) -> Self {
        Self {
            session_id: resume_session_id.clone(),
            state,
            token,
            resume_session_id,
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
        let resume_session_id = self.resume_session_id.clone();
        let addr = ctx.address();

        actix::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                init_session(&http, &base_url, &token, resume_session_id)
            })
            .await;
            let outcome = match result {
                Ok(Ok(ready)) => Ok(ready),
                Ok(Err(e)) => Err(e.to_string()),
                Err(e) => Err(format!("internal fault: {}", e)),
            };
            addr.do_send(SessionReady { outcome });
        });

        ctx.run_interval(PENDING_RESEARCH_TICK, |act, ctx| {
            let Some(session_id) = act.session_id.clone() else {
                return;
            };
            let http: Client = act.state.http.clone();
            let base_url = act.state.fiery_pit_url.clone();
            let token = act.token.clone();
            let addr = ctx.address();

            actix::spawn(async move {
                let result = tokio::task::spawn_blocking(move || {
                    list_pending_research(&http, &base_url, &token, &session_id)
                })
                .await;
                let outcome = match result {
                    Ok(Ok(items)) => Ok(items),
                    Ok(Err(e)) => Err(e.to_string()),
                    Err(e) => Err(format!("internal fault: {}", e)),
                };
                addr.do_send(PendingResearchTick { outcome });
            });
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

impl Handler<PendingResearchTick> for FrontDeskActor {
    type Result = ();

    fn handle(&mut self, tick: PendingResearchTick, ctx: &mut Self::Context) {
        match tick.outcome {
            Ok(items) => {
                for item in items {
                    ctx.text(
                        json!({
                            "type": "research_update",
                            "query": item.query,
                            "reply": item.reply,
                            "citation_count": item.citation_count,
                        })
                        .to_string(),
                    );
                }
            }
            Err(e) => {
                // Not fatal — a transient failure just gets retried next tick.
                log::warn!("pending-research poll failed: {}", e);
            }
        }
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

/// Resume the browser-supplied session (if any), else create a new one;
/// fetch the ruleset list either way.
fn init_session(
    http: &Client,
    base_url: &str,
    token: &str,
    resume_session_id: Option<String>,
) -> Result<(String, Vec<RulesetInfo>), crate::assistant_client::AssistantError> {
    let session_id = match resume_session_id {
        Some(id) => id,
        None => create_session(http, base_url, token)?,
    };
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
    let query = query.into_inner();
    ws::start(
        FrontDeskActor::new(state.into_inner(), query.token, query.session_id),
        &req,
        stream,
    )
}
