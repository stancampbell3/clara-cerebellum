// Clara-CLIPS: CLIPS integration library

pub mod backend;

// Re-export commonly used types
pub use backend::ffi;
pub use backend::{is_construct, split_clips_constructs, ClipsEnvironment};

// Force-link coire FFI symbols so the C linker can find them.
// Without these re-exports, the linker strips the #[no_mangle] symbols
// from clara-coire because nothing in Rust code references them.
#[cfg(feature = "ffi")]
pub use clara_coire::clips_bridge::{
    rust_coire_emit, rust_coire_poll, rust_coire_mark, rust_coire_count, rust_coire_free_string,
};

// Force-link ad hoc Coire topic (clara-ritual) FFI symbols — same reason.
#[cfg(feature = "ffi")]
pub use clara_ritual::clips_bridge::{
    rust_ritual_topic_create, rust_ritual_topic_list, rust_ritual_topic_delete,
    rust_ritual_topic_publish, rust_ritual_topic_poll, rust_ritual_topic_poll_from,
    rust_ritual_free_string,
};
