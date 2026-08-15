pub mod adhoc;
pub mod bridge;
pub mod broker;
pub mod envelope;
pub mod error;
pub mod handle;
pub mod registry;
pub mod ritual;
pub mod topic;

#[cfg(feature = "ffi")]
pub mod clips_bridge;

pub use adhoc::PolledTopic;
pub use bridge::{bridge_from_env, dis_domain_from_env};
pub use broker::{InMemoryBroker, KafkaBridge};
#[cfg(feature = "rskafka")]
pub use broker::RsKafkaClient;
pub use envelope::{label, MessageKind, RitualConfig, Routing, TephraEnvelope, TephraPayload};
pub use error::RitualError;
pub use handle::RitualHandle;
pub use registry::{RitualRegistry, RitualSummary};
pub use ritual::RitualState;
pub use topic::{coire_topic_name, topic_name};

use std::sync::{Arc, OnceLock};

struct GlobalRitual {
    bridge:     Arc<dyn KafkaBridge>,
    dis_domain: String,
}

static GLOBAL: OnceLock<GlobalRitual> = OnceLock::new();

/// Initialize the global `KafkaBridge` singleton and its ambient Dis domain.
///
/// Should be called once at application startup (or REPL startup) after
/// constructing a bridge via [`bridge_from_env`] or by hand. This is what
/// makes the bridge reachable from the ad hoc `coire_topic_*`/`coire-topic-*`
/// predicates in Prolog and CLIPS, independent of any formally-joined
/// Ritual — see [`global`] and the [`adhoc`] module.
pub fn init_global(bridge: Arc<dyn KafkaBridge>, dis_domain: impl Into<String>) -> Result<(), RitualError> {
    GLOBAL
        .set(GlobalRitual { bridge, dis_domain: dis_domain.into() })
        .map_err(|_| RitualError::AlreadyInitialized)?;
    log::info!("Global clara_ritual KafkaBridge initialized");
    Ok(())
}

/// Get a reference to the global `KafkaBridge`.
/// Panics if `init_global()` has not been called.
pub fn global() -> &'static Arc<dyn KafkaBridge> {
    &GLOBAL
        .get()
        .expect("Global clara_ritual KafkaBridge not initialized — call clara_ritual::init_global() first")
        .bridge
}

/// Get the ambient Dis domain registered at `init_global()` time.
/// Panics if `init_global()` has not been called.
pub fn global_dis_domain() -> &'static str {
    &GLOBAL
        .get()
        .expect("Global clara_ritual KafkaBridge not initialized — call clara_ritual::init_global() first")
        .dis_domain
}
