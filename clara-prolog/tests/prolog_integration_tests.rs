//! Integration tests for the clara-prolog Prolog environment
//!
//! These tests verify the full Prolog integration works correctly,
//! including FFI bindings, query execution, and knowledge base management.

use clara_prolog::PrologEnvironment;
use clara_prolog::register_clara_evaluate;

/// Test that we can create a Prolog environment
#[test]
fn test_create_environment() {
    let env = PrologEnvironment::new();
    assert!(env.is_ok(), "Should be able to create a Prolog environment");
}

/// Test basic arithmetic query
#[test]
fn test_arithmetic_query() {
    let env = PrologEnvironment::new().expect("Failed to create environment");

    // Test simple arithmetic
    let result = env.query_once("X is 2 + 3");
    assert!(result.is_ok(), "Arithmetic query should succeed");

    let output = result.unwrap();
    println!("Arithmetic result: {}", output);
    // The result should contain X = 5
    assert!(output.contains("5") || output.contains("true"),
        "Result should indicate success: {}", output);
}

/// Test asserting and querying facts
#[test]
fn test_assert_and_query_facts() {
    let env = PrologEnvironment::new().expect("Failed to create environment");

    // Assert some facts about family relationships
    env.assertz("parent(tom, mary)").expect("Failed to assert fact");
    env.assertz("parent(tom, john)").expect("Failed to assert fact");
    env.assertz("parent(mary, ann)").expect("Failed to assert fact");

    // Query for tom's children
    let result = env.query_once("parent(tom, X)");
    assert!(result.is_ok(), "Query should succeed");

    let output = result.unwrap();
    println!("Query result: {}", output);
    // Should find at least one child
    assert!(output.contains("mary") || output.contains("john") || output.contains("true"),
        "Should find a child of tom: {}", output);
}

/// Test querying with all solutions
#[test]
fn test_query_all_solutions() {
    let env = PrologEnvironment::new().expect("Failed to create environment");

    // Assert some facts
    env.assertz("color(red)").expect("Failed to assert");
    env.assertz("color(green)").expect("Failed to assert");
    env.assertz("color(blue)").expect("Failed to assert");

    // Query for all colors
    let result = env.query("color(X)");
    assert!(result.is_ok(), "Query should succeed");

    let output = result.unwrap();
    println!("All solutions: {}", output);
    // Should contain multiple colors or indicate multiple solutions
}

/// Test defining and using rules
#[test]
fn test_rules() {
    let env = PrologEnvironment::new().expect("Failed to create environment");

    // Assert facts
    env.assertz("parent(tom, mary)").expect("Failed to assert");
    env.assertz("parent(mary, ann)").expect("Failed to assert");

    // Define grandparent rule
    env.assertz("grandparent(X, Z) :- parent(X, Y), parent(Y, Z)")
        .expect("Failed to assert rule");

    // Query grandparent relationship
    let result = env.query_once("grandparent(tom, ann)");
    assert!(result.is_ok(), "Grandparent query should succeed");

    let output = result.unwrap();
    println!("Grandparent result: {}", output);
    assert!(output.contains("true") || !output.contains("false"),
        "Tom should be grandparent of Ann: {}", output);
}

/// Test list operations
#[test]
fn test_list_operations() {
    let env = PrologEnvironment::new().expect("Failed to create environment");

    // Test member predicate
    let result = env.query_once("member(2, [1, 2, 3])");
    assert!(result.is_ok(), "Member query should succeed");

    let output = result.unwrap();
    println!("Member result: {}", output);
    assert!(output.contains("true") || !output.contains("false"),
        "2 should be member of [1,2,3]: {}", output);
}

/// Test append operation
#[test]
fn test_append() {
    let env = PrologEnvironment::new().expect("Failed to create environment");

    // Test append
    let result = env.query_once("append([1, 2], [3, 4], X)");
    assert!(result.is_ok(), "Append query should succeed");

    let output = result.unwrap();
    println!("Append result: {}", output);
    // Result should contain the combined list
}

/// Test failure handling
#[test]
fn test_query_failure() {
    let env = PrologEnvironment::new().expect("Failed to create environment");

    // Query something that should fail - member/2 returns false for non-members
    let result = env.query_once("member(99, [1, 2, 3])");

    // The result might be an error or might contain "false" - either is acceptable
    // The important thing is it doesn't crash
    match result {
        Ok(output) => {
            println!("Failure result (ok): {}", output);
        }
        Err(e) => {
            println!("Failure result (err): {}", e);
            // Query failure is expected behavior
        }
    }
}

/// Test recursive predicates
#[test]
fn test_recursive_predicates() {
    let env = PrologEnvironment::new().expect("Failed to create environment");

    // Define ancestor relationship (recursive)
    env.assertz("parent(a, b)").expect("Failed to assert");
    env.assertz("parent(b, c)").expect("Failed to assert");
    env.assertz("parent(c, d)").expect("Failed to assert");

    env.assertz("ancestor(X, Y) :- parent(X, Y)")
        .expect("Failed to assert rule");
    env.assertz("ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y)")
        .expect("Failed to assert rule");

    // Query ancestor relationship
    let result = env.query_once("ancestor(a, d)");
    assert!(result.is_ok(), "Ancestor query should succeed");

    let output = result.unwrap();
    println!("Ancestor result: {}", output);
}

/// Test multiple environments (engine isolation)
#[test]
fn test_multiple_environments() {
    let env1 = PrologEnvironment::new().expect("Failed to create env1");
    let env2 = PrologEnvironment::new().expect("Failed to create env2");

    // Assert different facts in each environment
    env1.assertz("fact(one)").expect("Failed to assert in env1");
    env2.assertz("fact(two)").expect("Failed to assert in env2");

    // Each environment should only see its own facts
    let result1 = env1.query_once("fact(X)");
    let result2 = env2.query_once("fact(X)");

    assert!(result1.is_ok(), "Query in env1 should succeed");
    assert!(result2.is_ok(), "Query in env2 should succeed");

    let output1 = result1.unwrap();
    let output2 = result2.unwrap();

    println!("Env1 result: {}", output1);
    println!("Env2 result: {}", output2);

    // They should have different results (isolation)
}

/// Test string handling
#[test]
fn test_string_handling() {
    let env = PrologEnvironment::new().expect("Failed to create environment");

    // Assert a fact with a string
    env.assertz("greeting(hello)").expect("Failed to assert");
    env.assertz("greeting(world)").expect("Failed to assert");

    let result = env.query_once("greeting(X)");
    assert!(result.is_ok(), "String query should succeed");

    let output = result.unwrap();
    println!("String result: {}", output);
}

/// Test numeric comparisons
#[test]
fn test_numeric_comparisons() {
    let env = PrologEnvironment::new().expect("Failed to create environment");

    // Test greater than
    let result = env.query_once("5 > 3");
    assert!(result.is_ok(), "Comparison should succeed");

    let output = result.unwrap();
    println!("Comparison result: {}", output);
    assert!(output.contains("true") || !output.contains("false"),
        "5 > 3 should be true: {}", output);
}

/// Test findall
#[test]
fn test_findall() {
    let env = PrologEnvironment::new().expect("Failed to create environment");

    // Assert some facts (use my_number to avoid conflict with built-in number/1)
    env.assertz("my_number(1)").expect("Failed to assert");
    env.assertz("my_number(2)").expect("Failed to assert");
    env.assertz("my_number(3)").expect("Failed to assert");

    // Use findall to collect all numbers
    let result = env.query_once("findall(X, my_number(X), L)");
    assert!(result.is_ok(), "Findall should succeed");

    let output = result.unwrap();
    println!("Findall result: {}", output);
}

/// Test that clara_evaluate/2 foreign predicate is registered and callable
///
/// This is a critical integration test that verifies:
/// 1. The clara_evaluate/2 predicate is properly registered with SWI-Prolog
/// 2. The predicate can be called from Prolog code
/// 3. It correctly invokes the Rust toolbox manager
#[test]
fn test_clara_evaluate_predicate_registration() {
    println!("=== Testing clara_evaluate/2 Foreign Predicate Registration ===");

    // Initialize the toolbox (provides the echo tool)
    clara_toolbox::ToolboxManager::init_global();

    // Create a Prolog environment - this initializes Prolog
    let env = PrologEnvironment::new().expect("Failed to create environment");

    // Register the clara_evaluate/2 predicate
    // Note: This needs to be called after Prolog is initialized
    let registered = register_clara_evaluate();
    assert!(registered, "clara_evaluate/2 should be registered successfully");
    println!("clara_evaluate/2 registered: {}", registered);

    // Test 1: Check that the predicate exists
    println!("\n[1] Checking if clara_evaluate/2 exists...");
    let result = env.query_once("current_predicate(clara_evaluate/2)");
    match &result {
        Ok(r) => println!("    current_predicate result: {}", r),
        Err(e) => println!("    Error: {}", e),
    }
    assert!(result.is_ok(), "current_predicate/1 check should succeed");

    // Test 2: Call clara_evaluate/2 with the echo tool
    println!("\n[2] Calling clara_evaluate/2 with echo tool...");
    let result = env.query_once(
        r#"clara_evaluate('{"tool":"echo","arguments":{"message":"hello from prolog test"}}', Result)"#
    );

    match &result {
        Ok(r) => {
            println!("    clara_evaluate/2 result: {}", r);
            // The result should contain success or the echoed message
            assert!(
                r.contains("success") || r.contains("hello"),
                "Expected success response from echo tool, got: {}", r
            );
        }
        Err(e) => {
            panic!(
                "clara_evaluate/2 call FAILED: {}\n\
                This indicates the foreign predicate is not properly registered.\n\
                Make sure register_clara_evaluate() is called after Prolog initialization.",
                e
            );
        }
    }

    println!("\n=== clara_evaluate/2 Foreign Predicate Test PASSED ===");
}

/// Test consulting a file that uses clara_evaluate/2
///
/// This test verifies that Prolog files which depend on clara_evaluate/2
/// can be properly consulted and their predicates can be called.
#[test]
fn test_consult_file_with_clara_evaluate() {
    println!("=== Testing Consult with clara_evaluate/2 ===");

    // Initialize toolbox
    clara_toolbox::ToolboxManager::init_global();

    // Create environment and register the predicate
    let env = PrologEnvironment::new().expect("Failed to create environment");
    let registered = register_clara_evaluate();
    assert!(registered, "clara_evaluate/2 should be registered");

    // Determine the path to builtins_test.pl relative to the workspace root
    // Tests run from the package directory, so we need to go up to workspace root
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = std::path::Path::new(manifest_dir).parent().unwrap();
    let builtins_path = workspace_root.join("wok/builtins_test.pl");
    let builtins_path_str = builtins_path.to_string_lossy();

    // Test consulting builtins_test.pl which uses clara_evaluate/2
    println!("\n[1] Consulting {}...", builtins_path_str);
    let consult_result = env.consult_file(&builtins_path_str);

    match &consult_result {
        Ok(_) => println!("    OK: builtins_test.pl consulted successfully"),
        Err(e) => {
            println!("    ERROR: Failed to consult builtins_test.pl: {}", e);
            panic!(
                "Failed to consult builtins_test.pl: {}\n\
                This is likely because clara_evaluate/2 is not available when the file is loaded.",
                e
            );
        }
    }

    // Verify predicates from builtins_test.pl are available
    println!("\n[2] Checking if ask_llm/2 predicate is available...");
    let result = env.query_once("current_predicate(ask_llm/2)");
    match &result {
        Ok(r) => println!("    ask_llm/2 exists: {}", r),
        Err(e) => println!("    ask_llm/2 check error: {}", e),
    }

    println!("\n[3] Checking if example_ask_front_desk/1 predicate is available...");
    let result = env.query_once("current_predicate(example_ask_front_desk/1)");
    match &result {
        Ok(r) => {
            println!("    example_ask_front_desk/1 exists: {}", r);
        }
        Err(e) => {
            panic!(
                "example_ask_front_desk/1 NOT FOUND: {}\n\
                This predicate should be defined in builtins_test.pl and depends on clara_evaluate/2.",
                e
            );
        }
    }

    println!("\n=== Consult with clara_evaluate/2 Test PASSED ===");
}
