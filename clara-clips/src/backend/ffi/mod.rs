// FFI module for CLIPS integration

pub mod bindings;
pub mod callbacks;
pub mod conversion;
pub mod environment;

// Re-export commonly used types
pub use environment::ClipsEnvironment;
pub use bindings::{Environment, CLIPSValue, EvalError};
pub use conversion::{clips_value_to_string, string_to_c_string, c_string_to_string};

// Re-export FFI functions from clara-toolbox
pub use callbacks::{evaluate_json_string, free_c_string};
// Note: rust_clara_evaluate and rust_free_string are provided by clara-toolbox
// with #[no_mangle] when the "ffi" feature is enabled
