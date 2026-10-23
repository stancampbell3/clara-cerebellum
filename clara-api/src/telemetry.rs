//! Logging setup: `env_logger` to stderr as always, plus an optional OTLP/HTTP
//! export of the same records (e.g. to Loki's native `/otlp` endpoint).
//!
//! Export is off unless `OTEL_EXPORTER_OTLP_ENDPOINT` or
//! `OTEL_EXPORTER_OTLP_LOGS_ENDPOINT` is set, so a stack without a collector
//! behaves exactly as before. The SDK's own `OTEL_*` variables (headers,
//! timeout, resource attributes) are honoured by the exporter/resource builders.

use log::{Log, Metadata, Record};
use opentelemetry_appender_log::OpenTelemetryLogBridge;
use opentelemetry_otlp::{LogExporter, Protocol, WithExportConfig};
use opentelemetry_sdk::logs::{SdkLogger, SdkLoggerProvider};
use opentelemetry_sdk::Resource;

const DEFAULT_SERVICE_NAME: &str = "dis";

// The exporter's own HTTP stack logs through `log`; exporting those records
// would feed the exporter its own output.
const EXPORT_EXCLUDED_TARGETS: &[&str] = &[
    "opentelemetry", "hyper", "hyper_util", "reqwest", "h2", "tower", "rustls",
];

struct TeeLogger {
    console: env_logger::Logger,
    otel: OpenTelemetryLogBridge<SdkLoggerProvider, SdkLogger>,
}

impl Log for TeeLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.console.enabled(metadata)
    }

    fn log(&self, record: &Record) {
        if !self.console.matches(record) {
            return;
        }
        self.console.log(record);
        let target = record.target();
        if !EXPORT_EXCLUDED_TARGETS
            .iter()
            .any(|t| target == *t || target.starts_with(&format!("{t}::")) || target.starts_with(&format!("{t}_")))
        {
            self.otel.log(record);
        }
    }

    fn flush(&self) {
        self.console.flush();
        self.otel.flush();
    }
}

fn otlp_configured() -> bool {
    ["OTEL_EXPORTER_OTLP_ENDPOINT", "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT"]
        .iter()
        .any(|k| std::env::var(k).map(|v| !v.is_empty()).unwrap_or(false))
}

fn build_provider() -> Result<SdkLoggerProvider, Box<dyn std::error::Error>> {
    let exporter = LogExporter::builder()
        .with_http()
        .with_protocol(Protocol::HttpBinary)
        .build()?;
    let service_name = std::env::var("OTEL_SERVICE_NAME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_SERVICE_NAME.to_string());
    Ok(SdkLoggerProvider::builder()
        .with_resource(Resource::builder().with_service_name(service_name).build())
        .with_batch_exporter(exporter)
        .build())
}

/// Install the global logger. Returns the provider when OTLP export is active;
/// call [`shutdown`] with it before exit so the final batch is flushed.
pub fn init_logging() -> Option<SdkLoggerProvider> {
    let console = env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .build();

    if !otlp_configured() {
        let max = console.filter();
        log::set_boxed_logger(Box::new(console)).expect("logger already set");
        log::set_max_level(max);
        return None;
    }

    match build_provider() {
        Ok(provider) => {
            let max = console.filter();
            let tee = TeeLogger {
                otel: OpenTelemetryLogBridge::new(&provider),
                console,
            };
            log::set_boxed_logger(Box::new(tee)).expect("logger already set");
            log::set_max_level(max);
            log::info!("OTLP log export enabled");
            Some(provider)
        }
        Err(e) => {
            let max = console.filter();
            log::set_boxed_logger(Box::new(console)).expect("logger already set");
            log::set_max_level(max);
            log::error!("OTLP log export requested but exporter failed to build: {e}");
            None
        }
    }
}

pub fn shutdown(provider: Option<SdkLoggerProvider>) {
    if let Some(p) = provider {
        if let Err(e) = p.shutdown() {
            eprintln!("OTLP log exporter shutdown failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Live check against a real OTLP endpoint; run with
    /// `OTEL_EXPORTER_OTLP_ENDPOINT=http://host:3100/otlp OTEL_SERVICE_NAME=dis-test \
    ///  cargo test -p clara-api otlp_live -- --ignored`
    #[test]
    #[ignore]
    fn otlp_live_roundtrip() {
        let provider = init_logging();
        assert!(provider.is_some(), "OTEL_EXPORTER_OTLP_ENDPOINT must be set");
        log::info!("dis otlp probe info");
        log::warn!(target: "clara_api::probe", "dis otlp probe warn");
        log::error!(target: "hyper::client", "must NOT be exported");
        shutdown(provider);
    }
}
