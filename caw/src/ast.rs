/// Abstract Syntax Tree definitions for the CAW language
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    TypeDecl(TypeDecl),
    AgentDecl(AgentDecl),
    FeatherDecl(FeatherDecl),
    RuneDecl(RuneDecl),
    Expression(Expression),
}

// Type declarations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeDecl {
    pub name: String,
    pub type_expr: TypeExpr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TypeExpr {
    Primitive(PrimitiveType),
    Record(Vec<(String, Box<TypeExpr>)>),
    Union(Box<TypeExpr>, Box<TypeExpr>),
    Vector(Box<TypeExpr>),
    Function(Vec<TypeExpr>, Box<TypeExpr>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PrimitiveType {
    String,
    Number,
    Boolean,
}

impl fmt::Display for PrimitiveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrimitiveType::String => write!(f, "String"),
            PrimitiveType::Number => write!(f, "Number"),
            PrimitiveType::Boolean => write!(f, "Boolean"),
        }
    }
}

// Agent declarations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentDecl {
    pub name: String,
    pub domain: DomainPath,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainPath {
    pub segments: Vec<String>,
    pub wildcard: bool, // true if ends with ._
}

impl fmt::Display for DomainPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.segments.join("."))?;
        if self.wildcard {
            write!(f, "._")?;
        }
        Ok(())
    }
}

// Feather declarations (facts)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatherDecl {
    pub name: String,
    pub type_name: String,
    pub value: Record,
}

// Rule declarations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuneDecl {
    pub name: String,
    pub conditions: Vec<Expression>,
    pub actions: Vec<Statement>,
}

// Expressions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expression {
    Literal(Literal),
    Identifier(String),
    FunctionCall(FunctionCall),
    AgentCall(AgentCall),
    MessageSend(Box<Expression>, Box<Expression>),
    Record(Record),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    String(String),
    Number(f64),
    Boolean(bool),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub args: Vec<Expression>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentCall {
    pub agent: String,
    pub method: String,
    pub args: Vec<Expression>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub fields: Vec<(String, Expression)>,
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expression::Literal(lit) => write!(f, "{}", lit),
            Expression::Identifier(id) => write!(f, "{}", id),
            Expression::FunctionCall(fc) => write!(f, "{}(...)", fc.name),
            Expression::AgentCall(ac) => write!(f, "{}.{}(...)", ac.agent, ac.method),
            Expression::MessageSend(lhs, rhs) => write!(f, "{} ! {}", lhs, rhs),
            Expression::Record(_) => write!(f, "{{...}}"),
        }
    }
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal::String(s) => write!(f, "\"{}\"", s),
            Literal::Number(n) => write!(f, "{}", n),
            Literal::Boolean(b) => write!(f, "{}", b),
        }
    }
}
