pub mod carrion_picker;
pub mod coire;
pub mod error;
pub mod event;
pub mod store;

#[cfg(feature = "ffi")]
pub mod clips_bridge;

pub use carrion_picker::CarrionPicker;
pub use coire::Coire;
pub use error::{CoireError, CoireResult};
pub use event::{ClaraEvent, EventStatus};
pub use store::CoireStore;

use std::sync::OnceLock;

static GLOBAL_COIRE: OnceLock<Coire> = OnceLock::new();

/// Initialize the global Coire singleton.
/// Should be called once at application startup.
pub fn init_global() -> CoireResult<()> {
    let coire = Coire::new()?;
    GLOBAL_COIRE
        .set(coire)
        .map_err(|_| CoireError::AlreadyInitialized)?;
    log::info!("Global Coire initialized");
    Ok(())
}

/// Get a reference to the global Coire.
/// Panics if `init_global()` has not been called.
pub fn global() -> &'static Coire {
    GLOBAL_COIRE
        .get()
        .expect("Global Coire not initialized — call clara_coire::init_global() first")
}
