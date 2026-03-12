use actix::{Actor, ActorContext, AsyncContext, Handler, Message, StreamHandler};
use actix_web::{web, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::deduce::{extract_solutions, run_deduce};
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
        Self {
            session: VisitorSession::new(),
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

                self.session.push_user(&text);

                // Snapshot data needed by the blocking closure.
                let clara_api_url = self.state.clara_api_url.clone();
                let clara_pl_path = self.state.clara_pl_path.clone();
                let clara_clp_path = self.state.clara_clp_path.clone();
                let system_prompt = self.state.config.company.system_prompt.clone();
                let fp_client = self.state.fiery_pit.clone();

                let prolog_clauses = self.session.prolog_clauses(&clara_pl_path);
                let deduce_context = self.session.deduce_context(&system_prompt);
                let evaluate_data = self.session.evaluate_data(&system_prompt);

                let addr = ctx.address();

                actix::spawn(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        run_turn(
                            &clara_api_url,
                            &clara_clp_path,
                            prolog_clauses,
                            deduce_context,
                            evaluate_data,
                            &fp_client,
                        )
                    })
                    .await;

                    let turn = match result {
                        Ok(Ok(t)) => t,
                        Ok(Err(e)) => TurnResult {
                            assistant_text: format!(
                                "A bureaucratic error has occurred. {}",
                                e
                            ),
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

        if let Some(status) = turn.new_status {
            self.session.status = status;
        }

        let status_label = self.session.status.label().to_string();
        let terminal = self.session.status.is_terminal();

        ctx.text(
            json!({
                "type":     "agent",
                "text":     turn.assistant_text,
                "status":   status_label
            })
            .to_string(),
        );

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
    fp_client: &fiery_pit_client::FieryPitClient,
) -> Result<TurnResult, Box<dyn std::error::Error + Send + Sync>> {
    let http = Client::new();

    log::debug!("run_turn: prolog_clauses={:?}", prolog_clauses);

    // 1. Suggestions
    log::debug!("run_turn: running suggestions deduce");
    let suggestions = run_deduce(
        &http,
        clara_api_url,
        prolog_clauses.clone(),
        clara_clp_path,
        "suggestion(visitor, S).",
        deduce_context.clone(),
        5,
    )
    .map(|r| extract_solutions(&r, "S"))
    .unwrap_or_else(|e| {
        log::warn!("Suggestions deduce failed: {}", e);
        vec![]
    });
    log::debug!("run_turn: suggestions={:?}", suggestions);

    // 2. Admittance
    log::debug!("run_turn: running admittance deduce");
    let admit_reasons = run_deduce(
        &http,
        clara_api_url,
        prolog_clauses,
        clara_clp_path,
        "admit(visitor, Reason).",
        deduce_context,
        5,
    )
    .map(|r| extract_solutions(&r, "Reason"))
    .unwrap_or_else(|e| {
        log::warn!("Admittance deduce failed: {}", e);
        vec![]
    });
    log::debug!("run_turn: admit_reasons={:?}", admit_reasons);

    // 3. Interpret admittance
    let new_status = interpret_admit(&admit_reasons);
    log::debug!("run_turn: new_status={:?}", new_status.as_ref().map(|s| s.label()));

    // 4. Augment system message with suggestions + decision
    let augmented_system = build_system_message(
        evaluate_data["context"][0]["content"]
            .as_str()
            .unwrap_or(""),
        &suggestions,
        &new_status,
    );
    log::debug!("run_turn: augmented system prompt:\n{}", augmented_system);

    let mut eval_payload = evaluate_data;
    if let Some(ctx_arr) = eval_payload["context"].as_array_mut() {
        if let Some(system_msg) = ctx_arr.get_mut(0) {
            system_msg["content"] = Value::String(augmented_system);
        }
    }

    // 5. Call /evaluate
    log::debug!(
        "run_turn: evaluate payload: {}",
        serde_json::to_string_pretty(&eval_payload).unwrap_or_default()
    );
    let raw_tephra = fp_client.evaluate(eval_payload);
    log::debug!(
        "run_turn: evaluate raw response: {}",
        match &raw_tephra {
            Ok(v) => serde_json::to_string_pretty(v).unwrap_or_default(),
            Err(e) => format!("ERROR: {}", e),
        }
    );

    let assistant_text = match raw_tephra {
        Ok(tephra) => tephra["hohi"]["response"]["response"]
            .as_str()
            .unwrap_or("(no response from evaluator)")
            .to_string(),
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

fn interpret_admit(reasons: &[String]) -> Option<VisitorStatus> {
    for reason in reasons {
        let lower = reason.to_lowercase();
        if lower.contains("grant entry") {
            return Some(VisitorStatus::Admitted(reason.clone()));
        }
        if lower.contains("do not admit") || lower.contains("direct to") {
            return Some(VisitorStatus::Redirected(reason.clone()));
        }
    }
    None
}

fn build_system_message(
    base: &str,
    suggestions: &[String],
    new_status: &Option<VisitorStatus>,
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

    if let Some(status) = new_status {
        match status {
            VisitorStatus::Admitted(reason) => {
                parts.push(format!(
                    "\n\nDecision: GRANT ENTRY. Reason on record: {}\nInform the visitor they are admitted. \
                     Maintain your grim formal tone.",
                    reason
                ));
            }
            VisitorStatus::Redirected(reason) => {
                parts.push(format!(
                    "\n\nDecision: DENY / REDIRECT. Reason on record: {}\nInform the visitor they cannot enter \
                     and direct them accordingly. Be firm but not cruel.",
                    reason
                ));
            }
            _ => {}
        }
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
