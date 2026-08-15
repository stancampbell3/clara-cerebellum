use std::env;
use std::sync::Arc;

use crate::broker::{InMemoryBroker, KafkaBridge};
use crate::error::RitualError;

/// Build a `KafkaBridge` from the `KAFKA_BOOTSTRAP` environment variable.
///
/// If `KAFKA_BOOTSTRAP` is set (a comma-separated `host:port` list) and the
/// `rskafka` feature is enabled, connects a real `RsKafkaClient`. Otherwise
/// falls back to an in-process `InMemoryBroker` — fine for a REPL talking to
/// itself, but invisible to any other process. Shared by `clara-api`'s
/// server startup and both the `prolog-repl`/`clips-repl` binaries so the
/// bootstrap logic doesn't drift between call sites.
pub fn bridge_from_env() -> Result<Arc<dyn KafkaBridge>, RitualError> {
    match env::var("KAFKA_BOOTSTRAP") {
        Ok(val) if !val.is_empty() => connect_kafka(&val),
        _ => {
            log::info!("clara_ritual::bridge_from_env: using InMemoryBroker (KAFKA_BOOTSTRAP not set)");
            Ok(Arc::new(InMemoryBroker::new()))
        }
    }
}

#[cfg(feature = "rskafka")]
fn connect_kafka(bootstrap: &str) -> Result<Arc<dyn KafkaBridge>, RitualError> {
    let brokers: Vec<String> = bootstrap.split(',').map(|s| s.trim().to_string()).collect();
    let client = crate::broker::RsKafkaClient::new(brokers)?;
    log::info!("clara_ritual::bridge_from_env: using RsKafkaClient (bootstrap={})", bootstrap);
    Ok(Arc::new(client))
}

#[cfg(not(feature = "rskafka"))]
fn connect_kafka(bootstrap: &str) -> Result<Arc<dyn KafkaBridge>, RitualError> {
    log::warn!(
        "clara_ritual::bridge_from_env: KAFKA_BOOTSTRAP={} set but the 'rskafka' feature is not \
         enabled — falling back to InMemoryBroker (isolated to this process)",
        bootstrap
    );
    Ok(Arc::new(InMemoryBroker::new()))
}

/// Read the ambient Dis domain from the `DIS_DOMAIN` environment variable,
/// defaulting to `"dis.local"` — the same default `clara-api` uses for
/// `config.server.dis_domain_id`. Intended for the REPL binaries, which
/// don't load the full `AppConfig`; `clara-api` itself should pass its
/// already-resolved `dis_domain_id` to `init_global` instead of calling this.
pub fn dis_domain_from_env() -> String {
    env::var("DIS_DOMAIN").unwrap_or_else(|_| "dis.local".to_string())
}
