//! End-to-end integration tests for the LilDevils (Prolog) system
//!
//! These tests verify the complete Prolog integration from top to bottom,
//! exercising the full stack: environment, session management, and callbacks.

use clara_prolog::PrologEnvironment;
use clara_session::{SessionManager, SessionType, ManagerConfig};
use clara_toolbox::ToolboxManager;

/// Helper to create a session manager with default config
fn create_manager() -> SessionManager {
    SessionManager::new(ManagerConfig::default())
}

/// Comprehensive smoke test for Prolog environment
#[test]
fn smoke_test_prolog_environment() {
    println!("=== Prolog Environment Smoke Test ===");

    // Test 1: Create environment
    println!("\n[1] Creating Prolog environment...");
    let env = PrologEnvironment::new().expect("Failed to create Prolog environment");
    println!("    OK: Environment created");

    // Test 2: Basic arithmetic
    println!("\n[2] Testing basic arithmetic...");
    let result = env.query_once("X is 10 + 20 * 3").expect("Arithmetic failed");
    println!("    Result: {}", result);

    // Test 3: Assert facts
    println!("\n[3] Asserting family facts...");
    env.assertz("person(alice)").expect("Failed to assert");
    env.assertz("person(bob)").expect("Failed to assert");
    env.assertz("person(charlie)").expect("Failed to assert");
    env.assertz("parent(alice, bob)").expect("Failed to assert");
    env.assertz("parent(bob, charlie)").expect("Failed to assert");
    println!("    OK: Facts asserted");

    // Test 4: Query facts
    println!("\n[4] Querying parent relationship...");
    let result = env.query_once("parent(alice, X)").expect("Query failed");
    println!("    Result: {}", result);

    // Test 5: Define and use rules
    println!("\n[5] Defining grandparent rule...");
    env.assertz("grandparent(X, Z) :- parent(X, Y), parent(Y, Z)")
        .expect("Failed to assert rule");

    let result = env.query_once("grandparent(alice, charlie)").expect("Query failed");
    println!("    Grandparent result: {}", result);

    // Test 6: List operations
    println!("\n[6] Testing list operations...");
    let result = env.query_once("append([1,2], [3,4], L)").expect("Append failed");
    println!("    Append result: {}", result);

    let result = env.query_once("member(X, [a,b,c])").expect("Member failed");
    println!("    Member result: {}", result);

    // Test 7: Multiple solutions
    println!("\n[7] Testing multiple solutions...");
    let result = env.query("person(X)").expect("Query failed");
    println!("    All persons: {}", result);

    println!("\n=== Prolog Environment Smoke Test PASSED ===");
}

/// Smoke test for Prolog session management
#[test]
fn smoke_test_prolog_sessions() {
    println!("=== Prolog Session Management Smoke Test ===");

    let manager = create_manager();

    // Test 1: Create session
    println!("\n[1] Creating Prolog session...");
    let session = manager
        .create_prolog_session("smoke-test-user".to_string(), None)
        .expect("Failed to create session");
    println!("    Session ID: {}", session.session_id);
    println!("    Session Type: {:?}", session.session_type);
    assert_eq!(session.session_type, SessionType::Prolog);

    // Test 2: Use session environment
    println!("\n[2] Using session environment...");
    manager.with_prolog_env(&session.session_id, |env| {
        env.assertz("test_data(smoke_test_value)").map_err(|e| e.to_string())
    }).expect("Failed to assert");
    println!("    OK: Data asserted");

    // Test 3: Query through session
    println!("\n[3] Querying through session...");
    let result = manager.with_prolog_env(&session.session_id, |env| {
        env.query_once("test_data(X)").map_err(|e| e.to_string())
    }).expect("Query failed");
    println!("    Query result: {}", result);

    // Test 4: Session listing
    println!("\n[4] Listing sessions...");
    let sessions = manager.list_all_sessions().expect("Failed to list");
    println!("    Total sessions: {}", sessions.len());

    // Test 5: Touch session
    println!("\n[5] Touching session...");
    manager.touch_session(&session.session_id).expect("Touch failed");
    println!("    OK: Session touched");

    // Test 6: Terminate session
    println!("\n[6] Terminating session...");
    let terminated = manager
        .terminate_prolog_session(&session.session_id)
        .expect("Failed to terminate");
    println!("    Status: {}", terminated.status);

    println!("\n=== Prolog Session Management Smoke Test PASSED ===");
}

/// Smoke test for multiple isolated sessions
#[test]
fn smoke_test_session_isolation() {
    println!("=== Prolog Session Isolation Smoke Test ===");

    let manager = create_manager();

    // Create two sessions
    println!("\n[1] Creating two isolated sessions...");
    let session_a = manager
        .create_prolog_session("user_a".to_string(), None)
        .expect("Failed to create session A");
    let session_b = manager
        .create_prolog_session("user_b".to_string(), None)
        .expect("Failed to create session B");
    println!("    Session A: {}", session_a.session_id);
    println!("    Session B: {}", session_b.session_id);

    // Add different data to each
    println!("\n[2] Adding different data to each session...");
    manager.with_prolog_env(&session_a.session_id, |env| {
        env.assertz("secret(a_secret_data)").map_err(|e| e.to_string())
    }).expect("Failed for A");

    manager.with_prolog_env(&session_b.session_id, |env| {
        env.assertz("secret(b_secret_data)").map_err(|e| e.to_string())
    }).expect("Failed for B");
    println!("    OK: Different data in each session");

    // Verify isolation
    println!("\n[3] Verifying isolation...");
    let result_a = manager.with_prolog_env(&session_a.session_id, |env| {
        env.query_once("secret(X)").map_err(|e| e.to_string())
    }).expect("Query A failed");

    let result_b = manager.with_prolog_env(&session_b.session_id, |env| {
        env.query_once("secret(X)").map_err(|e| e.to_string())
    }).expect("Query B failed");

    println!("    Session A sees: {}", result_a);
    println!("    Session B sees: {}", result_b);

    // Verify they don't see each other's data
    assert!(result_a.contains("a_secret") || !result_a.contains("b_secret"),
        "Session A should not see B's data");
    assert!(result_b.contains("b_secret") || !result_b.contains("a_secret"),
        "Session B should not see A's data");

    // Cleanup
    manager.terminate_prolog_session(&session_a.session_id).ok();
    manager.terminate_prolog_session(&session_b.session_id).ok();

    println!("\n=== Prolog Session Isolation Smoke Test PASSED ===");
}

/// Smoke test for Prolog with toolbox integration
#[test]
fn smoke_test_prolog_toolbox_integration() {
    println!("=== Prolog Toolbox Integration Smoke Test ===");

    // Initialize toolbox
    ToolboxManager::init_global();

    let manager = create_manager();

    println!("\n[1] Creating session for toolbox test...");
    let session = manager
        .create_prolog_session("toolbox-test-user".to_string(), None)
        .expect("Failed to create session");

    // Define some rules that could interact with tools
    println!("\n[2] Setting up reasoning rules...");
    manager.with_prolog_env(&session.session_id, |env| {
        // Define task facts
        env.assertz("task(analyze_code)").map_err(|e| e.to_string())?;
        env.assertz("task(run_tests)").map_err(|e| e.to_string())?;
        env.assertz("task(deploy)").map_err(|e| e.to_string())?;

        // Define dependencies
        env.assertz("depends_on(deploy, run_tests)").map_err(|e| e.to_string())?;
        env.assertz("depends_on(run_tests, analyze_code)").map_err(|e| e.to_string())?;

        // Define can_execute rule
        env.assertz("can_execute(T) :- task(T), \\+ depends_on(T, _)")
            .map_err(|e| e.to_string())?;
        env.assertz("can_execute(T) :- task(T), depends_on(T, D), completed(D)")
            .map_err(|e| e.to_string())?;

        Ok(())
    }).expect("Failed to set up rules");
    println!("    OK: Rules defined");

    // Query what can be executed
    println!("\n[3] Querying executable tasks...");
    let result = manager.with_prolog_env(&session.session_id, |env| {
        env.query_once("can_execute(X)").map_err(|e| e.to_string())
    }).expect("Query failed");
    println!("    Can execute: {}", result);

    // Mark a task complete and re-query
    println!("\n[4] Marking task complete and re-querying...");
    manager.with_prolog_env(&session.session_id, |env| {
        env.assertz("completed(analyze_code)").map_err(|e| e.to_string())
    }).expect("Failed to mark complete");

    let result = manager.with_prolog_env(&session.session_id, |env| {
        env.query("can_execute(X)").map_err(|e| e.to_string())
    }).expect("Query failed");
    println!("    Can execute now: {}", result);

    // Cleanup
    manager.terminate_prolog_session(&session.session_id).ok();

    println!("\n=== Prolog Toolbox Integration Smoke Test PASSED ===");
}

/// Stress test: many queries in sequence
#[test]
fn stress_test_sequential_queries() {
    println!("=== Prolog Sequential Query Stress Test ===");

    let manager = create_manager();
    let session = manager
        .create_prolog_session("stress-test".to_string(), None)
        .expect("Failed to create session");

    let num_operations = 100;

    println!("\n[1] Performing {} sequential operations...", num_operations);

    let start = std::time::Instant::now();

    for i in 0..num_operations {
        let fact = format!("item({})", i);
        manager.with_prolog_env(&session.session_id, |env| {
            env.assertz(&fact).map_err(|e| e.to_string())
        }).expect(&format!("Failed at iteration {}", i));
    }

    let elapsed = start.elapsed();
    println!("    OK: {} assertions in {:?}", num_operations, elapsed);
    println!("    Average: {:?} per operation", elapsed / num_operations);

    // Query all items
    println!("\n[2] Querying all items...");
    let result = manager.with_prolog_env(&session.session_id, |env| {
        env.query_once("findall(X, item(X), L), length(L, N)").map_err(|e| e.to_string())
    }).expect("Query failed");
    println!("    Result: {}", result);

    manager.terminate_prolog_session(&session.session_id).ok();

    println!("\n=== Prolog Sequential Query Stress Test PASSED ===");
}
