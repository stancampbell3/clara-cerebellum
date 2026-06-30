//! Integration tests for the FieryPit auto-bootstrap auth flow.
//!
//! Each test spins up a mock lildaemon via mockito so the `create_ritual`
//! handler can make real HTTP calls without a running lildaemon instance.
//!
//! Tests that set env vars acquire `ENV_LOCK` to avoid races with concurrent
//! test threads.

use actix_web::{test, web, App};
use clara_api::handlers::ritual_handler;
use clara_api::handlers::session_handler::AppState;
use clara_ritual::{InMemoryBroker, RitualRegistry};
use serde_json::json;
use std::sync::{Arc, Mutex};

// Serialise env-var-touching tests within this binary.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn make_test_state() -> web::Data<AppState> {
    use clara_session::{SessionManager, ManagerConfig};
    use clara_api::subprocess::SubprocessPool;
    use std::collections::{HashMap, HashSet};
    use std::sync::RwLock;

    web::Data::new(AppState {
        session_manager: SessionManager::new(ManagerConfig::default()),
        subprocess_pool: SubprocessPool::new("./clips".to_string(), "__END__".to_string()),
        deductions: Arc::new(RwLock::new(HashMap::new())),
        coire_store: None,
        active_coire_sessions: Arc::new(RwLock::new(HashSet::new())),
        snapshot_ttl_ms: 604_800_000,
        ritual_registry: Arc::new(RitualRegistry::new(
            "dis.test",
            Arc::new(InMemoryBroker::new()),
        )),
        dis_domain: "dis.test".to_string(),
        kafka_bootstrap: None,
        fiery_pit_token_cache: Arc::new(Mutex::new(None)),
    })
}

// ---------------------------------------------------------------------------
// Happy path: Dis acquires a service token and bootstrap-joins a participant
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn test_create_ritual_bootstraps_participant_with_service_token() {
    let _g = ENV_LOCK.lock().unwrap();

    let mut mock_lild = mockito::Server::new_async().await;

    // Mock: POST /auth/service-token → 200 with a JWT
    let token_mock = mock_lild
        .mock("POST", "/auth/service-token")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"access_token":"bootstrap-svc-jwt","expires_in":2592000,"token_type":"bearer"}"#)
        .create_async()
        .await;

    // Mock: POST /ritual/join → 200 (lildaemon accepts the join)
    let join_mock = mock_lild
        .mock("POST", "/ritual/join")
        .match_header("authorization", "Bearer bootstrap-svc-jwt")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"ritual_id":"00000000-0000-0000-0000-000000000001","status":"joined","evaluator":null}"#)
        .create_async()
        .await;

    std::env::set_var("LILDAEMON_SERVICE_SECRET", "shared-test-secret");
    std::env::remove_var("FIERYPIT_SERVICE_KEY");

    let app = test::init_service(
        App::new()
            .app_data(make_test_state())
            .route("/ritual", web::post().to(ritual_handler::create_ritual)),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/ritual")
        .set_json(json!({
            "name": "bootstrap-test",
            "participants": [mock_lild.url()]
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    std::env::remove_var("LILDAEMON_SERVICE_SECRET");

    assert_eq!(resp.status().as_u16(), 201, "Ritual should be created");
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body.get("ritual_id").is_some(), "Response must contain ritual_id");

    token_mock.assert_async().await;
    join_mock.assert_async().await;
}

// ---------------------------------------------------------------------------
// Retry path: Dis clears cache and re-acquires on 401
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn test_create_ritual_retries_service_token_after_401() {
    let _g = ENV_LOCK.lock().unwrap();

    let mut mock_lild = mockito::Server::new_async().await;

    // /auth/service-token is called twice: once before the first join attempt
    // and once after the 401 clears the cache.
    let token_mock = mock_lild
        .mock("POST", "/auth/service-token")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"access_token":"retry-svc-jwt","expires_in":2592000,"token_type":"bearer"}"#)
        .expect(2)
        .create_async()
        .await;

    // Both join attempts return 401.  The handler logs a warning for the
    // participant but still returns 201 — bootstrap failure is non-fatal.
    let join_mock = mock_lild
        .mock("POST", "/ritual/join")
        .with_status(401)
        .with_body(r#"{"detail":"token not found"}"#)
        .expect(2)
        .create_async()
        .await;

    std::env::set_var("LILDAEMON_SERVICE_SECRET", "retry-test-secret");
    std::env::remove_var("FIERYPIT_SERVICE_KEY");

    let app = test::init_service(
        App::new()
            .app_data(make_test_state())
            .route("/ritual", web::post().to(ritual_handler::create_ritual)),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/ritual")
        .set_json(json!({
            "name": "retry-test",
            "participants": [mock_lild.url()]
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    std::env::remove_var("LILDAEMON_SERVICE_SECRET");

    // Ritual is created even though both join attempts failed.
    assert_eq!(resp.status().as_u16(), 201);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body.get("ritual_id").is_some());

    // Both mocks must have been hit the expected number of times.
    token_mock.assert_async().await;
    join_mock.assert_async().await;
}

// ---------------------------------------------------------------------------
// No participants — bootstrap is skipped entirely (no HTTP calls)
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn test_create_ritual_without_participants_skips_bootstrap() {
    let app = test::init_service(
        App::new()
            .app_data(make_test_state())
            .route("/ritual", web::post().to(ritual_handler::create_ritual)),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/ritual")
        .set_json(json!({ "name": "no-participants", "participants": [] }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status().as_u16(), 201);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body.get("ritual_id").is_some());
}

// ---------------------------------------------------------------------------
// Static FIERYPIT_SERVICE_KEY override bypasses token acquisition
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn test_create_ritual_uses_static_service_key_when_set() {
    let _g = ENV_LOCK.lock().unwrap();

    let mut mock_lild = mockito::Server::new_async().await;

    // /auth/service-token must NOT be called — static key takes precedence.
    let no_token_call = mock_lild
        .mock("POST", "/auth/service-token")
        .expect(0)
        .create_async()
        .await;

    let join_mock = mock_lild
        .mock("POST", "/ritual/join")
        .match_header("authorization", "Bearer static-key-value")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"ritual_id":"00000000-0000-0000-0000-000000000002","status":"joined","evaluator":null}"#)
        .create_async()
        .await;

    std::env::set_var("FIERYPIT_SERVICE_KEY", "static-key-value");
    std::env::remove_var("LILDAEMON_SERVICE_SECRET");

    let app = test::init_service(
        App::new()
            .app_data(make_test_state())
            .route("/ritual", web::post().to(ritual_handler::create_ritual)),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/ritual")
        .set_json(json!({
            "name": "static-key-test",
            "participants": [mock_lild.url()]
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    std::env::remove_var("FIERYPIT_SERVICE_KEY");

    assert_eq!(resp.status().as_u16(), 201);
    no_token_call.assert_async().await;
    join_mock.assert_async().await;
}
