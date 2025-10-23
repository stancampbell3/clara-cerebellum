//! Clara Cerebrum Session Management Module
//!
//! This module provides session lifecycle management for CLIPS evaluation sessions.
//! It supports:
//! - Session creation and termination
//! - Resource tracking and limits
//! - Session metadata and status
//! - In-memory session storage
//!
//! # Example
//!
//! ```no_run
//! use clara_session::{SessionManager, ManagerConfig};
//!
//! let manager = SessionManager::new(ManagerConfig::default());
//! let session = manager.create_session("user-123".to_string(), None)?;
//!
//! println!("Created session: {}", session.session_id);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod metadata;
pub mod store;
pub mod manager;

// Stub modules for future implementation
pub mod lifecycle;
pub mod queue;
pub mod eviction;

pub use metadata::{Session, SessionId, SessionStatus, ResourceUsage, ResourceLimits};
pub use store::{SessionStore, StoreError};
pub use manager::{SessionManager, ManagerConfig, ManagerError};
