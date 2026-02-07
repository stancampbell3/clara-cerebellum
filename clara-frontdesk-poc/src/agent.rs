use crate::config::FrontDeskConfig;
use fiery_pit_client::{CreateSessionRequest, FieryPitClient};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;

const PROLOG_RULES: &str = include_str!("prolog_rules.pl");

#[derive(Debug, Serialize, Clone)]
pub struct AgentResponse {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub text: String,
    pub state: String,
    pub intent: String,
    pub turn: u32,
}

pub struct FrontDeskAgent {
    fiery_pit: Arc<FieryPitClient>,
    current_state: String,
    config: FrontDeskConfig,
    turn_count: u32,
    history: Vec<(String, String)>,
}

// FrontDeskAgent is Send since all fields are Send
// (FieryPitClient wraps reqwest::blocking::Client which is Send)
unsafe impl Send for FrontDeskAgent {}

impl FrontDeskAgent {
    /// Create a new agent. Must be called from a blocking context (not inside tokio runtime).
    pub fn new(
        fiery_pit: Arc<FieryPitClient>,
        config: FrontDeskConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        match fiery_pit.set_evaluator_typed("ember") {
            Ok(resp) => log::info!("Set evaluator: status={}, evaluator={:?}", resp.status, resp.evaluator),
            Err(e) => return Err(format!("Failed to set evaluator to ember: {}", e).into()),
        }

        log::info!("FrontDeskAgent initialized.");

        Ok(Self {
            fiery_pit,
            current_state: "greeting".to_string(),
            config,
            turn_count: 0,
            history: Vec::new(),
        })
    }

    pub fn greeting(&self) -> AgentResponse {
        AgentResponse {
            msg_type: "message".to_string(),
            text: self.config.formatted_greeting(),
            state: "greeting".to_string(),
            intent: "greeting".to_string(),
            turn: 0,
        }
    }

    /// Handle a user message. Must be called from a blocking context.
    pub fn handle_message(&mut self, user_input: &str) -> AgentResponse {
        self.turn_count += 1;
        self.history
            .push(("user".to_string(), user_input.to_string()));

        let intent = match self.classify_intent(user_input) {
            Ok(i) => i,
            Err(e) => {
                log::error!("Intent classification failed: {}", e);
                "unknown".to_string()
            }
        };
        log::info!(
            "Turn {}: intent={}, state={}",
            self.turn_count,
            intent,
            self.current_state
        );

        let (next_state, action) = match self.query_transition(&intent) {
            Ok((ns, a)) => (ns, a),
            Err(e) => {
                log::error!("Prolog transition failed: {}", e);
                ("inquiry".to_string(), "gather_intent".to_string())
            }
        };
        log::info!("Transition: {} -> {} (action: {})", self.current_state, next_state, action);

        if let Err(e) = self.assert_context(&intent) {
            log::warn!("Failed to assert context: {}", e);
        }

        let response_text = match self.generate_response(&action) {
            Ok(text) => text,
            Err(e) => {
                log::error!("Response generation failed: {}", e);
                "I apologize, but I'm having trouble processing your request. Could you try again?"
                    .to_string()
            }
        };

        self.current_state = next_state.clone();
        self.history
            .push(("agent".to_string(), response_text.clone()));

        AgentResponse {
            msg_type: "message".to_string(),
            text: response_text,
            state: next_state,
            intent,
            turn: self.turn_count,
        }
    }

    pub fn is_farewell(&self) -> bool {
        self.current_state == "farewell"
    }

    pub fn cleanup(&self) {
        log::info!("Closing FronDeskAgent");
    }

    fn classify_intent(&self, user_input: &str) -> Result<String, Box<dyn std::error::Error>> {
        let prompt = format!(
            "You are classifying user intent for a front desk agent at {}.\n\
             Classify the following message into one of these intents:\n\
             product_info, service_info, contact_info, hours_info, farewell, followup, unknown\n\n\
             User message: \"{}\"\n\
             Previous state: {}\n\n\
             Respond with ONLY the intent label, nothing else.",
            self.config.company.name, user_input, self.current_state
        );

        let tephra = self.fiery_pit.evaluate_tephra(json!({
            "prompt": prompt,
            "model" : "qwen2.5:7b", // TODO: make this configurable
        }))?;
        let result = tephra.into_response()?;

        let intent_raw = extract_text_from_value(&result);
        let intent = intent_raw.trim().to_lowercase();

        let valid_intents = [
            "product_info",
            "service_info",
            "contact_info",
            "hours_info",
            "farewell",
            "followup",
            "unknown",
        ];

        if valid_intents.contains(&intent.as_str()) {
            Ok(intent)
        } else {
            log::warn!("LLM returned unexpected intent '{}', defaulting to unknown", intent);
            Ok("unknown".to_string())
        }
    }

    fn query_transition(
        &self,
        intent: &str,
    ) -> Result<(String, String), Box<dyn std::error::Error>> {
        let goal = format!(
            "next_state({}, {}, NextState, Action)",
            self.current_state, intent
        );

        // let result = self
        //   .fiery_pit
        //    .prolog_query(&self.prolog_session_id, &goal, false)?;
        // Process this on our evaluator by wrapping it in a "goal" JSON object for our Evaluator to recognize and handle with our Prolog rules
        let query_json = json!({
            "goal": {
                "predicate": "next_state",
                "args": [self.current_state.clone(), intent.to_string()],
                "vars": ["NextState", "Action"]
            },
            "model" : "qwen2.5:7b", // TODO: make this configurable
        });

        let tephra = self.fiery_pit.evaluate_tephra(query_json)?;
        let result = tephra.into_response()?;
        let (next_state, action) = parse_prolog_bindings(&result)?;
        Ok((next_state, action))
    }

    fn assert_context(&self, intent: &str) -> Result<(), Box<dyn std::error::Error>> {
        let clauses = vec![
            format!(
                "conversation_context(session, turn_count, {}).",
                self.turn_count
            ),
            format!("conversation_context(session, last_intent, {}).", intent),
        ];

        // TODO: implement this
        Ok(())
    }

    fn generate_response(&self, action: &str) -> Result<String, Box<dyn std::error::Error>> {
        let recent_history: String = self
            .history
            .iter()
            .rev()
            .take(6)
            .rev()
            .map(|(role, text)| format!("{}: {}", role, text))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            "You are {}, the front desk assistant at {}. {}\n\
             Current action: {}\n\
             Company info: {}\n\
             Conversation history:\n{}\n\n\
             Generate a helpful, friendly, concise response. Stay in character.\n\
             Do not include any prefixes like 'Agent:' or 'Ember:'. Just provide the response text.",
            self.config.agent.name,
            self.config.company.name,
            self.config.company.tagline,
            action,
            self.config.company_context_summary(),
            recent_history
        );

        let tephra = self.fiery_pit.evaluate_tephra(json!(
            {
                "prompt": prompt,
                "model" : "qwen2.5:7b", // TODO: make this configurable
            }
        ))?;
        let result = tephra.into_response()?;

        let text = extract_text_from_value(&result);
        Ok(text.trim().to_string())
    }
}

fn parse_prolog_bindings(result: &Value) -> Result<(String, String), Box<dyn std::error::Error>> {
    if let Some(bindings) = result.get("bindings") {
        if let Some(obj) = bindings.as_object() {
            let next = extract_atom(obj.get("NextState"))
                .or_else(|| extract_atom(obj.get("Next")))
                .unwrap_or_else(|| "inquiry".to_string());
            let action = extract_atom(obj.get("Action"))
                .unwrap_or_else(|| "gather_intent".to_string());
            return Ok((next, action));
        }
        if let Some(arr) = bindings.as_array() {
            if let Some(first) = arr.first().and_then(|v| v.as_object()) {
                let next = extract_atom(first.get("NextState"))
                    .or_else(|| extract_atom(first.get("Next")))
                    .unwrap_or_else(|| "inquiry".to_string());
                let action = extract_atom(first.get("Action"))
                    .unwrap_or_else(|| "gather_intent".to_string());
                return Ok((next, action));
            }
        }
    }

    if let Some(res) = result.get("result") {
        if let Some(obj) = res.as_object() {
            let next = extract_atom(obj.get("NextState"))
                .or_else(|| extract_atom(obj.get("Next")))
                .unwrap_or_else(|| "inquiry".to_string());
            let action = extract_atom(obj.get("Action"))
                .unwrap_or_else(|| "gather_intent".to_string());
            return Ok((next, action));
        }
    }

    if let Some(solutions) = result.get("solutions").and_then(|v| v.as_array()) {
        if let Some(first) = solutions.first().and_then(|v| v.as_object()) {
            let next = extract_atom(first.get("NextState"))
                .or_else(|| extract_atom(first.get("Next")))
                .unwrap_or_else(|| "inquiry".to_string());
            let action = extract_atom(first.get("Action"))
                .unwrap_or_else(|| "gather_intent".to_string());
            return Ok((next, action));
        }
    }

    log::warn!(
        "Could not parse Prolog bindings from response: {}, using defaults",
        result
    );
    Ok(("inquiry".to_string(), "gather_intent".to_string()))
}

fn extract_atom(val: Option<&Value>) -> Option<String> {
    val.and_then(|v| {
        if let Some(s) = v.as_str() {
            Some(s.to_string())
        } else {
            Some(v.to_string().trim_matches('"').to_string())
        }
    })
}

/// Extract a text string from a Tephra inner response value.
/// The response from the Ember evaluator may be a plain string, or an object
/// with a "result", "response", or "text" field.
fn extract_text_from_value(value: &Value) -> String {
    if let Some(s) = value.as_str() {
        return s.to_string();
    }
    if let Some(obj) = value.as_object() {
        for key in &["result", "response", "text", "output", "content"] {
            if let Some(s) = obj.get(*key).and_then(|v| v.as_str()) {
                return s.to_string();
            }
        }
    }
    value.to_string()
}
