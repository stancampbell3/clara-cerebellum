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

    /// Build the evaluate payload: context = history minus last user turn,
    /// prompt = last user turn (so OllamaFish appends it as final user message).
    pub fn evaluate_data(&self, system_message: &str) -> Value {
        // Split conversation: everything except the last message (which is the user turn)
        let history = if self.conversation.len() > 1 {
            &self.conversation[..self.conversation.len() - 1]
        } else {
            &[]
        };

        let prompt = self.conversation.last()
            .and_then(|m| m["content"].as_str())
            .unwrap_or("")
            .to_string();

        let mut ctx = vec![json!({"role": "system", "content": system_message})];
        ctx.extend(history.iter().cloned());

        json!({
            "prompt":  prompt,
            "context": ctx,
            "model":   "qwen2.5:7b"
        })
    }

}
