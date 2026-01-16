//! Clara Cerebrum Session Management Module
//!
//! This module provides session lifecycle management for reasoning engine sessions.
//! It supports:
//! - CLIPS (LilDaemon) and Prolog (LilDevils) sessions
//! - Session creation and termination
//! - Resource tracking and limits
//! - Session metadata and status
//! - In-memory session storage
//!
//! # Example
//!
//! ```no_run
//! use clara_session::{SessionManager, ManagerConfig, SessionType};
//!
//! let manager = SessionManager::new(ManagerConfig::default());
//!
//! // Create a CLIPS session (default)
//! let clips_session = manager.create_session("user-123".to_string(), None)?;
//!
//! // Create a Prolog session (LilDevils)
//! let prolog_session = manager.create_prolog_session("user-123".to_string(), None)?;
//!
//! println!("Created CLIPS session: {}", clips_session.session_id);
//! println!("Created Prolog session: {}", prolog_session.session_id);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod metadata;
pub mod store;
pub mod manager;

// Stub modules for future implementation
pub mod lifecycle;
pub mod queue;
pub mod eviction;

pub use metadata::{Session, SessionId, SessionStatus, SessionStats, SessionType, ResourceUsage, ResourceLimits};
pub use store::{SessionStore, StoreError};
pub use manager::{SessionManager, ManagerConfig, ManagerError};
