/// CAW Runtime Engine
/// Executes parsed CAW programs with rule evaluation and agent messaging

use crate::ast::*;
use crate::types::TypeChecker;
use crate::CawResult;
use serde_json::{json, Value};
use std::collections::HashMap;

/// Runtime engine for executing CAW programs
pub struct Runtime {
    facts: Vec<Fact>,
    rules: Vec<Rule>,
    agents: HashMap<String, Agent>,
    type_checker: TypeChecker,
}

/// A fact in the knowledge base
#[derive(Debug, Clone)]
pub struct Fact {
    pub name: String,
    pub data: Value,
}

/// A rule in the system
#[derive(Debug, Clone)]
pub struct Rule {
    pub name: String,
    pub conditions: Vec<Expression>,
    pub actions: Vec<Statement>,
}

/// An agent in the system
#[derive(Debug, Clone)]
pub struct Agent {
    pub name: String,
    pub domain: DomainPath,
    pub facts: Vec<Fact>,
    pub rules: Vec<Rule>,
}

impl Runtime {
    pub fn new() -> Self {
        Self {
            facts: Vec::new(),
            rules: Vec::new(),
            agents: HashMap::new(),
            type_checker: TypeChecker::new(),
        }
    }

    /// Load and execute a program
    pub fn execute_program(&mut self, program: &Program) -> CawResult<Value> {
        let mut results = Vec::new();

        for statement in &program.statements {
            match statement {
                Statement::TypeDecl(td) => {
                    // Register type in type checker
                    self.type_checker.env_mut().bind(td.name.clone(), td.type_expr.clone());
                }
                Statement::AgentDecl(ad) => {
                    // Create agent
                    let agent = Agent {
                        name: ad.name.clone(),
                        domain: ad.domain.clone(),
                        facts: Vec::new(),
                        rules: Vec::new(),
                    };
                    self.agents.insert(ad.name.clone(), agent);
                }
                Statement::FeatherDecl(fd) => {
                    // Assert fact
                    let fact = Fact {
                        name: fd.name.clone(),
                        data: json!({
                            "type": fd.type_name,
                            "value": self.eval_record(&fd.value)?
                        }),
                    };
                    self.facts.push(fact);
                }
                Statement::RuneDecl(rd) => {
                    // Register rule
                    let rule = Rule {
                        name: rd.name.clone(),
                        conditions: rd.conditions.clone(),
                        actions: rd.actions.clone(),
                    };
                    self.rules.push(rule);
                }
                Statement::Expression(expr) => {
                    // Evaluate expression
                    let result = self.eval_expression(expr)?;
                    results.push(result);
                }
            }
        }

        if results.is_empty() {
            Ok(Value::Null)
        } else if results.len() == 1 {
            Ok(results.into_iter().next().unwrap())
        } else {
            Ok(Value::Array(results))
        }
    }

    /// Evaluate an expression
    pub fn eval_expression(&self, expr: &Expression) -> CawResult<Value> {
        match expr {
            Expression::Literal(lit) => Ok(self.eval_literal(lit)),
            Expression::Identifier(id) => {
                // Look up in facts
                if let Some(fact) = self.facts.iter().find(|f| f.name == *id) {
                    Ok(fact.data.clone())
                } else {
                    Ok(json!({"error": format!("Unknown identifier: {}", id)}))
                }
            }
            Expression::FunctionCall(fc) => self.eval_function_call(fc),
            Expression::Record(rec) => self.eval_record(rec),
            Expression::AgentCall(_) => {
                // TODO: Implement agent calls
                Ok(json!({"status": "agent call not yet implemented"}))
            }
            Expression::MessageSend(_, _) => {
                // TODO: Implement message sending
                Ok(json!({"status": "message send not yet implemented"}))
            }
        }
    }

    fn eval_literal(&self, lit: &Literal) -> Value {
        match lit {
            Literal::String(s) => json!(s),
            Literal::Number(n) => json!(n),
            Literal::Boolean(b) => json!(b),
        }
    }

    fn eval_function_call(&self, fc: &FunctionCall) -> CawResult<Value> {
        // Built-in functions
        match fc.name.as_str() {
            "assert" => {
                Ok(json!({"action": "assert", "args": fc.args.len()}))
            }
            "query" => {
                Ok(json!({"action": "query", "facts": self.facts.len()}))
            }
            "facts" => {
                let fact_names: Vec<String> = self.facts.iter().map(|f| f.name.clone()).collect();
                Ok(json!(fact_names))
            }
            _ => Ok(json!({"function": fc.name.clone(), "args": fc.args.len()})),
        }
    }

    fn eval_record(&self, rec: &Record) -> CawResult<Value> {
        let mut obj = serde_json::Map::new();
        for (key, expr) in &rec.fields {
            let value = self.eval_expression(expr)?;
            obj.insert(key.clone(), value);
        }
        Ok(Value::Object(obj))
    }

    /// Get all facts
    pub fn facts(&self) -> &[Fact] {
        &self.facts
    }

    /// Get all rules
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// Get all agents
    pub fn agents(&self) -> &HashMap<String, Agent> {
        &self.agents
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}
