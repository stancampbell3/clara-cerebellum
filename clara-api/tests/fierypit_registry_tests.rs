//! Integration tests for FieryPit registration/discovery
//! (`PUT|GET /fierypits`, `DELETE /fierypits/{id}`).
//!
//! Mirrors bootstrap_auth_tests.rs's shape: real actix `test::init_service`
//! against a real `AppState` (`InMemoryBroker`), `ENV_LOCK`-serialised
//! since these tests set `DIS_DOMAIN_PEER_TOKEN`.

use actix_web::{test, web, App};
use clara_api::fierypit_registry::FieryPitRegistry;
use clara_api::handlers::fierypit_registry_handler::{delete_fiery_pit, list_fiery_pits, upsert_fiery_pit};
use clara_api::handlers::session_handler::AppState;
use clara_api::subprocess::SubprocessPool;
use clara_ritual::{InMemoryBroker, RitualRegistry};
use clara_session::{ManagerConfig, SessionManager};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn make_test_state() -> web::Data<AppState> {
    use std::collections::{HashMap, HashSet};
    use std::sync::RwLock;

    web::Data::new(AppState {
        session_manager: SessionManager::new(ManagerConfig::default()),
        subprocess_pool: SubprocessPool::new("./clips".to_string(), "__END__".to_string()),
        deductions: Arc::new(RwLock::new(HashMap::new())),
        coire_store: None,
        active_coire_sessions: Arc::new(RwLock::new(HashSet::new())),
        snapshot_ttl_ms: 604_800_000,
        ritual_registry: Arc::new(RitualRegistry::new("dis.test", Arc::new(InMemoryBroker::new()))),
        dis_domain: "dis.test".to_string(),
        kafka_bootstrap: None,
        fiery_pit_token_cache: Arc::new(Mutex::new(None)),
        fiery_pit_registry: Arc::new(FieryPitRegistry::new(Duration::from_secs(90))),
    })
}

fn app_service(state: web::Data<AppState>) -> App<
    impl actix_web::dev::ServiceFactory<
        actix_web::dev::ServiceRequest,
        Config = (),
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
        InitError = (),
    >,
> {
    App::new()
        .app_data(state)
        .route("/fierypits", web::get().to(list_fiery_pits))
        .route("/fierypits/{id}", web::put().to(upsert_fiery_pit))
        .route("/fierypits/{id}", web::delete().to(delete_fiery_pit))
}

#[actix_web::test]
async fn missing_token_is_401_on_all_three_routes() {
    let _g = ENV_LOCK.lock().unwrap();
    std::env::set_var("DIS_DOMAIN_PEER_TOKEN", "shared-secret");

    let app = test::init_service(app_service(make_test_state())).await;
    let id = uuid::Uuid::new_v4();

    let put_resp = test::call_service(
        &app,
        test::TestRequest::put()
            .uri(&format!("/fierypits/{id}"))
            .set_json(json!({"base_url": "http://a:6666", "dis_domain": "dis.test", "evaluators": []}))
            .to_request(),
    )
    .await;
    assert_eq!(put_resp.status().as_u16(), 401);

    let get_resp = test::call_service(&app, test::TestRequest::get().uri("/fierypits").to_request()).await;
    assert_eq!(get_resp.status().as_u16(), 401);

    let del_resp = test::call_service(
        &app,
        test::TestRequest::delete().uri(&format!("/fierypits/{id}")).to_request(),
    )
    .await;
    assert_eq!(del_resp.status().as_u16(), 401);

    std::env::remove_var("DIS_DOMAIN_PEER_TOKEN");
}

#[actix_web::test]
async fn put_then_get_roundtrip_with_valid_token() {
    let _g = ENV_LOCK.lock().unwrap();
    std::env::set_var("DIS_DOMAIN_PEER_TOKEN", "shared-secret");

    let app = test::init_service(app_service(make_test_state())).await;
    let id = uuid::Uuid::new_v4();

    let put_resp = test::call_service(
        &app,
        test::TestRequest::put()
            .uri(&format!("/fierypits/{id}"))
            .insert_header(("Authorization", "Bearer shared-secret"))
            .set_json(json!({
                "base_url": "http://pineal:6666",
                "dis_domain": "dis.test",
                "evaluators": ["clara_mind_splinter"]
            }))
            .to_request(),
    )
    .await;
    assert_eq!(put_resp.status().as_u16(), 200);

    let get_resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/fierypits")
            .insert_header(("Authorization", "Bearer shared-secret"))
            .to_request(),
    )
    .await;
    assert_eq!(get_resp.status().as_u16(), 200);
    let body: serde_json::Value = test::read_body_json(get_resp).await;
    let fierypits = body["fierypits"].as_array().unwrap();
    assert_eq!(fierypits.len(), 1);
    assert_eq!(fierypits[0]["base_url"], "http://pineal:6666");
    assert_eq!(fierypits[0]["evaluators"][0], "clara_mind_splinter");

    std::env::remove_var("DIS_DOMAIN_PEER_TOKEN");
}

#[actix_web::test]
async fn put_twice_updates_not_duplicates() {
    let _g = ENV_LOCK.lock().unwrap();
    std::env::set_var("DIS_DOMAIN_PEER_TOKEN", "shared-secret");

    let app = test::init_service(app_service(make_test_state())).await;
    let id = uuid::Uuid::new_v4();
    let auth = ("Authorization", "Bearer shared-secret");

    for evaluators in [json!([]), json!(["ollama"])] {
        let resp = test::call_service(
            &app,
            test::TestRequest::put()
                .uri(&format!("/fierypits/{id}"))
                .insert_header(auth)
                .set_json(json!({"base_url": "http://a:6666", "dis_domain": "dis.test", "evaluators": evaluators}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);
    }

    let get_resp = test::call_service(
        &app,
        test::TestRequest::get().uri("/fierypits").insert_header(auth).to_request(),
    )
    .await;
    let body: serde_json::Value = test::read_body_json(get_resp).await;
    let fierypits = body["fierypits"].as_array().unwrap();
    assert_eq!(fierypits.len(), 1);
    assert_eq!(fierypits[0]["evaluators"][0], "ollama");

    std::env::remove_var("DIS_DOMAIN_PEER_TOKEN");
}

#[actix_web::test]
async fn delete_then_get_no_longer_lists_it() {
    let _g = ENV_LOCK.lock().unwrap();
    std::env::set_var("DIS_DOMAIN_PEER_TOKEN", "shared-secret");

    let app = test::init_service(app_service(make_test_state())).await;
    let id = uuid::Uuid::new_v4();
    let auth = ("Authorization", "Bearer shared-secret");

    test::call_service(
        &app,
        test::TestRequest::put()
            .uri(&format!("/fierypits/{id}"))
            .insert_header(auth)
            .set_json(json!({"base_url": "http://a:6666", "dis_domain": "dis.test", "evaluators": []}))
            .to_request(),
    )
    .await;

    let del_resp = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri(&format!("/fierypits/{id}"))
            .insert_header(auth)
            .to_request(),
    )
    .await;
    assert_eq!(del_resp.status().as_u16(), 200);

    let get_resp = test::call_service(
        &app,
        test::TestRequest::get().uri("/fierypits").insert_header(auth).to_request(),
    )
    .await;
    let body: serde_json::Value = test::read_body_json(get_resp).await;
    assert_eq!(body["fierypits"].as_array().unwrap().len(), 0);

    std::env::remove_var("DIS_DOMAIN_PEER_TOKEN");
}

#[actix_web::test]
async fn evaluator_and_dis_domain_filters() {
    let _g = ENV_LOCK.lock().unwrap();
    std::env::set_var("DIS_DOMAIN_PEER_TOKEN", "shared-secret");

    let app = test::init_service(app_service(make_test_state())).await;
    let auth = ("Authorization", "Bearer shared-secret");

    for (url, domain, evaluators) in [
        ("http://a:6666", "dis.test", json!(["ollama"])),
        ("http://b:6666", "dis.other", json!(["clara_mind_splinter"])),
    ] {
        test::call_service(
            &app,
            test::TestRequest::put()
                .uri(&format!("/fierypits/{}", uuid::Uuid::new_v4()))
                .insert_header(auth)
                .set_json(json!({"base_url": url, "dis_domain": domain, "evaluators": evaluators}))
                .to_request(),
        )
        .await;
    }

    let get_resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/fierypits?evaluator=ollama")
            .insert_header(auth)
            .to_request(),
    )
    .await;
    let body: serde_json::Value = test::read_body_json(get_resp).await;
    assert_eq!(body["fierypits"].as_array().unwrap().len(), 1);

    let get_resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/fierypits?dis_domain=dis.other")
            .insert_header(auth)
            .to_request(),
    )
    .await;
    let body: serde_json::Value = test::read_body_json(get_resp).await;
    assert_eq!(body["fierypits"].as_array().unwrap().len(), 1);
    assert_eq!(body["fierypits"][0]["base_url"], "http://b:6666");

    std::env::remove_var("DIS_DOMAIN_PEER_TOKEN");
}
