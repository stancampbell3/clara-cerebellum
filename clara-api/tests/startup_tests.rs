//! Startup and configuration tests for Clara API server
//!
//! These tests verify that server components initialize correctly.
//! They run outside of an async runtime to properly test the startup sequence.

/// Test that ToolboxManager initializes correctly with all required tools
#[test]
fn test_toolbox_manager_initialization() {
    // Initialize the toolbox
    clara_toolbox::ToolboxManager::init_global();

    // Get the global manager and verify tools are registered
    let manager = clara_toolbox::ToolboxManager::global().lock().unwrap();
    let tools = manager.list_tools();

    println!("Registered tools: {:?}", tools);

    // Verify echo tool is registered
    assert!(
        tools.contains(&"echo".to_string()),
        "Echo tool should be registered"
    );

    // Verify splinteredmind tool is registered
    assert!(
        tools.contains(&"splinteredmind".to_string()),
        "Splinteredmind tool should be registered"
    );

    // Verify we have at least 2 tools
    assert!(
        tools.len() >= 2,
        "Should have at least echo and splinteredmind tools, got: {:?}",
        tools
    );
}

/// Test that Prolog initializes correctly with clara_evaluate/2 predicate
#[test]
fn test_prolog_initialization() {
    // Initialize toolbox first (required for clara_evaluate to work)
    clara_toolbox::ToolboxManager::init_global();

    // Initialize Prolog
    clara_prolog::init_global();

    // Create a Prolog environment and verify clara_evaluate/2 is available
    let env = clara_prolog::PrologEnvironment::new()
        .expect("Failed to create Prolog environment");

    // Check that clara_evaluate/2 predicate exists
    let result = env.query_once("current_predicate(clara_evaluate/2)");
    assert!(
        result.is_ok(),
        "clara_evaluate/2 predicate should be registered"
    );

    println!("Prolog initialized with clara_evaluate/2: {:?}", result);
}

/// Test that server components can be initialized together without conflicts
#[test]
fn test_full_server_initialization_sequence() {
    println!("=== Testing Full Server Initialization Sequence ===");

    // Step 1: Initialize ToolboxManager (must happen before async runtime)
    println!("[1] Initializing ToolboxManager...");
    clara_toolbox::ToolboxManager::init_global();

    let tools = {
        let manager = clara_toolbox::ToolboxManager::global().lock().unwrap();
        manager.list_tools()
    };
    println!("    Registered tools: {:?}", tools);
    assert!(tools.len() >= 2, "Should have at least 2 tools");

    // Step 2: Initialize Prolog
    println!("[2] Initializing Prolog...");
    clara_prolog::init_global();

    // Verify clara_evaluate/2 is registered
    let registered = clara_prolog::register_clara_evaluate();
    println!("    clara_evaluate/2 registered: {}", registered);
    assert!(registered, "clara_evaluate/2 should be registered");

    // Step 3: Create a session manager
    println!("[3] Creating SessionManager...");
    let _session_manager = clara_session::SessionManager::new(
        clara_session::ManagerConfig::default()
    );
    println!("    SessionManager created successfully");

    // Step 4: Verify Prolog environment works with toolbox
    println!("[4] Verifying Prolog + Toolbox integration...");
    let env = clara_prolog::PrologEnvironment::new()
        .expect("Failed to create Prolog environment");

    // Test calling clara_evaluate with echo tool
    let result = env.query_once(
        r#"clara_evaluate('{"tool":"echo","arguments":{"message":"startup test"}}', R)"#
    );

    match &result {
        Ok(r) => {
            println!("    Echo tool result: {}", r);
            assert!(
                r.contains("success") || r.contains("startup test"),
                "Echo should work: {}",
                r
            );
        }
        Err(e) => {
            panic!("clara_evaluate with echo tool failed: {}", e);
        }
    }

    println!("=== Full Server Initialization Sequence PASSED ===");
}

/// Test that clara_evaluate can invoke the splinteredmind tool
/// Note: This test will make HTTP calls to FieryPit at FIERYPIT_URL (default: localhost:6666)
#[test]
fn test_clara_evaluate_with_splinteredmind_health() {
    println!("=== Testing clara_evaluate with splinteredmind (health check) ===");

    // Initialize toolbox and prolog
    clara_toolbox::ToolboxManager::init_global();
    clara_prolog::init_global();

    // Create Prolog environment
    let env = clara_prolog::PrologEnvironment::new()
        .expect("Failed to create Prolog environment");

    // Test splinteredmind health operation
    // This will call FieryPit's /health endpoint
    let result = env.query_once(
        r#"clara_evaluate('{"tool":"splinteredmind","arguments":{"operation":"health"}}', R)"#
    );

    match &result {
        Ok(r) => {
            println!("    Splinteredmind health result: {}", r);
            // The result should contain either success from FieryPit or an HTTP error
            // (if FieryPit isn't running)
        }
        Err(e) => {
            // This is acceptable if FieryPit isn't running
            println!("    Splinteredmind health check error (FieryPit may not be running): {}", e);
        }
    }

    println!("=== clara_evaluate with splinteredmind test completed ===");
}

/// Test that consulting front_desk3.pl works and splinteredmind predicates are available
#[test]
fn test_consult_front_desk_with_splinteredmind_predicates() {
    println!("=== Testing front_desk3.pl consult with splinteredmind predicates ===");

    // Initialize toolbox and prolog
    clara_toolbox::ToolboxManager::init_global();
    clara_prolog::init_global();

    // Create Prolog environment
    let env = clara_prolog::PrologEnvironment::new()
        .expect("Failed to create Prolog environment");

    // Consult front_desk3.pl
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = std::path::Path::new(manifest_dir).parent().unwrap();
    let front_desk_path = workspace_root.join("wok/front_desk3.pl");
    let front_desk_path_str = front_desk_path.to_string_lossy();

    println!("[1] Consulting {}...", front_desk_path_str);
    let consult_result = env.consult_file(&front_desk_path_str);

    match &consult_result {
        Ok(_) => println!("    OK: front_desk3.pl consulted"),
        Err(e) => panic!("Failed to consult front_desk3.pl: {}", e),
    }

    // Verify splinteredmind-dependent predicates are available
    println!("[2] Checking for use_clara/0 predicate...");
    let result = env.query_once("current_predicate(use_clara/0)");
    match &result {
        Ok(r) => println!("    use_clara/0 exists: {}", r),
        Err(e) => panic!("use_clara/0 not found: {}", e),
    }

    println!("[3] Checking for use_echo/0 predicate...");
    let result = env.query_once("current_predicate(use_echo/0)");
    match &result {
        Ok(r) => println!("    use_echo/0 exists: {}", r),
        Err(e) => panic!("use_echo/0 not found: {}", e),
    }

    println!("[4] Checking for ask_llm/3 predicate...");
    let result = env.query_once("current_predicate(ask_llm/3)");
    match &result {
        Ok(r) => println!("    ask_llm/3 exists: {}", r),
        Err(e) => panic!("ask_llm/3 not found: {}", e),
    }

    println!("=== front_desk3.pl consult test PASSED ===");
}

/// Test invoking use_echo predicate (which calls splinteredmind to set evaluator)
/// Note: Requires FieryPit to be running at FIERYPIT_URL
#[test]
fn test_invoke_use_echo_predicate() {
    println!("=== Testing use_echo predicate invocation ===");

    // Initialize toolbox and prolog
    clara_toolbox::ToolboxManager::init_global();
    clara_prolog::init_global();

    // Create Prolog environment
    let env = clara_prolog::PrologEnvironment::new()
        .expect("Failed to create Prolog environment");

    // Consult front_desk3.pl
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = std::path::Path::new(manifest_dir).parent().unwrap();
    let front_desk_path = workspace_root.join("wok/front_desk3.pl");

    println!("[1] Consulting front_desk3.pl...");
    env.consult_file(&front_desk_path.to_string_lossy())
        .expect("Failed to consult front_desk3.pl");
    println!("    OK");

    // Try to invoke use_echo
    // This will call clara_evaluate -> splinteredmind -> FieryPit
    println!("[2] Invoking use_echo predicate...");
    let result = env.query_once("use_echo");

    match &result {
        Ok(r) => {
            println!("    use_echo result: {}", r);
            println!("    SUCCESS: use_echo predicate executed without crash");
        }
        Err(e) => {
            // This is acceptable if FieryPit isn't running
            println!("    use_echo error (FieryPit may not be running): {}", e);
            println!("    Note: This is expected if FieryPit is not available at localhost:6666");
        }
    }

    println!("=== use_echo predicate test completed ===");
}
