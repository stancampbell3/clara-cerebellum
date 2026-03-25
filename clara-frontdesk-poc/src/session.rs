use std::collections::HashSet;

use serde_json::{json, Value};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum VisitorStatus {
    Active,
    Admitted(String),
    Denied(String),
    Redirected(String),
}

impl VisitorStatus {
    pub fn is_terminal(&self) -> bool {
        !matches!(self, VisitorStatus::Active)
    }

    pub fn label(&self) -> &str {
        match self {
            VisitorStatus::Active => "active",
            VisitorStatus::Admitted(_) => "admitted",
            VisitorStatus::Denied(_) => "denied",
            VisitorStatus::Redirected(_) => "redirected",
        }
    }
}

pub struct VisitorSession {
    /// Prolog atom used for this visitor in all clauses/goals.
    pub visitor: String,
    /// Full conversation history as role/content objects.
    pub conversation: Vec<Value>,
    /// Known Prolog facts (besides visitor/1) to assert into each deduce call.
    pub facts: HashSet<String>,
    pub status: VisitorStatus,
}

impl VisitorSession {
    pub fn new() -> Self {
        Self {
            visitor: "visitor".to_string(),
            conversation: Vec::new(),
            facts: HashSet::new(),
            status: VisitorStatus::Active,
        }
    }

    pub fn push_user(&mut self, content: &str) {
        self.conversation.push(json!({"role": "user", "content": content}));
    }

    pub fn push_assistant(&mut self, content: &str) {
        self.conversation.push(json!({"role": "assistant", "content": content}));
        // Mark as greeted after first agent response.
        self.facts.insert(format!("greeted({})", self.visitor));
    }

    /// Build the prolog_clauses array for a /deduce call.
    pub fn prolog_clauses(&self, pl_path: &str) -> Vec<String> {
        let mut clauses = vec![
            format!("consult('{}').", pl_path),
            format!("visitor({}).", self.visitor),
        ];
        for fact in &self.facts {
            clauses.push(format!("{}.", fact));
        }
        clauses
    }

    /// Build the context array passed to /deduce (full conversation).
    pub fn deduce_context(&self, system_prompt: &str) -> Vec<Value> {
        let mut ctx = vec![json!({"role": "system", "content": system_prompt})];
        ctx.extend(self.conversation.iter().cloned());
        ctx
    }

    /// Build the evaluate payload for the KindlingEvaluator LLM path.
    ///
    /// KindlingEvaluator.evaluate_async() routes `{"prompt": ...}` to OllamaEvaluator.
    /// It reads the system prompt from the top-level `"system"` string field and
    /// conversation history from `"context"` (array of role/content objects).
    /// The system prompt must NOT be embedded as context[0] — KindlingEvaluator reads
    /// `data.get("system")` directly and the OllamaEvaluator will prepend it to the
    /// messages array only when the first context entry is not already a system message.
    pub fn evaluate_data(&self, system_message: &str) -> Value {
        // Last message is the current user turn (sent as "prompt").
        let prompt = self.conversation.last()
            .and_then(|m| m["content"].as_str())
            .unwrap_or("")
            .to_string();

        // Prior turns only — no system message embedded here.
        let history: Vec<Value> = if self.conversation.len() > 1 {
            self.conversation[..self.conversation.len() - 1].to_vec()
        } else {
            vec![]
        };

        json!({
            "prompt":  prompt,
            "system":  system_message,
            "context": history,
            "model":   "qwen-clara:latest"
        })
    }

}
