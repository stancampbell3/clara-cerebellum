//! Process-wide Prometheus recorder.
//!
//! Installed once via `PrometheusBuilder::install_recorder` (not `.install()`,
//! which spawns its own HTTP server) so `clara-api`'s existing `/metrics`
//! route can render the same handle inline — one process, one port.

use std::sync::OnceLock;

pub use metrics_exporter_prometheus::PrometheusHandle;
use metrics_exporter_prometheus::PrometheusBuilder;

static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Install the global Prometheus recorder on first call; later calls return
/// the same handle (safe to call from every process that wants metrics,
/// including test setup).
pub fn init() -> PrometheusHandle {
    HANDLE
        .get_or_init(|| {
            PrometheusBuilder::new()
                .install_recorder()
                .expect("install Prometheus metrics recorder")
        })
        .clone()
}
