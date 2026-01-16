//! Clara-Prolog: Rust wrapper for SWI-Prolog integration (LilDevils)
//!
//! This crate provides a safe Rust interface to embedded SWI-Prolog engines,
//! enabling Prolog-based logical reasoning within the Clara neurosymbolic system.
//!
//! # Architecture
//!
//! - `PrologEnvironment`: Safe wrapper around a SWI-Prolog engine (one per session)
//! - FFI bindings to SWI-Prolog C API
//! - Callbacks from Prolog to Rust via `clara_evaluate/2` predicate
//!
//! # Example
//!
//! ```ignore
//! use clara_prolog::PrologEnvironment;
//!
//! let env = PrologEnvironment::new()?;
//! env.assertz("parent(tom, mary)")?;
//! env.assertz("parent(tom, james)")?;
//!
//! let solutions = env.query("parent(tom, X)")?;
//! // Returns JSON array of solutions
//! ```

pub mod backend;
pub mod error;

// Re-export main types for convenience
pub use backend::ffi::PrologEnvironment;
pub use backend::ffi::{register_clara_evaluate, rust_clara_evaluate, rust_free_string};
pub use error::{PrologError, PrologResult};

/// Initialize the global Prolog system
///
/// This should be called once at application startup.
/// It initializes the SWI-Prolog runtime and registers callbacks.
pub fn init_global() {
    backend::ffi::environment::ensure_prolog_initialized()
        .expect("Failed to initialize Prolog");
    register_clara_evaluate();
    log::info!("Clara-Prolog (LilDevils) initialized");
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_crate_compiles() {
        // Basic smoke test that the crate compiles
        // More detailed tests in submodules
    }
}
