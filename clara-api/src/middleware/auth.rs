//! Auth guard for FieryPit-only inbound endpoints (registration/heartbeat/
//! deregister/discovery — see `handlers::fierypit_registry_handler`).
//!
//! Mirrors lildaemon's `goat.app.users.auth.get_current_user_or_peer`
//! semantics, but simpler: this surface is FieryPit-only, never a user
//! JWT. clara-api has no user-JWT auth of any kind today —
//! `AuthConfig.jwt_secret` is loaded and validated at config-load time but
//! has zero consumers anywhere in this crate (confirmed by grep before
//! writing this); the Actix app wraps only `Logger`. This is the first
//! real inbound auth check in clara-api.
//!
//! Returns `Ok(())` when authorized, or a ready-to-return `HttpResponse`
//! (401) otherwise — matches this codebase's handlers-return-HttpResponse-
//! directly convention rather than introducing a real `FromRequest`
//! extractor or `actix_web::Error`, neither of which anything else in
//! clara-api uses.

use actix_web::{HttpRequest, HttpResponse};
use serde_json::json;

fn bearer_token(req: &HttpRequest) -> Option<String> {
    req.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Checks the request's `Authorization: Bearer <token>` header against the
/// `DIS_DOMAIN_PEER_TOKEN` shared secret — the same one
/// `goat.app.fiery_pit_peer_client` already uses for FieryPit-to-FieryPit
/// `/ritual/join` calls. Reused deliberately rather than minting a second
/// secret: both are the same trust boundary ("every FieryPit under this
/// Dis domain").
pub fn require_peer_token(req: &HttpRequest) -> Result<(), HttpResponse> {
    let configured = std::env::var("DIS_DOMAIN_PEER_TOKEN")
        .ok()
        .filter(|s| !s.is_empty());
    let Some(expected) = configured else {
        log::warn!(
            "require_peer_token: DIS_DOMAIN_PEER_TOKEN not configured on Dis — rejecting FieryPit registry call"
        );
        return Err(HttpResponse::Unauthorized()
            .json(json!({ "error": "DIS_DOMAIN_PEER_TOKEN not configured on Dis" })));
    };
    match bearer_token(req) {
        Some(tok) if tok == expected => Ok(()),
        _ => Err(HttpResponse::Unauthorized().json(json!({ "error": "invalid or missing peer token" }))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;
    use std::sync::Mutex;

    // Serialise env-var-touching tests within this module — same idiom as
    // clara-api/tests/bootstrap_auth_tests.rs's ENV_LOCK.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn missing_header_is_unauthorized() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("DIS_DOMAIN_PEER_TOKEN", "secret123");
        let req = TestRequest::default().to_http_request();
        assert!(require_peer_token(&req).is_err());
        std::env::remove_var("DIS_DOMAIN_PEER_TOKEN");
    }

    #[test]
    fn wrong_token_is_unauthorized() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("DIS_DOMAIN_PEER_TOKEN", "secret123");
        let req = TestRequest::default()
            .insert_header(("Authorization", "Bearer wrong"))
            .to_http_request();
        assert!(require_peer_token(&req).is_err());
        std::env::remove_var("DIS_DOMAIN_PEER_TOKEN");
    }

    #[test]
    fn correct_token_is_authorized() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("DIS_DOMAIN_PEER_TOKEN", "secret123");
        let req = TestRequest::default()
            .insert_header(("Authorization", "Bearer secret123"))
            .to_http_request();
        assert!(require_peer_token(&req).is_ok());
        std::env::remove_var("DIS_DOMAIN_PEER_TOKEN");
    }

    #[test]
    fn unconfigured_secret_is_unauthorized_even_with_a_header() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var("DIS_DOMAIN_PEER_TOKEN");
        let req = TestRequest::default()
            .insert_header(("Authorization", "Bearer anything"))
            .to_http_request();
        assert!(require_peer_token(&req).is_err());
    }
}
