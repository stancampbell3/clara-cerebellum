//! Integration tests for the /devils/* REST API endpoints
//!
//! These tests verify the Prolog (LilDevils) API endpoints work correctly,
//! including session management and query execution.

use actix_web::{test, web, App};
use clara_api::handlers::devils_handler;
use clara_api::handlers::session_handler::AppState;
use clara_api::subprocess::SubprocessPool;
use clara_session::{SessionManager, ManagerConfig};
use serde_json::json;

/// Create test app state
fn create_test_state() -> web::Data<AppState> {
    web::Data::new(AppState {
        session_manager: SessionManager::new(ManagerConfig::default()),
        subprocess_pool: SubprocessPool::new(
            "./clips".to_string(),
            "__END__".to_string(),
        ),
    })
}

/// Test creating a Prolog session via POST /devils/sessions
#[actix_web::test]
async fn test_create_prolog_session() {
    let state = create_test_state();

    let app = test::init_service(
        App::new()
            .app_data(state.clone())
            .route("/devils/sessions", web::post().to(devils_handler::create_prolog_session))
    ).await;

    let req = test::TestRequest::post()
        .uri("/devils/sessions")
        .set_json(&json!({
            "user_id": "test-user"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success(), "Create session should succeed");

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body.get("session_id").is_some(), "Response should contain session_id");
    assert_eq!(body.get("user_id").and_then(|v| v.as_str()), Some("test-user"));
    assert_eq!(body.get("status").and_then(|v| v.as_str()), Some("active"));
}

/// Test listing Prolog sessions via GET /devils/sessions
#[actix_web::test]
async fn test_list_prolog_sessions() {
    let state = create_test_state();

    // Create a session first
    state.session_manager
        .create_prolog_session("test-user".to_string(), None)
        .expect("Failed to create session");

    let app = test::init_service(
        App::new()
            .app_data(state.clone())
            .route("/devils/sessions", web::get().to(devils_handler::list_prolog_sessions))
    ).await;

    let req = test::TestRequest::get()
        .uri("/devils/sessions")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success(), "List sessions should succeed");

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body.get("sessions").is_some(), "Response should contain sessions array");
    assert!(body.get("total").is_some(), "Response should contain total count");

    let total = body.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
    assert!(total >= 1, "Should have at least one session");
}

/// Test getting a specific Prolog session via GET /devils/sessions/{id}
#[actix_web::test]
async fn test_get_prolog_session() {
    let state = create_test_state();

    // Create a session
    let session = state.session_manager
        .create_prolog_session("test-user".to_string(), None)
        .expect("Failed to create session");

    let app = test::init_service(
        App::new()
            .app_data(state.clone())
            .route("/devils/sessions/{session_id}", web::get().to(devils_handler::get_prolog_session))
    ).await;

    let req = test::TestRequest::get()
        .uri(&format!("/devils/sessions/{}", session.session_id))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success(), "Get session should succeed");

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(
        body.get("session_id").and_then(|v| v.as_str()),
        Some(session.session_id.to_string().as_str())
    );
}

/// Test terminating a Prolog session via DELETE /devils/sessions/{id}
#[actix_web::test]
async fn test_terminate_prolog_session() {
    let state = create_test_state();

    // Create a session
    let session = state.session_manager
        .create_prolog_session("test-user".to_string(), None)
        .expect("Failed to create session");

    let app = test::init_service(
        App::new()
            .app_data(state.clone())
            .route("/devils/sessions/{session_id}", web::delete().to(devils_handler::terminate_prolog_session))
    ).await;

    let req = test::TestRequest::delete()
        .uri(&format!("/devils/sessions/{}", session.session_id))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success(), "Terminate session should succeed");

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body.get("status").and_then(|v| v.as_str()), Some("terminated"));
}

/// Test executing a Prolog query via POST /devils/sessions/{id}/query
#[actix_web::test]
async fn test_query_prolog() {
    let state = create_test_state();

    // Create a session
    let session = state.session_manager
        .create_prolog_session("test-user".to_string(), None)
        .expect("Failed to create session");

    let app = test::init_service(
        App::new()
            .app_data(state.clone())
            .route("/devils/sessions/{session_id}/query", web::post().to(devils_handler::query_prolog))
    ).await;

    let req = test::TestRequest::post()
        .uri(&format!("/devils/sessions/{}/query", session.session_id))
        .set_json(&json!({
            "goal": "X is 2 + 3"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success(), "Query should succeed");

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body.get("success").and_then(|v| v.as_bool()), Some(true));
    assert!(body.get("result").is_some(), "Should have result");
    assert!(body.get("runtime_ms").is_some(), "Should have runtime_ms");
}

/// Test loading clauses via POST /devils/sessions/{id}/consult
#[actix_web::test]
async fn test_consult_prolog() {
    let state = create_test_state();

    // Create a session
    let session = state.session_manager
        .create_prolog_session("test-user".to_string(), None)
        .expect("Failed to create session");

    let app = test::init_service(
        App::new()
            .app_data(state.clone())
            .route("/devils/sessions/{session_id}/consult", web::post().to(devils_handler::consult_prolog))
    ).await;

    let req = test::TestRequest::post()
        .uri(&format!("/devils/sessions/{}/consult", session.session_id))
        .set_json(&json!({
            "clauses": [
                "parent(tom, mary)",
                "parent(tom, john)",
                "parent(mary, ann)"
            ]
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success(), "Consult should succeed");

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body.get("status").and_then(|v| v.as_str()), Some("clauses_loaded"));
    assert_eq!(body.get("count").and_then(|v| v.as_u64()), Some(3));
}

/// Test full workflow: create session, consult, query, terminate
#[actix_web::test]
async fn test_full_prolog_workflow() {
    let state = create_test_state();

    let app = test::init_service(
        App::new()
            .app_data(state.clone())
            .route("/devils/sessions", web::post().to(devils_handler::create_prolog_session))
            .route("/devils/sessions/{session_id}/consult", web::post().to(devils_handler::consult_prolog))
            .route("/devils/sessions/{session_id}/query", web::post().to(devils_handler::query_prolog))
            .route("/devils/sessions/{session_id}", web::delete().to(devils_handler::terminate_prolog_session))
    ).await;

    // Step 1: Create session
    let req = test::TestRequest::post()
        .uri("/devils/sessions")
        .set_json(&json!({"user_id": "workflow-user"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: serde_json::Value = test::read_body_json(resp).await;
    let session_id = body.get("session_id").and_then(|v| v.as_str()).unwrap();

    // Step 2: Load knowledge base
    let req = test::TestRequest::post()
        .uri(&format!("/devils/sessions/{}/consult", session_id))
        .set_json(&json!({
            "clauses": [
                "likes(mary, food)",
                "likes(mary, wine)",
                "likes(john, wine)",
                "likes(john, mary)"
            ]
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    // Step 3: Query the knowledge base
    let req = test::TestRequest::post()
        .uri(&format!("/devils/sessions/{}/query", session_id))
        .set_json(&json!({
            "goal": "likes(mary, X)"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body.get("success").and_then(|v| v.as_bool()), Some(true));

    // Step 4: Terminate session
    let req = test::TestRequest::delete()
        .uri(&format!("/devils/sessions/{}", session_id))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
}

/// Test error handling: query non-existent session
#[actix_web::test]
async fn test_query_nonexistent_session() {
    let state = create_test_state();

    let app = test::init_service(
        App::new()
            .app_data(state.clone())
            .route("/devils/sessions/{session_id}/query", web::post().to(devils_handler::query_prolog))
    ).await;

    let req = test::TestRequest::post()
        .uri("/devils/sessions/nonexistent-session-id/query")
        .set_json(&json!({"goal": "true"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(!resp.status().is_success(), "Query to non-existent session should fail");
}

/// Test error handling: get non-existent session
#[actix_web::test]
async fn test_get_nonexistent_session() {
    let state = create_test_state();

    let app = test::init_service(
        App::new()
            .app_data(state.clone())
            .route("/devils/sessions/{session_id}", web::get().to(devils_handler::get_prolog_session))
    ).await;

    let req = test::TestRequest::get()
        .uri("/devils/sessions/nonexistent-session-id")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(!resp.status().is_success(), "Get non-existent session should fail");
}
