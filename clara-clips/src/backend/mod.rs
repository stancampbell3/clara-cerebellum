// Backend implementations for CLIPS integration

pub mod ffi;

// Re-export FFI types for convenience
pub use ffi::{is_construct, split_clips_constructs, ClipsEnvironment};
