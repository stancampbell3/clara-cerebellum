//! Clara Cerebrum Core Module
//!
//! This module provides core types, traits, and error handling for the Clara Cerebrum service.
//! It defines:
//! - Error types and result types for the entire application
//! - Request/response types for the REST API
//! - Service traits that define the business logic contracts
//! - Resource management types
//!
//! # Example
//!
//! ```no_run
//! use clara_core::{
//!     types::CreateSessionRequest,
//!     error::{ClaraError, ClaraResult},
//! };
//!
//! let req = CreateSessionRequest::new("user-123".to_string());
//! println!("Creating session for user: {}", req.user_id);
//! ```

pub mod error;
pub mod types;
pub mod traits;
pub mod service;

// Re-export commonly used items
pub use error::{ClaraError, ClaraResult, ErrorResponse};
pub use types::*;
pub use traits::{SessionService, EvalService, LoadService, PersistenceService, ReplProtocol, SecurityFilter};
