use crate::config::FrontDeskConfig;
use fiery_pit_client::{CreateSessionRequest, FieryPitClient, FieryPitError};
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
    ember_session_id: String,
    current_state: String,
    config: FrontDeskConfig,
    turn_count: u32,
    history: Vec<(String, String)>,
}

// FrontDeskAgent is Send since all fields are Send
// (FieryPitClient wraps reqwest::blocking::Client which is Send)
unsafe impl Send for FrontDeskAgent {}

// Utility extraction function to get string from JSON value, handling different possible formats
fn extract_session_id(v: &Result<serde_json::Value, FieryPitError>) -> Option<String> {
    match v {
        Ok(json) => {
            if let Some(s) = json.as_str() {
                Some(s.to_string())
            } else if let Some(obj) = json.as_object() {
                obj.get("session_id")
                    .or_else(|| obj.get("id"))
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
            } else {
                None
            }
        }
        Err(e) => {
            log::error!("Error from Fiery Pit: {}", e);
            None
        }
    }
}

impl FrontDeskAgent {
    /// Create a new agent. Must be called from a blocking context (not inside tokio runtime).
    pub fn new(
        fiery_pit: Arc<FieryPitClient>,
        config: FrontDeskConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let evaluator_resp = fiery_pit.set_evaluator("ember");
        // todo : handle error

        // Robustly extract a session id from several possible shapes
        let ember_session_id = if let Some(id) = extract_session_id(&evaluator_resp) {
            id
        } else {
            return Err("Missing session_id in Fiery Pit response".into());
        };

        log::info!("Created Ember session: {}", ember_session_id);

        Ok(Self {
            fiery_pit,
            ember_session_id: ember_session_id,
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

    /// Terminate the Prolog session. Must be called from a blocking context.
    pub fn cleanup(&self) {
        log::info!("Done with Ember session: {}", self.ember_session_id);
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

        let result = self.fiery_pit.evaluate(json!(prompt))?;

        let intent_raw = if let Some(s) = result.as_str() {
            s.to_string()
        } else if let Some(s) = result.get("result").and_then(|v| v.as_str()) {
            s.to_string()
        } else if let Some(s) = result.get("response").and_then(|v| v.as_str()) {
            s.to_string()
        } else {
            result.to_string()
        };

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

        let result = self
            .fiery_pit
            .prolog_query(&self.ember_session_id, &goal, false)?;

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

        self.fiery_pit
            .prolog_consult(&self.ember_session_id, clauses)?;
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

        let result = self.fiery_pit.evaluate(json!(prompt))?;

        let text = if let Some(s) = result.as_str() {
            s.to_string()
        } else if let Some(s) = result.get("result").and_then(|v| v.as_str()) {
            s.to_string()
        } else if let Some(s) = result.get("response").and_then(|v| v.as_str()) {
            s.to_string()
        } else {
            result.to_string()
        };

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
