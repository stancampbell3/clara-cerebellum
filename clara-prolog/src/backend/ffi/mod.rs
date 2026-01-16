//! FFI bindings and wrappers for SWI-Prolog
//!
//! This module provides:
//! - `bindings`: Low-level extern "C" declarations for SWI-Prolog C API
//! - `conversion`: Type conversion utilities between Rust and Prolog
//! - `environment`: Safe `PrologEnvironment` wrapper
//! - `callbacks`: Prolog→Rust callback implementations

pub mod bindings;
pub mod callbacks;
pub mod conversion;
pub mod environment;

pub use bindings::*;
pub use callbacks::{register_clara_evaluate, rust_clara_evaluate, rust_free_string};
pub use conversion::*;
pub use environment::PrologEnvironment;
