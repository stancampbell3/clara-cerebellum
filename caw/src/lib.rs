#[macro_use]
extern crate pest_derive;

use thiserror::Error;

pub mod ast;
pub mod parser;
pub mod runtime;
pub mod transpiler;
pub mod types;
pub mod pretty_print;
pub mod repl;

#[cfg(test)]
mod tests;

// Re-export main types
pub use ast::*;
pub use parser::CawParser;
pub use runtime::Runtime;
pub use transpiler::ClipsTranspiler;
pub use repl::{ReplSession, ReplCommand};

#[derive(Error, Debug)]
pub enum CawError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Type error: {0}")]
    TypeError(String),

    #[error("Runtime error: {0}")]
    RuntimeError(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

pub type CawResult<T> = Result<T, CawError>;

/// Version information
pub const VERSION: &str = "0.1.0";
pub const LANGUAGE_NAME: &str = "CAW";
