//! Clara Cerebrum REST API
//!
//! Provides HTTP endpoints for session management and CLIPS evaluation.

pub mod handlers;
pub mod models;
pub mod routes;
pub mod server;
pub mod middleware;
pub mod validation;

pub use server::start_server;
