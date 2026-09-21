use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use actix::{Actor, ActorContext, AsyncContext, Handler, Message, StreamHandler};
use actix_web::{web, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;

use crate::assistant_client::{
    ack_pending_research, create_session, list_pending_research, list_rulesets, resolve_escalation,
    send, set_session_ruleset, EscalationOutcome, PendingResearchInfo, RulesetInfo, SendResponse,
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
    /// Sent by the client once it has durably persisted a delivered
    /// `research_update` result (sessionStorage) — see
    /// id_ritual_of_rituals_planning.md Part 2.2/2.3. Only once this
    /// arrives does the server call POST .../ack, closing the delivery-
    /// loss window a plain read-marks-delivered GET used to leave open.
    ResearchAck { request_id: String },
    /// The user's Approve/Deny for an action the Ego's Superego escalated (a `research_update` of kind
    /// "escalation"). `decision` is "approve" or "deny"; anything else is refused here, before any call.
    EscalationResolve { request_id: String, decision: String },
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
struct EscalationResolved {
    request_id: String,
    outcome: Result<EscalationOutcome, String>,
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
    /// Last-known state per still-outstanding request_id (researching/
    /// answering) — PENDING_RESEARCH_TICK polls every 5s but a
    /// research_status frame should only go out on an actual state
    /// change, not every tick. Also lets a later tick detect a row that
    /// silently DISAPPEARED from the outstanding set (server marked it
    /// 'failed', e.g. a clara-api restart wiping its deduction — see
    /// research_queue.py's DisRitualNotFound handling) and announce that
    /// as a synthesized "failed" research_status frame, so the client can
    /// retire its in-progress chip instead of it being stuck forever — a
    /// normal completion always passes through 'ready' first (handled
    /// separately below), so anything that vanishes without ever being
    /// 'ready' is exactly the failure case.
    pending_status: HashMap<String, TrackedResearch>,
}

#[derive(Clone)]
struct TrackedResearch {
    kind: String,
    query: String,
    status: String,
}

impl FrontDeskActor {
    fn new(state: Arc<AppState>, token: String, resume_session_id: Option<String>) -> Self {
        Self {
            session_id: resume_session_id.clone(),
            state,
            token,
            resume_session_id,
            pending_status: HashMap::new(),
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
                    IncomingMsg::ResearchAck { request_id } => {
                        self.handle_research_ack(request_id, ctx)
                    }
                    IncomingMsg::EscalationResolve {
                        request_id,
                        decision,
                    } => self.handle_escalation_resolve(request_id, decision, ctx),
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

    /// Forward the user's approve/deny. The backend, not this actor, enforces that the session belongs to
    /// this visitor's token; a refusal comes back to the browser as an `escalation_error` frame.
    fn handle_escalation_resolve(
        &mut self,
        request_id: String,
        decision: String,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        if !valid_decision(&decision) {
            ctx.text(escalation_error_frame(&request_id, "Unknown decision.", false));
            return;
        }
        let Some(session_id) = self.session_id.clone() else {
            ctx.text(escalation_error_frame(
                &request_id,
                "Session not ready yet — try again in a moment.",
                false,
            ));
            return;
        };
        self.pending_status.remove(&request_id);

        let http: Client = self.state.http.clone();
        let base_url = self.state.fiery_pit_url.clone();
        let token = self.token.clone();
        let addr = ctx.address();

        actix::spawn(async move {
            let rid = request_id.clone();
            let result = tokio::task::spawn_blocking(move || {
                resolve_escalation(&http, &base_url, &token, &session_id, &rid, &decision)
            })
            .await;
            let outcome = match result {
                Ok(Ok(out)) => Ok(out),
                Ok(Err(e)) => Err(e.to_string()),
                Err(e) => Err(format!("internal fault: {}", e)),
            };
            addr.do_send(EscalationResolved {
                request_id,
                outcome,
            });
        });
    }

    /// Fire-and-forget ack: the client has already durably persisted the
    /// result (sessionStorage) by the time this arrives, so there is
    /// nothing to send back to it either way — a failed ack just means
    /// the server-side row stays 'ready' and gets acked again on a later
    /// retry (POST .../ack is idempotent), not a user-visible problem.
    fn handle_research_ack(&mut self, request_id: String, _ctx: &mut ws::WebsocketContext<Self>) {
        let Some(session_id) = self.session_id.clone() else {
            return;
        };
        self.pending_status.remove(&request_id);

        let http: Client = self.state.http.clone();
        let base_url = self.state.fiery_pit_url.clone();
        let token = self.token.clone();

        actix::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                ack_pending_research(&http, &base_url, &token, &session_id, &request_id)
            })
            .await;
            if let Ok(Err(e)) = result {
                log::warn!("pending-research ack failed (will retry on next ack attempt): {}", e);
            }
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

impl Handler<EscalationResolved> for FrontDeskActor {
    type Result = ();

    fn handle(&mut self, done: EscalationResolved, ctx: &mut Self::Context) {
        let frame = match done.outcome {
            Ok(out) => escalation_resolved_frame(&done.request_id, &out),
            Err(e) => {
                log::warn!("escalation resolve failed: {}", e);
                let (text, is_final) = friendly_resolve_error(&e);
                escalation_error_frame(&done.request_id, &text, is_final)
            }
        };
        ctx.text(frame);
    }
}

impl Handler<PendingResearchTick> for FrontDeskActor {
    type Result = ();

    fn handle(&mut self, tick: PendingResearchTick, ctx: &mut Self::Context) {
        match tick.outcome {
            Ok(items) => {
                let mut seen = std::collections::HashSet::new();
                for item in items {
                    seen.insert(item.request_id.clone());

                    if item.status == "ready" {
                        // A 'ready' row is delivered every tick until the
                        // client acks it (see handle_research_ack) — the
                        // GET is now a safe, repeatable peek (2026-09-13),
                        // not a drain, so re-sending here is the correct
                        // "at least once until acked" behavior, not a
                        // duplicate-delivery bug. The client is
                        // responsible for de-duping by request_id if a
                        // tick lands again before its own ack round trip
                        // completes.
                        ctx.text(
                            json!({
                                "type": "research_update",
                                "request_id": item.request_id,
                                "kind": item.kind,
                                "query": item.query,
                                "reply": item.reply,
                                "citation_count": item.citation_count,
                                "ref": item.reference,
                            })
                            .to_string(),
                        );
                        self.pending_status.remove(&item.request_id);
                        continue;
                    }

                    // Still running (researching/answering) — only push a
                    // research_status frame on an actual state change, not
                    // every 5s tick, so the in-progress indicator doesn't
                    // thrash.
                    let changed = self
                        .pending_status
                        .get(&item.request_id)
                        .map(|prev| prev.status != item.status)
                        .unwrap_or(true);
                    if changed {
                        ctx.text(
                            json!({
                                "type": "research_status",
                                "request_id": item.request_id,
                                "kind": item.kind,
                                "query": item.query,
                                "status": item.status,
                            })
                            .to_string(),
                        );
                    }
                    self.pending_status.insert(
                        item.request_id.clone(),
                        TrackedResearch {
                            kind: item.kind,
                            query: item.query,
                            status: item.status,
                        },
                    );
                }

                // Anything tracked from a prior tick but absent from this
                // one (and not just resolved via 'ready' above, already
                // removed) vanished from the outstanding set without ever
                // completing — synthesize a "failed" status so the client
                // can retire its in-progress chip instead of it sticking
                // around forever.
                let gone: Vec<String> = self
                    .pending_status
                    .keys()
                    .filter(|k| !seen.contains(*k))
                    .cloned()
                    .collect();
                for request_id in gone {
                    if let Some(tracked) = self.pending_status.remove(&request_id) {
                        ctx.text(
                            json!({
                                "type": "research_status",
                                "request_id": request_id,
                                "kind": tracked.kind,
                                "query": tracked.query,
                                "status": "failed",
                            })
                            .to_string(),
                        );
                    }
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


// ─── Pure helpers (unit-tested below) ─────────────────────────────────────────

fn valid_decision(decision: &str) -> bool {
    decision == "approve" || decision == "deny"
}

fn escalation_resolved_frame(request_id: &str, out: &EscalationOutcome) -> String {
    json!({
        "type": "escalation_resolved",
        "request_id": request_id,
        "state": out.state,
        "executed": out.executed,
        "message": out.message,
    })
    .to_string()
}

/// `is_final`: the backend has retired the request (gone, already handled, expired), so the browser should
/// drop it instead of offering the buttons again.
fn escalation_error_frame(request_id: &str, text: &str, is_final: bool) -> String {
    json!({"type": "escalation_error", "request_id": request_id, "text": text, "final": is_final})
        .to_string()
}

/// Backend refusals worth telling the user about plainly, and whether they are final (the request is gone
/// for good); anything else is a generic, retryable message.
fn friendly_resolve_error(raw: &str) -> (String, bool) {
    if raw.contains("(410)") {
        ("This request expired, so it counts as denied.".to_string(), true)
    } else if raw.contains("(409)") {
        ("This request was already handled.".to_string(), true)
    } else if raw.contains("(404)") || raw.contains("(403)") {
        ("This request is no longer available.".to_string(), true)
    } else {
        ("Couldn't send your answer — please try again.".to_string(), false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> Option<IncomingMsg> {
        serde_json::from_str(raw).ok()
    }

    #[test]
    fn parses_an_escalation_resolve_frame() {
        match parse(r#"{"type":"escalation_resolve","request_id":"abc","decision":"approve"}"#) {
            Some(IncomingMsg::EscalationResolve { request_id, decision }) => {
                assert_eq!(request_id, "abc");
                assert_eq!(decision, "approve");
            }
            _ => panic!("did not parse as EscalationResolve"),
        }
    }

    #[test]
    fn rejects_a_resolve_frame_missing_a_field() {
        assert!(parse(r#"{"type":"escalation_resolve","request_id":"abc"}"#).is_none());
        assert!(parse(r#"{"type":"escalation_resolve","decision":"approve"}"#).is_none());
    }

    #[test]
    fn existing_frames_still_parse() {
        assert!(matches!(parse(r#"{"type":"chat","text":"hi"}"#), Some(IncomingMsg::Chat { .. })));
        assert!(matches!(
            parse(r#"{"type":"research_ack","request_id":"r"}"#),
            Some(IncomingMsg::ResearchAck { .. })
        ));
    }

    #[test]
    fn only_approve_and_deny_are_valid_decisions() {
        assert!(valid_decision("approve") && valid_decision("deny"));
        for bad in ["", "Approve", "maybe", "approve ", "yes"] {
            assert!(!valid_decision(bad), "{bad:?}");
        }
    }

    #[test]
    fn frames_carry_the_request_id_and_the_outcome() {
        let out = EscalationOutcome {
            state: "approved".into(),
            executed: true,
            outcome: Some("published".into()),
            message: "Approved. published".into(),
        };
        let v: serde_json::Value = serde_json::from_str(&escalation_resolved_frame("r1", &out)).unwrap();
        assert_eq!(v["type"], "escalation_resolved");
        assert_eq!(v["request_id"], "r1");
        assert_eq!(v["state"], "approved");
        assert_eq!(v["executed"], true);
        let e: serde_json::Value =
            serde_json::from_str(&escalation_error_frame("r1", "nope", true)).unwrap();
        assert_eq!(e["type"], "escalation_error");
        assert_eq!(e["text"], "nope");
        assert_eq!(e["final"], true);
    }

    #[test]
    fn errors_are_translated_for_the_user() {
        let (t, f) = friendly_resolve_error("resolve_escalation failed (410): expired");
        assert!(t.contains("expired") && f);
        let (t, f) = friendly_resolve_error("resolve_escalation failed (409): x");
        assert!(t.contains("already") && f);
        let (t, f) = friendly_resolve_error("resolve_escalation failed (404): x");
        assert!(t.contains("no longer") && f);
        let (t, f) = friendly_resolve_error("connection refused");
        assert!(t.contains("try again") && !f, "a transient failure must stay retryable");
    }
}
