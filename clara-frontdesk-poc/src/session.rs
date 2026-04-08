use std::collections::HashSet;

use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub enum VisitorStatus {
    Active,
    Admitted(String),
    Denied(String),
    Redirected(String, String), // (reason, where)
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
            VisitorStatus::Redirected(_, _) => "redirected",
        }
    }

    pub fn reason(&self) -> &str {
        match self {
            VisitorStatus::Admitted(r) | VisitorStatus::Denied(r) => r,
            VisitorStatus::Redirected(r, _) => r,
            VisitorStatus::Active => "",
        }
    }

    pub fn where_to(&self) -> &str {
        match self {
            VisitorStatus::Redirected(_, w) => w,
            _ => "",
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
    /// Number of user turns processed so far.
    pub exchange_count: u32,
    /// Maximum exchanges before The Keeper loses patience.
    pub patience_limit: u32,
}

impl VisitorSession {
    pub fn new(patience_limit: u32) -> Self {
        Self {
            visitor: "visitor".to_string(),
            conversation: Vec::new(),
            facts: HashSet::new(),
            status: VisitorStatus::Active,
            exchange_count: 0,
            patience_limit,
        }
    }

    pub fn exchanges_remaining(&self) -> u32 {
        self.patience_limit.saturating_sub(self.exchange_count)
    }

    pub fn push_user(&mut self, content: &str) {
        self.conversation.push(json!({"role": "user", "content": content}));
        self.exchange_count += 1;
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
    /// KindlingEvaluator routes `{"prompt": ...}` to OllamaEvaluator.
    /// The system prompt is read from the top-level `"system"` string field.
    /// Conversation history comes from `"context"` (array of role/content objects).
    /// The system prompt must NOT be embedded as context[0].
    ///
    /// `deduction_model` is carried as an in-band field so `run_turn` can
    /// override the model for the post-deduction evaluate call without growing
    /// the function signature further.
    pub fn evaluate_data(&self, system_message: &str, model: &str, deduction_model: &str) -> Value {
        let prompt = self.conversation.last()
            .and_then(|m| m["content"].as_str())
            .unwrap_or("")
            .to_string();

        let history: Vec<Value> = if self.conversation.len() > 1 {
            self.conversation[..self.conversation.len() - 1].to_vec()
        } else {
            vec![]
        };

        json!({
            "prompt":           prompt,
            "system":           system_message,
            "context":          history,
            "model":            model,
            "deduction_model":  deduction_model
        })
    }
}
