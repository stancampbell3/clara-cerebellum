// Clara-CLIPS: CLIPS integration library

pub mod backend;

// Re-export commonly used types
pub use backend::ffi;
pub use backend::ClipsEnvironment;

// Force-link coire FFI symbols so the C linker can find them.
// Without these re-exports, the linker strips the #[no_mangle] symbols
// from clara-coire because nothing in Rust code references them.
#[cfg(feature = "ffi")]
pub use clara_coire::clips_bridge::{
    rust_coire_emit, rust_coire_poll, rust_coire_mark, rust_coire_count, rust_coire_free_string,
};
