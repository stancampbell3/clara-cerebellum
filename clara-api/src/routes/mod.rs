pub mod sessions;
pub mod health;
pub mod metrics;
pub mod eval;

use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    health::configure(cfg);
    sessions::configure(cfg);
    metrics::configure(cfg);
}
