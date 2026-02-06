use crate::agent::FrontDeskAgent;
use crate::config::FrontDeskConfig;
use actix::prelude::*;
use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use fiery_pit_client::FieryPitClient;
use std::sync::{Arc, Mutex};

pub struct ChatSession {
    agent: Option<Arc<Mutex<FrontDeskAgent>>>,
    fiery_pit: Arc<FieryPitClient>,
    config: FrontDeskConfig,
}

impl ChatSession {
    pub fn new(fiery_pit: Arc<FieryPitClient>, config: FrontDeskConfig) -> Self {
        Self {
            agent: None,
            fiery_pit,
            config,
        }
    }
}

impl Actor for ChatSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("WebSocket session started");

        let fp = self.fiery_pit.clone();
        let cfg = self.config.clone();

        // Create the agent on a blocking thread (FieryPitClient is sync)
        let fut = actix::fut::wrap_future::<_, Self>(async move {
            tokio::task::spawn_blocking(move || FrontDeskAgent::new(fp, cfg)).await
        });

        ctx.wait(fut.map(|result, actor: &mut Self, ctx: &mut ws::WebsocketContext<Self>| {
            match result {
                Ok(Ok(agent)) => {
                    let greeting = agent.greeting();
                    let json = serde_json::to_string(&greeting).unwrap_or_default();
                    ctx.text(json);
                    actor.agent = Some(Arc::new(Mutex::new(agent)));
                }
                Ok(Err(e)) => {
                    log::error!("Failed to create FrontDeskAgent: {}", e);
                    let error_msg = serde_json::json!({
                        "type": "error",
                        "text": format!("Failed to initialize agent: {}", e),
                        "state": "error",
                        "intent": "",
                        "turn": 0
                    });
                    ctx.text(error_msg.to_string());
                    ctx.close(None);
                }
                Err(e) => {
                    log::error!("Blocking task panicked: {}", e);
                    ctx.close(None);
                }
            }
        }));
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::info!("WebSocket session stopped");

        // Cleanup the Prolog session on a blocking thread
        if let Some(agent_arc) = self.agent.take() {
            std::thread::spawn(move || {
                if let Ok(agent) = agent_arc.lock() {
                    agent.cleanup();
                }
            });
        }
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ChatSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(text)) => {
                let user_input = text.trim().to_string();
                if user_input.is_empty() {
                    return;
                }

                log::info!("Received message: {}", user_input);

                if let Some(agent_arc) = self.agent.clone() {
                    // Run the blocking FieryPit calls on a blocking thread
                    let fut = actix::fut::wrap_future::<_, Self>(async move {
                        tokio::task::spawn_blocking(move || {
                            let mut agent = agent_arc.lock().unwrap();
                            let response = agent.handle_message(&user_input);
                            let is_farewell = agent.is_farewell();
                            (response, is_farewell)
                        })
                        .await
                    });

                    // ctx.wait pauses processing further messages until this completes
                    ctx.wait(fut.map(
                        |result, _actor: &mut Self, ctx: &mut ws::WebsocketContext<Self>| {
                            match result {
                                Ok((response, is_farewell)) => {
                                    match serde_json::to_string(&response) {
                                        Ok(json) => ctx.text(json),
                                        Err(e) => {
                                            log::error!("Failed to serialize response: {}", e);
                                            let error_msg = serde_json::json!({
                                                "type": "error",
                                                "text": "Internal error",
                                                "state": "error",
                                                "intent": "",
                                                "turn": 0
                                            });
                                            ctx.text(error_msg.to_string());
                                        }
                                    }
                                    if is_farewell {
                                        log::info!(
                                            "Conversation reached farewell state, closing"
                                        );
                                        ctx.close(None);
                                    }
                                }
                                Err(e) => {
                                    log::error!("Blocking task panicked: {}", e);
                                    let error_msg = serde_json::json!({
                                        "type": "error",
                                        "text": "Internal processing error",
                                        "state": "error",
                                        "intent": "",
                                        "turn": 0
                                    });
                                    ctx.text(error_msg.to_string());
                                }
                            }
                        },
                    ));
                } else {
                    let error_msg = serde_json::json!({
                        "type": "error",
                        "text": "Agent not initialized yet, please wait",
                        "state": "error",
                        "intent": "",
                        "turn": 0
                    });
                    ctx.text(error_msg.to_string());
                }
            }
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Pong(_)) => {}
            Ok(ws::Message::Close(reason)) => {
                log::info!("WebSocket close received");
                ctx.close(reason);
                ctx.stop();
            }
            Ok(ws::Message::Binary(_)) => {
                log::warn!("Binary messages not supported");
            }
            Ok(ws::Message::Continuation(_)) => {
                log::warn!("Continuation frames not supported");
            }
            Ok(ws::Message::Nop) => {}
            Err(e) => {
                log::error!("WebSocket protocol error: {}", e);
                ctx.stop();
            }
        }
    }
}

pub async fn ws_handler(
    req: HttpRequest,
    stream: web::Payload,
    fiery_pit: web::Data<Arc<FieryPitClient>>,
    config: web::Data<FrontDeskConfig>,
) -> Result<HttpResponse, Error> {
    let session = ChatSession::new(fiery_pit.get_ref().clone(), config.get_ref().clone());
    ws::start(session, &req, stream)
}
