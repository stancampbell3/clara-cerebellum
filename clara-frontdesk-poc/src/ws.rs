use actix::{Actor, ActorContext, AsyncContext, Handler, Message, StreamHandler};
use actix_web::{web, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::deduce::{extract_list_var, extract_named_solutions, extract_str_var, run_deduce};
use crate::session::{VisitorSession, VisitorStatus};
use crate::state::AppState;

// ─── Internal actor message carrying one completed turn ───────────────────────

#[derive(Message)]
#[rtype(result = "()")]
struct TurnResult {
    assistant_text: String,
    new_status: Option<VisitorStatus>,
}

// ─── Actor ───────────────────────────────────────────────────────────────────

pub struct FrontDeskActor {
    session: VisitorSession,
    state: Arc<AppState>,
}

impl FrontDeskActor {
    fn new(state: Arc<AppState>) -> Self {
        let patience = state.config.company.patience;
        Self {
            session: VisitorSession::new(patience),
            state,
        }
    }
}

impl Actor for FrontDeskActor {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let greeting = format!(
            "Welcome to the {}. I am {}. State your business.",
            self.state.config.company.name,
            self.state.config.company.agent_name,
        );
        self.session.push_assistant(&greeting);
        ctx.text(
            json!({"type": "agent", "text": greeting, "status": "active"}).to_string(),
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

                if self.session.status.is_terminal() {
                    ctx.text(
                        json!({"type": "error", "text": "This session has concluded.",
                               "status": self.session.status.label()})
                        .to_string(),
                    );
                    return;
                }

                // Increment exchange count and check patience BEFORE any HTTP work.
                self.session.push_user(&text);
                let remaining = self.session.exchanges_remaining();

                if remaining == 0 {
                    self.session.status = VisitorStatus::Denied(
                        "The Keeper has grown weary of this interview.".to_string(),
                    );
                    ctx.text(json!({
                        "type":   "terminal",
                        "status": "denied",
                        "text":   "Enough. This audience is concluded. Remove yourself.",
                        "reason": "The Keeper has grown weary of this interview.",
                        "where":  ""
                    }).to_string());
                    ctx.close(None);
                    return;
                }

                // Snapshot data for the blocking closure.
                let clara_api_url = self.state.clara_api_url.clone();
                let clara_pl_path = self.state.clara_pl_path.clone();
                let clara_clp_path = self.state.clara_clp_path.clone();
                let system_prompt = self.state.config.company.system_prompt.clone();
                let model = self.state.config.company.model.clone();
                let fp_client = self.state.fiery_pit.clone();

                let prolog_clauses = self.session.prolog_clauses(&clara_pl_path);
                let deduce_context = self.session.deduce_context(&system_prompt);
                let evaluate_data = self.session.evaluate_data(&system_prompt, &model);

                let addr = ctx.address();

                actix::spawn(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        run_turn(
                            &clara_api_url,
                            &clara_clp_path,
                            prolog_clauses,
                            deduce_context,
                            evaluate_data,
                            remaining,
                            &fp_client,
                        )
                    })
                    .await;

                    let turn = match result {
                        Ok(Ok(t)) => t,
                        Ok(Err(e)) => TurnResult {
                            assistant_text: format!("A bureaucratic error has occurred. {}", e),
                            new_status: None,
                        },
                        Err(e) => TurnResult {
                            assistant_text: format!("Internal fault: {}", e),
                            new_status: None,
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

// ─── TurnResult handler — update session state, send WS frame ─────────────────

impl Handler<TurnResult> for FrontDeskActor {
    type Result = ();

    fn handle(&mut self, turn: TurnResult, ctx: &mut Self::Context) {
        self.session.push_assistant(&turn.assistant_text);

        if let Some(ref status) = turn.new_status {
            self.session.status = status.clone();
        }

        let terminal = self.session.status.is_terminal();
        let status_label = self.session.status.label().to_string();

        let msg = if terminal {
            json!({
                "type":   "terminal",
                "status": status_label,
                "text":   turn.assistant_text,
                "reason": self.session.status.reason(),
                "where":  self.session.status.where_to()
            })
        } else {
            json!({
                "type":   "agent",
                "text":   turn.assistant_text,
                "status": status_label
            })
        };

        ctx.text(msg.to_string());

        if terminal {
            ctx.close(None);
        }
    }
}

// ─── Blocking work (runs in spawn_blocking) ───────────────────────────────────

fn run_turn(
    clara_api_url: &str,
    clara_clp_path: &str,
    prolog_clauses: Vec<String>,
    deduce_context: Vec<Value>,
    evaluate_data: Value,
    exchanges_remaining: u32,
    fp_client: &fiery_pit_client::FieryPitClient,
) -> Result<TurnResult, Box<dyn std::error::Error + Send + Sync>> {
    let http = Client::new();

    log::debug!("run_turn: prolog_clauses={:?}", prolog_clauses);

    // Single deduce call: daemonic_turn/5 returns suggestions + decision in one shot.
    log::debug!("run_turn: running daemonic_turn deduce");
    let (suggestions, new_status) = match run_deduce(
        &http,
        clara_api_url,
        prolog_clauses,
        clara_clp_path,
        "daemonic_turn(visitor, Suggestions, Decision, Reason, Where).",
        deduce_context,
        5,
    ) {
        Ok(ref result) => {
            let sol = extract_named_solutions(result);
            log::debug!("run_turn: named solution={:?}", sol);
            let suggestions = extract_list_var(&sol, "Suggestions");
            let status = interpret_decision(&sol);
            (suggestions, status)
        }
        Err(e) => {
            log::warn!("daemonic_turn deduce failed: {}", e);
            (vec![], None)
        }
    };

    log::debug!("run_turn: suggestions={:?}", suggestions);
    log::debug!("run_turn: new_status={:?}", new_status.as_ref().map(|s| s.label()));

    // Augment system prompt with suggestions, decision, and patience warning.
    let augmented_system = build_system_message(
        evaluate_data["system"].as_str().unwrap_or(""),
        &suggestions,
        &new_status,
        exchanges_remaining,
    );
    log::debug!("run_turn: augmented system prompt:\n{}", augmented_system);

    let mut eval_payload = evaluate_data;
    eval_payload["system"] = Value::String(augmented_system);

    // Call /evaluate — typed Tephra path, extract content from hohi.response.content.
    log::debug!(
        "run_turn: evaluate payload: {}",
        serde_json::to_string_pretty(&eval_payload).unwrap_or_default()
    );
    // after receiving `tephra` from fp_client.evaluate_tephra(...)
    let assistant_text = match fp_client.evaluate_tephra(eval_payload) {
        Ok(tephra) => {
            // Try multiple possible locations for the evaluator text:
            // 1) response.content (preferred)
            // 2) response (string field inside object)
            // 3) response itself is a string value
            let text_opt = tephra
                .response() // Option<&serde_json::Value>
                .and_then(|r| {
                    // Attempt response.content OR response
                    r.get("content")
                        .or_else(|| r.get("response"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        // If neither key exists but response is a plain string, use that.
                        .or_else(|| r.as_str().map(|s| s.to_string()))
                });

            // Fallback and helpful debug logging if nothing found
            text_opt.unwrap_or_else(|| {
                log::debug!(
                "evaluate_tephra returned unexpected shape: {:?}",
                tephra.response()
            );
                "(no response from evaluator)".to_string()
            })
        }
        Err(e) => {
            log::error!("FieryPit evaluate error: {}", e);
            "I am unable to process your request at this time. Please try again.".to_string()
        }
    };

    log::debug!("run_turn: assistant_text={:?}", assistant_text);

    Ok(TurnResult {
        assistant_text,
        new_status,
    })
}

/// Map the Decision atom from daemonic_turn/5 to a VisitorStatus.
/// Expects Decision ∈ { admitted, denied, redirected, pending }.
fn interpret_decision(sol: &std::collections::HashMap<String, serde_json::Value>) -> Option<VisitorStatus> {
    let decision = extract_str_var(sol, "Decision");
    let reason   = extract_str_var(sol, "Reason");
    let where_to = extract_str_var(sol, "Where");

    match decision.as_str() {
        "admitted"   => Some(VisitorStatus::Admitted(reason)),
        "denied"     => Some(VisitorStatus::Denied(reason)),
        "redirected" => Some(VisitorStatus::Redirected(reason, where_to)),
        _            => None, // "pending" or empty → stay Active
    }
}

fn build_system_message(
    base: &str,
    suggestions: &[String],
    new_status: &Option<VisitorStatus>,
    exchanges_remaining: u32,
) -> String {
    let mut parts = vec![base.to_string()];

    if !suggestions.is_empty() {
        parts.push(format!(
            "\n\nCurrent admittance system guidance:\n{}",
            suggestions
                .iter()
                .map(|s| format!("- {}", s))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    match new_status {
        Some(VisitorStatus::Admitted(reason)) => {
            parts.push(format!(
                "\n\nDecision: GRANT ENTRY. Reason on record: {}\n\
                 Inform the visitor they are admitted. Maintain your grim formal tone.",
                reason
            ));
        }
        Some(VisitorStatus::Redirected(reason, where_to)) => {
            parts.push(format!(
                "\n\nDecision: DENY / REDIRECT. Reason: {}\nDirect the visitor to: {}.\n\
                 Be firm but not needlessly cruel.",
                reason, where_to
            ));
        }
        Some(VisitorStatus::Denied(reason)) => {
            parts.push(format!(
                "\n\nDecision: DENY. Reason: {}\nInform the visitor they are not admitted.",
                reason
            ));
        }
        _ => {}
    }

    // Patience warning for penultimate and final turns (before exhaustion fires).
    if exchanges_remaining <= 2 && new_status.is_none() {
        parts.push(
            "\n\nYou are growing impatient. This visitor is testing your considerable tolerance. \
             Make clear this is their final opportunity to state legitimate business."
                .to_string(),
        );
    }

    parts.join("")
}

// ─── Route handler ────────────────────────────────────────────────────────────

pub async fn ws_index(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<AppState>,
) -> actix_web::Result<HttpResponse> {
    ws::start(FrontDeskActor::new(state.into_inner()), &req, stream)
}
