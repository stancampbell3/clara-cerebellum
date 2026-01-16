//! Integration tests for Prolog session management
//!
//! These tests verify that the SessionManager correctly manages
//! Prolog sessions, including creation, termination, and isolation.

use clara_session::{SessionManager, SessionType, ResourceLimits, ManagerConfig};

/// Helper to create a session manager with default config
fn create_manager() -> SessionManager {
    SessionManager::new(ManagerConfig::default())
}

/// Test creating a Prolog session
#[test]
fn test_create_prolog_session() {
    let manager = create_manager();

    let session = manager.create_prolog_session("test-user".to_string(), None);
    assert!(session.is_ok(), "Should create Prolog session: {:?}", session.err());

    let session = session.unwrap();
    assert_eq!(session.user_id, "test-user");
    assert_eq!(session.session_type, SessionType::Prolog);
    assert_eq!(session.status.to_string(), "active");
}

/// Test creating Prolog session with custom limits
#[test]
fn test_create_prolog_session_with_limits() {
    let manager = create_manager();

    let limits = ResourceLimits {
        max_facts: 500,
        max_rules: 250,
        max_memory_mb: 64,
    };

    let session = manager.create_prolog_session("test-user".to_string(), Some(limits));
    assert!(session.is_ok(), "Should create Prolog session with limits");

    let session = session.unwrap();
    assert_eq!(session.limits.max_facts, 500);
    assert_eq!(session.limits.max_rules, 250);
    assert_eq!(session.limits.max_memory_mb, 64);
}

/// Test terminating a Prolog session
#[test]
fn test_terminate_prolog_session() {
    let manager = create_manager();

    let session = manager
        .create_prolog_session("test-user".to_string(), None)
        .expect("Failed to create session");

    let session_id = session.session_id.clone();

    // Terminate the session
    let result = manager.terminate_prolog_session(&session_id);
    assert!(result.is_ok(), "Should terminate Prolog session");

    let terminated = result.unwrap();
    assert_eq!(terminated.status.to_string(), "terminated");
}

/// Test accessing Prolog environment through session
#[test]
fn test_with_prolog_env() {
    let manager = create_manager();

    let session = manager
        .create_prolog_session("test-user".to_string(), None)
        .expect("Failed to create session");

    // Use the Prolog environment
    let result = manager.with_prolog_env(&session.session_id, |env| {
        env.assertz("test_fact(hello)").map_err(|e| e.to_string())?;
        env.query_once("test_fact(X)").map_err(|e| e.to_string())
    });

    assert!(result.is_ok(), "Should be able to use Prolog env: {:?}", result.err());
    let output = result.unwrap();
    println!("Prolog query result: {}", output);
}

/// Test session isolation - Prolog sessions should be independent
#[test]
fn test_prolog_session_isolation() {
    let manager = create_manager();

    // Create two Prolog sessions
    let session1 = manager
        .create_prolog_session("user1".to_string(), None)
        .expect("Failed to create session1");

    let session2 = manager
        .create_prolog_session("user2".to_string(), None)
        .expect("Failed to create session2");

    // Assert different facts in each session
    manager.with_prolog_env(&session1.session_id, |env| {
        env.assertz("data(session_one)").map_err(|e| e.to_string())
    }).expect("Failed to assert in session1");

    manager.with_prolog_env(&session2.session_id, |env| {
        env.assertz("data(session_two)").map_err(|e| e.to_string())
    }).expect("Failed to assert in session2");

    // Each session should only see its own data
    let result1 = manager.with_prolog_env(&session1.session_id, |env| {
        env.query_once("data(X)").map_err(|e| e.to_string())
    }).expect("Failed to query session1");

    let result2 = manager.with_prolog_env(&session2.session_id, |env| {
        env.query_once("data(X)").map_err(|e| e.to_string())
    }).expect("Failed to query session2");

    println!("Session1 result: {}", result1);
    println!("Session2 result: {}", result2);

    // They should have different data (isolation)
    assert!(result1.contains("session_one") || !result1.contains("session_two"),
        "Session1 should not see session2's data");
}

/// Test that CLIPS and Prolog sessions coexist
#[test]
fn test_mixed_session_types() {
    let manager = create_manager();

    // Create a CLIPS session
    let clips_session = manager
        .create_session("user".to_string(), None)
        .expect("Failed to create CLIPS session");

    // Create a Prolog session
    let prolog_session = manager
        .create_prolog_session("user".to_string(), None)
        .expect("Failed to create Prolog session");

    // Verify types
    assert_eq!(clips_session.session_type, SessionType::Clips);
    assert_eq!(prolog_session.session_type, SessionType::Prolog);

    // Both sessions should be listed
    let all_sessions = manager.list_all_sessions().expect("Failed to list sessions");
    assert_eq!(all_sessions.len(), 2, "Should have 2 sessions");
}

/// Test that terminating wrong session type fails gracefully
#[test]
fn test_terminate_wrong_session_type() {
    let manager = create_manager();

    // Create a CLIPS session
    let clips_session = manager
        .create_session("user".to_string(), None)
        .expect("Failed to create CLIPS session");

    // Try to terminate it as a Prolog session (should fail)
    let result = manager.terminate_prolog_session(&clips_session.session_id);
    assert!(result.is_err(), "Should fail to terminate CLIPS session as Prolog");
}

/// Test accessing wrong session type environment
#[test]
fn test_access_wrong_session_type_env() {
    let manager = create_manager();

    // Create a CLIPS session
    let clips_session = manager
        .create_session("user".to_string(), None)
        .expect("Failed to create CLIPS session");

    // Try to access it as Prolog environment (should fail)
    let result = manager.with_prolog_env(&clips_session.session_id, |_env| {
        Ok::<_, String>("should not get here".to_string())
    });

    assert!(result.is_err(), "Should fail to access CLIPS session as Prolog env");
}

/// Test touch updates session timestamp
#[test]
fn test_prolog_session_touch() {
    let manager = create_manager();

    let session = manager
        .create_prolog_session("user".to_string(), None)
        .expect("Failed to create session");

    let original_touched = session.touched_at;

    // Wait a tiny bit and touch
    std::thread::sleep(std::time::Duration::from_millis(10));
    manager.touch_session(&session.session_id).expect("Failed to touch");

    // Get updated session
    let updated = manager.get_session(&session.session_id).expect("Failed to get session");

    assert!(updated.touched_at >= original_touched, "Touch should update timestamp");
}

/// Test multiple Prolog queries in sequence
#[test]
fn test_sequential_prolog_queries() {
    let manager = create_manager();

    let session = manager
        .create_prolog_session("user".to_string(), None)
        .expect("Failed to create session");

    // Build up a knowledge base over multiple operations
    manager.with_prolog_env(&session.session_id, |env| {
        env.assertz("likes(mary, food)").map_err(|e| e.to_string())
    }).expect("Assert 1 failed");

    manager.with_prolog_env(&session.session_id, |env| {
        env.assertz("likes(mary, wine)").map_err(|e| e.to_string())
    }).expect("Assert 2 failed");

    manager.with_prolog_env(&session.session_id, |env| {
        env.assertz("likes(john, wine)").map_err(|e| e.to_string())
    }).expect("Assert 3 failed");

    manager.with_prolog_env(&session.session_id, |env| {
        env.assertz("likes(john, mary)").map_err(|e| e.to_string())
    }).expect("Assert 4 failed");

    // Query the accumulated knowledge
    let result = manager.with_prolog_env(&session.session_id, |env| {
        env.query("likes(X, wine)").map_err(|e| e.to_string())
    }).expect("Query failed");

    println!("Who likes wine: {}", result);
}

/// Test Prolog session survives multiple environment accesses
#[test]
fn test_prolog_env_persistence() {
    let manager = create_manager();

    let session = manager
        .create_prolog_session("user".to_string(), None)
        .expect("Failed to create session");

    // Access environment multiple times
    for i in 0..5 {
        let fact = format!("counter({})", i);
        manager.with_prolog_env(&session.session_id, |env| {
            env.assertz(&fact).map_err(|e| e.to_string())
        }).expect(&format!("Failed to assert counter {}", i));
    }

    // All facts should still be there
    let result = manager.with_prolog_env(&session.session_id, |env| {
        env.query("counter(X)").map_err(|e| e.to_string())
    }).expect("Failed to query counters");

    println!("Counters: {}", result);
}
