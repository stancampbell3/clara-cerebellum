//! Clara-Cycle: Reasoning Cycle Controller
//!
//! Drives the `Prolog → relay → CLIPS → relay → convergence` loop that forms
//! the core of Clara's neurosymbolic deduction engine.
//!
//! # Quick start
//!
//! ```ignore
//! use clara_cycle::{DeductionSession, CycleController, CycleStatus};
//! use std::sync::{Arc, atomic::AtomicBool};
//!
//! let mut session = DeductionSession::new()?;
//! session.seed_prolog(&["man(stan).".into(), "mortal(X) :- man(X).".into()])?;
//!
//! let interrupt = Arc::new(AtomicBool::new(false));
//! let mut controller = CycleController::new(
//!     session,
//!     100,
//!     Some("mortal(stan)".into()),
//!     interrupt,
//! );
//!
//! let result = controller.run()?;
//! assert_eq!(result.status, CycleStatus::Converged);
//! ```

pub mod controller;
pub mod error;
pub mod relay;
pub mod result;
pub mod session;
pub mod transpile;

pub use controller::CycleController;
pub use error::CycleError;
pub use result::{CoireSnapshot, CycleStatus, DeductionResult};
pub use session::DeductionSession;
