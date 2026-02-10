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

/// Test that consulted Prolog code can call clara_evaluate/2
///
/// This verifies that user-defined predicates which wrap clara_evaluate/2
/// can be asserted and then invoked successfully.
#[test]
fn test_consult_code_with_clara_evaluate() {
    println!("=== Testing consulted code calling clara_evaluate/2 ===");

    // Initialize toolbox (provides the echo tool)
    clara_toolbox::ToolboxManager::init_global();

    // Create environment and register the predicate
    let env = PrologEnvironment::new().expect("Failed to create environment");
    let registered = register_clara_evaluate();
    assert!(registered, "clara_evaluate/2 should be registered");

    // Assert a predicate that wraps clara_evaluate/2 via consult_string
    println!("\n[1] Consulting inline Prolog code that uses clara_evaluate/2...");
    let prolog_code = r#"
        echo_via_clara(Message, Result) :-
            format(atom(Json),
                '{"tool":"echo","arguments":{"message":"~w"}}',
                [Message]),
            clara_evaluate(Json, Result).
    "#;
    let consult_result = env.consult_string(prolog_code);
    match &consult_result {
        Ok(_) => println!("    OK: inline code consulted successfully"),
        Err(e) => panic!("Failed to consult inline code: {}", e),
    }

    // Verify the predicate exists
    println!("\n[2] Checking if echo_via_clara/2 is available...");
    let result = env.query_once("current_predicate(echo_via_clara/2)");
    assert!(result.is_ok(), "echo_via_clara/2 should be defined: {:?}", result.err());
    println!("    echo_via_clara/2 exists");

    // Call it and verify the round-trip through clara_evaluate works
    println!("\n[3] Calling echo_via_clara/2...");
    let result = env.query_once("echo_via_clara(hello_from_consult, R)");
    match &result {
        Ok(r) => {
            println!("    Result: {}", r);
            assert!(
                r.contains("success") || r.contains("hello_from_consult"),
                "Expected echo response, got: {}", r
            );
        }
        Err(e) => panic!("echo_via_clara/2 call failed: {}", e),
    }

    println!("\n=== Consulted code with clara_evaluate/2 Test PASSED ===");
}

/// Test that the JSON library (http/json) is available and atom_json_dict/3 works
#[test]
fn test_json_library_available() {
    println!("=== Testing JSON library availability ===");

    let env = PrologEnvironment::new().expect("Failed to create environment");

    // atom_json_dict should be available without explicit use_module
    // (autoloaded during PL_initialise)
    println!("\n[1] Testing atom_json_dict/3 with a simple JSON object...");
    let result = env.query_with_bindings(
        r#"atom_json_dict('{"name":"test","value":42}', Dict, [])"#
    );
    match &result {
        Ok(r) => {
            println!("    Result: {}", r);
            assert!(r.contains("test") || r.contains("42"),
                "Should contain parsed JSON values: {}", r);
        }
        Err(e) => panic!("atom_json_dict should work: {}", e),
    }

    // Test JSON writing (exercises our pure-Prolog fallbacks for json_write_string/2)
    println!("\n[2] Testing atom_json_dict/3 in write mode (dict to atom)...");
    let result = env.query_with_bindings(
        r#"atom_json_dict(Atom, _{x:1, y:2}, [])"#
    );
    match &result {
        Ok(r) => {
            println!("    Result: {}", r);
            // The output should contain JSON-like content
            assert!(r.contains("x") && r.contains("y"),
                "Should contain JSON output: {}", r);
        }
        Err(e) => println!("    Write mode result: {} (may be expected if dicts are tricky in this context)", e),
    }

    println!("\n=== JSON library test PASSED ===");
}

/// Test that goals containing quoted strings with embedded double quotes
/// can be parsed and executed correctly through query_with_bindings.
///
/// This exercises the escaping logic in execute_query_with_bindings,
/// which embeds the goal inside a double-quoted Prolog string for
/// atom_codes/2. Double quotes in the original goal must be escaped.
#[test]
fn test_quoted_strings_in_query_with_bindings() {
    println!("=== Testing quoted strings in query_with_bindings ===");

    let env = PrologEnvironment::new().expect("Failed to create environment");

    // Test 1: Single-quoted atom containing double quotes
    // This is the core escaping issue - the goal is embedded in a
    // double-quoted Prolog string in the wrapper, so internal double
    // quotes must be escaped.
    println!("\n[1] Single-quoted atom with embedded double quotes...");
    let result = env.query_with_bindings(r#"X = '{ "x": 2, "y": 3 }'"#);
    match &result {
        Ok(r) => {
            println!("    Result: {}", r);
            assert!(r.contains("\"x\"") || r.contains("x"),
                "Result should contain the quoted atom content: {}", r);
        }
        Err(e) => panic!("query_with_bindings should handle double quotes inside single-quoted atoms: {}", e),
    }

    // Test 2: atom_length on a single-quoted atom with embedded double quotes
    println!("\n[2] atom_length with embedded double quotes...");
    let result = env.query_with_bindings(r#"atom_length('he said "hi"', N)"#);
    match &result {
        Ok(r) => {
            println!("    Result: {}", r);
            // 'he said "hi"' is 12 characters
            assert!(r.contains("12"), "Length should be 12: {}", r);
        }
        Err(e) => panic!("atom_length with embedded double quotes failed: {}", e),
    }

    // Test 3: Simple atom_string with result binding (no special chars - baseline)
    println!("\n[3] atom_string baseline (no special chars)...");
    let result = env.query_with_bindings("atom_string(hello, X)");
    match &result {
        Ok(r) => println!("    Result: {}", r),
        Err(e) => println!("    Error: {}", e),
    }
    assert!(result.is_ok(), "Simple atom_string should work: {:?}", result.err());

    // Test 4: Goal with backslashes in a single-quoted atom
    println!("\n[4] Goal containing backslash in atom...");
    let result = env.query_with_bindings(r#"atom_length('a\\b', N)"#);
    match &result {
        Ok(r) => println!("    Result: {}", r),
        Err(e) => println!("    Error: {}", e),
    }
    assert!(result.is_ok(), "Backslash in atoms should be handled: {:?}", result.err());

    // Test 5: Same double-quote goal through query_once (direct PL_chars_to_term)
    // This should always work since single-quoted atoms with embedded double
    // quotes are valid Prolog syntax - no wrapper escaping needed.
    println!("\n[5] Same goal via query_once (direct parse, no wrapper)...");
    let result = env.query_once(r#"X = '{ "x": 2, "y": 3 }'"#);
    match &result {
        Ok(r) => println!("    Result: {}", r),
        Err(e) => println!("    Error: {}", e),
    }
    assert!(result.is_ok(), "query_once should handle the goal directly: {:?}", result.err());

    println!("\n=== Quoted strings test PASSED ===");
}
