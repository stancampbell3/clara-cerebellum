// FFI module for CLIPS integration

pub mod bindings;
pub mod callbacks;
pub mod conversion;
pub mod environment;

// Re-export commonly used types
pub use environment::ClipsEnvironment;
pub use bindings::{Environment, CLIPSValue, EvalError};
pub use conversion::{clips_value_to_string, string_to_c_string, c_string_to_string};
pub use callbacks::{rust_clara_evaluate, rust_free_string};
