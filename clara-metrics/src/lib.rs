//! Metrics and exporters for the Clara stack.
//!
//! Every metric is emitted once, at an existing chokepoint in the calling
//! crate (`clara-ritual`, `clara-toolbox`, `clara-api`), through the typed
//! functions in [`counters`], [`gauges`] and [`histograms`] rather than raw
//! `metrics_core::counter!`/`gauge!`/`histogram!` calls scattered through
//! business logic. [`exporter`] installs the process-wide Prometheus
//! recorder and renders it for `clara-api`'s `/metrics` route.
//!
//! The `metrics` facade crate is renamed `metrics_core` in `Cargo.toml` to
//! avoid colliding with this crate's own `metrics` module.

#[cfg(feature = "with-prometheus")]
pub mod exporter;
pub mod metrics;

pub use self::metrics::{counters, gauges, histograms};
#[cfg(feature = "with-prometheus")]
pub use exporter::{init, PrometheusHandle};
