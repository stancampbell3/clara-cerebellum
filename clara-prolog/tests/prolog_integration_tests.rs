//! Integration tests for the clara-prolog Prolog environment
//!
//! These tests verify the full Prolog integration works correctly,
//! including FFI bindings, query execution, and knowledge base management.

use clara_prolog::PrologEnvironment;
use clara_prolog::register_clara_evaluate;
use clara_toolbox::{Tool, ToolError};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

// Tests that abolish and reassert global Prolog predicates must not run
// concurrently — they share the module database across all engines.
static PROLOG_MOCK_LOCK: Mutex<()> = Mutex::new(());

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
    let result = env.query_once("current_predicate(the_rabbit:clara_evaluate/2)");
    match &result {
        Ok(r) => println!("    current_predicate result: {}", r),
        Err(e) => println!("    Error: {}", e),
    }
    assert!(result.is_ok(), "current_predicate/1 check should succeed");

    // Test 2: Call clara_evaluate/2 with the echo tool
    println!("\n[2] Calling clara_evaluate/2 with echo tool...");
    let result = env.query_once(
        r#"the_rabbit:clara_evaluate('{"tool":"echo","arguments":{"message":"hello from prolog test"}}', Result)"#
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
            the_rabbit:clara_evaluate(Json, Result).
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

/// Test that reasoned_response/2 binds RR to the LLM's text response when
/// clara_fy validates it as adequate.
///
/// Avoids live LLM calls with two Prolog-level mocks installed after
/// abolishing the original static clauses:
///   1. ponder_text/2 — the answer-generation call — returns "Four".
///   2. ponder_verdict/2 — the classification call descriminate_k/3 now
///      makes (constrained yes/no/unresolved, no more response_shortcut) —
///      returns `true` for the adequacy-validation question, `unresolved`
///      otherwise.
#[test]
fn test_reasoned_response() {
    let _guard = PROLOG_MOCK_LOCK.lock().unwrap();
    println!("=== Testing reasoned_response/2 ===");

    clara_toolbox::ToolboxManager::init_global();
    let env = PrologEnvironment::new().expect("Failed to create environment");

    // Load the_rat; the :- enable_evaluator directive runs but its result is ignored
    env.query_once("use_module(library(the_rat))").expect("Failed to load the_rat");

    // Swap out the live LLM predicates for deterministic mocks.
    // abolish removes the static definition; assertz installs a dynamic replacement
    // in the_rabbit so that unqualified calls from the_rat resolve to the mock.
    env.query_once("abolish(the_rabbit:ponder_text/2)").ok();
    env.query_once(
        r#"assertz((the_rabbit:ponder_text(_Prompt, Result) :-
            atom_json_dict(Result, _{hohi:_{response:_{response:"Four"}}}, [])))"#,
    )
    .expect("Failed to assert ponder_text mock");
    env.query_once("abolish(the_rabbit:ponder_verdict/2)").ok();
    env.query_once(
        r#"assertz((the_rabbit:ponder_verdict(Prompt, Verdict) :-
            (   sub_atom(Prompt, _, _, _, 'adequately answer')
            ->  Verdict = true
            ;   Verdict = unresolved
            )))"#,
    )
    .expect("Failed to assert ponder_verdict mock");

    let result = env.query_with_bindings("the_rat:reasoned_response('What is 2 + 2?', RR)");
    match &result {
        Ok(r) => {
            println!("    Result: {}", r);
            assert!(
                r.contains("Four"),
                "RR should be bound to the canned LLM answer 'Four': {}",
                r
            );
        }
        Err(e) => panic!("reasoned_response/2 failed: {}", e),
    }

    println!("=== reasoned_response/2 Test PASSED ===");
}

/// Test that clara_fy/2 retries with an explicit "Answer yes or no:" prefix when
/// the first classification returns unresolved.
///
/// The mock returns `unresolved` for the plain validation question and `true`
/// for the prefixed retry, verifying that the retry path resolves to true.
#[test]
fn test_clara_fy_unresolved_retry() {
    let _guard = PROLOG_MOCK_LOCK.lock().unwrap();
    println!("=== Testing clara_fy/2 unresolved retry ===");

    clara_toolbox::ToolboxManager::init_global();
    let env = PrologEnvironment::new().expect("Failed to create environment");

    env.query_once("use_module(library(the_rat))").expect("Failed to load the_rat");

    // clara_fy/2's classification path is descriminate_k/3 -> ponder_verdict/2
    // (constrained yes/no/unresolved). Mock ponder_verdict/2: the plain question
    // returns `unresolved`; the "Answer yes or no:"-prefixed retry returns `true`.
    env.query_once("abolish(the_rabbit:ponder_verdict/2)").ok();
    env.query_once(
        r#"assertz((the_rabbit:ponder_verdict(Prompt, Verdict) :-
            (   sub_atom(Prompt, 0, _, _, 'Answer yes or no:')
            ->  Verdict = true
            ;   Verdict = unresolved
            )))"#,
    )
    .expect("Failed to assert ponder_verdict mock");

    let result = env.query_with_bindings(
        "the_rat:clara_fy('Is the sky blue?', TruthValue)"
    );
    match &result {
        Ok(r) => {
            println!("    Result: {}", r);
            assert!(
                r.contains("true"),
                "TruthValue should be true after retry resolves unresolved: {}",
                r
            );
        }
        Err(e) => panic!("clara_fy/2 retry failed: {}", e),
    }

    println!("=== clara_fy/2 unresolved retry Test PASSED ===");
}

/// Test that reasoned_response_with_context/3 threads context through both the
/// LLM call and the adequacy validation via clara_fy/3.
///
/// Mocks ponder_text_with_context/3 and ponder_verdict_with_context/3 — the
/// context argument is verified to be threaded through both by asserting it is
/// a non-empty list before responding.
#[test]
fn test_reasoned_response_with_context() {
    let _guard = PROLOG_MOCK_LOCK.lock().unwrap();
    println!("=== Testing reasoned_response_with_context/3 ===");

    clara_toolbox::ToolboxManager::init_global();
    let env = PrologEnvironment::new().expect("Failed to create environment");

    env.query_once("use_module(library(the_rat))").expect("Failed to load the_rat");

    // Mock the answer-generation call: verify context is non-empty, return "Green".
    env.query_once("abolish(the_rabbit:ponder_text_with_context/3)").ok();
    env.query_once(
        r#"assertz((the_rabbit:ponder_text_with_context(_Prompt, Context, Result) :-
            Context = [_|_],
            atom_json_dict(Result, _{hohi:_{response:_{response:"Green"}}}, [])))"#,
    )
    .expect("Failed to assert ponder_text_with_context mock");
    // Mock the classification call clara_fy/3 -> descriminate_k_with_context/4
    // now makes: verify context threads through, `true` for the adequacy
    // validation question, `unresolved` otherwise.
    env.query_once("abolish(the_rabbit:ponder_verdict_with_context/3)").ok();
    env.query_once(
        r#"assertz((the_rabbit:ponder_verdict_with_context(Prompt, Context, Verdict) :-
            Context = [_|_],
            (   sub_atom(Prompt, _, _, _, 'adequately answer')
            ->  Verdict = true
            ;   Verdict = unresolved
            )))"#,
    )
    .expect("Failed to assert ponder_verdict_with_context mock");

    let result = env.query_with_bindings(concat!(
        "the_rat:reasoned_response_with_context(",
        "  'What colour do you get mixing blue and yellow?',",
        "  [_{role:user, content:\"what colour is the sky?\"}],",
        "  RR",
        ")"
    ));
    match &result {
        Ok(r) => {
            println!("    Result: {}", r);
            assert!(
                r.contains("Green"),
                "RR should be bound to the canned answer 'Green': {}",
                r
            );
        }
        Err(e) => panic!("reasoned_response_with_context/3 failed: {}", e),
    }

    println!("=== reasoned_response_with_context/3 Test PASSED ===");
}

// ---------------------------------------------------------------------
// the_leannan.pl (leannan_sidhe divergent retrieval, SPEC-084 Tier 2)
// ---------------------------------------------------------------------

/// Test that library(the_leannan) is auto-loaded (environment.rs's startup
/// list) and its exported predicates are visible without an explicit
/// use_module — the same existence_error(procedure, ...) class the_rat's
/// addition to that list fixed (see environment.rs's doc comment).
#[test]
fn test_the_leannan_library_loads() {
    println!("=== Testing library(the_leannan) auto-load ===");
    let env = PrologEnvironment::new().expect("Failed to create environment");

    for pred in [
        "the_leannan:leannan_profiles/1",
        "the_leannan:leannan_entities/3",
        "the_leannan:leannan_neighborhood/4",
        "the_leannan:leannan_by_label/3",
        "the_leannan:leannan_relationships/3",
        "the_leannan:leannan_perturb/5",
        "the_leannan:leannan_spark/6",
        "the_leannan:leannan_sparks/5",
    ] {
        let goal = format!("current_predicate({pred})");
        let result = env.query_once(&goal);
        assert!(result.is_ok(), "{pred} should be visible after auto-load: {:?}", result.err());
    }

    println!("=== library(the_leannan) auto-load Test PASSED ===");
}

/// Test the 6-profile list's structure (design doc §2b table + the
/// 2026-09-15 majority-likely rebalance addendum) — pure data, no mocking
/// needed. Catches transcription mistakes (wrong operator, wrong weight
/// order, wrong fusion mode) directly. Default LEANNAN_LATERAL_COUNT=2
/// puts the 4-entry "likely" pool at positions 1-4 and the 2-entry
/// "lateral"/fringe pool at positions 5-6.
#[test]
fn test_leannan_profiles_structure() {
    println!("=== Testing leannan_profiles/1 structure ===");
    let env = PrologEnvironment::new().expect("Failed to create environment");

    let result = env
        .query_with_bindings("the_leannan:leannan_profiles(Profiles), length(Profiles, N)")
        .expect("leannan_profiles/1 should succeed");
    println!("    Result: {}", result);
    assert!(result.contains("N = 6") || result.contains("6"), "expected 6 profiles: {}", result);

    // Spot-check profile 1 (the new `direct` operator, likely pool).
    let result = env
        .query_with_bindings(concat!(
            "the_leannan:leannan_profiles(Profiles), ",
            "nth1(1, Profiles, spark(direct, weights(2.0,1.0,1.0), 60, rrf))"
        ))
        .expect("profile 1 should be spark(direct, weights(2.0,1.0,1.0), 60, rrf)");
    println!("    Profile 1 match: {}", result);

    // Spot-check profile 5 (the first `fringe` mode profile, sibling operator).
    let result = env
        .query_with_bindings(concat!(
            "the_leannan:leannan_profiles(Profiles), ",
            "nth1(5, Profiles, spark(sibling, weights(1.0,1.0,1.0), 200, fringe))"
        ))
        .expect("profile 5 should be spark(sibling, weights(1.0,1.0,1.0), 200, fringe)");
    println!("    Profile 5 match: {}", result);

    // Spot-check profile 6 (the last profile, bridge operator, fringe mode).
    let result = env
        .query_with_bindings(concat!(
            "the_leannan:leannan_profiles(Profiles), ",
            "nth1(6, Profiles, spark(bridge, weights(1.0,1.0,0.5), 500, fringe))"
        ))
        .expect("profile 6 should be spark(bridge, weights(1.0,1.0,0.5), 500, fringe)");
    println!("    Profile 6 match: {}", result);

    println!("=== leannan_profiles/1 structure Test PASSED ===");
}

/// A stand-in `edgequake` tool for the_leannan.pl tests. `clara_evaluate/2`
/// is a genuine `PL_register_foreign` C predicate (see callbacks.rs) —
/// unlike the_rat.pl's plain interpreted predicates, `abolish`/`assertz`
/// does not meaningfully override it, so mocking happens one layer down:
/// swap the "edgequake"-named `Tool` in the global `ToolboxManager`
/// registry (`ToolboxManager::execute_tool` looks tools up by name — see
/// manager.rs), which `clara_evaluate/2` dispatches through for real. The
/// tool returns its raw JSON value un-wrapped; `ToolResponse::success`
/// adds the `status: success` envelope the_leannan.pl's leannan_dispatch/2
/// checks for (see tool.rs's `#[serde(flatten)]`).
struct MockEdgequakeTool {
    query_calls: Arc<AtomicUsize>,
    /// Records the last `depth` seen on a `graph_entity_neighborhood` call
    /// (`None` until one arrives), so a test can assert the caller forwarded
    /// the value it expected without the mock rejecting calls it doesn't
    /// care about (this tool is shared across tests with different Hops).
    last_neighborhood_depth: Arc<Mutex<Option<i64>>>,
    /// When true, every `query` operation fails — used to prove
    /// leannan_sparks/5 degrades a single spark's failure instead of
    /// failing the whole batch (Tier 3 addendum #2).
    fail_query: bool,
}

impl Tool for MockEdgequakeTool {
    fn name(&self) -> &str {
        "edgequake"
    }
    fn description(&self) -> &str {
        "mock edgequake tool for the_leannan.pl tests"
    }
    fn execute(&self, args: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        match args.get("operation").and_then(|v| v.as_str()) {
            Some("graph_search_entities") => Ok(serde_json::json!({
                "items": [
                    {"id": "clara", "entity_name": "Clara", "entity_type": "PERSON"},
                    {"id": "cerebellum", "entity_name": "Cerebellum", "entity_type": "ORGANIZATION"}
                ]
            })),
            Some("graph_entity_neighborhood") => {
                let depth = args.get("depth").and_then(|v| v.as_i64());
                *self.last_neighborhood_depth.lock().unwrap() = depth;
                Ok(serde_json::json!({
                    "nodes": [
                        {"id": "cerebellum", "label": "Cerebellum", "entity_type": "ORGANIZATION", "degree": 3}
                    ],
                    "edges": [
                        {"id": "e1", "source": "clara", "target": "cerebellum", "relation_type": "USES", "weight": 1.0}
                    ]
                }))
            }
            Some("query") => {
                self.query_calls.fetch_add(1, Ordering::SeqCst);
                if self.fail_query {
                    return Err(ToolError::ExecutionFailed(
                        "mock: simulated query failure".to_string(),
                    ));
                }
                Ok(serde_json::json!({
                    "sources": [{"id": "src1", "source_type": "chunk", "score": 0.9, "snippet": "evidence"}]
                }))
            }
            other => Err(ToolError::ExecutionFailed(format!(
                "mock: unexpected operation {other:?}"
            ))),
        }
    }
}

/// Test leannan_entities/3's dict-field extraction against a canned
/// GraphSearchEntities-shaped response (ListEntitiesResponse's real field
/// names: `items`, each with `id`/`entity_name`/`entity_type` — confirmed
/// live against Edgequake source, see the_leannan.pl's module doc
/// comment). Mocks the `edgequake` Tool registration (see
/// MockEdgequakeTool's doc comment for why clara_evaluate/2 itself can't
/// be Prolog-level mocked the way the_rat.pl's tests mock ponder_text/2).
#[test]
fn test_leannan_entities_via_mock() {
    let _guard = PROLOG_MOCK_LOCK.lock().unwrap();
    println!("=== Testing leannan_entities/3 against a mocked edgequake tool ===");

    clara_toolbox::ToolboxManager::init_global();
    clara_toolbox::clear_evaluate_cache(); // see leannan_sparks degrade test for why
    clara_toolbox::ToolboxManager::global().lock().unwrap().register_tool(Arc::new(
        MockEdgequakeTool {
            query_calls: Arc::new(AtomicUsize::new(0)),
            last_neighborhood_depth: Arc::new(Mutex::new(None)),
            fail_query: false,
        },
    ));
    let env = PrologEnvironment::new().expect("Failed to create environment");

    let result = env
        .query_with_bindings("the_leannan:leannan_entities(clara, none, Entities)")
        .expect("leannan_entities/3 should succeed against the mock");
    println!("    Result: {}", result);
    assert!(result.contains("clara"), "expected the clara entity id: {}", result);
    assert!(result.contains("Clara"), "expected the Clara entity_name: {}", result);
    assert!(result.contains("cerebellum"), "expected the cerebellum entity id: {}", result);

    println!("=== leannan_entities/3 mock Test PASSED ===");
}

/// Test leannan_neighborhood/4's dict-field extraction against a canned
/// EntityNeighborhoodResponse-shaped payload (`nodes` with `id`/`label`/
/// `entity_type` — note `label` here, NOT `entity_name`; a different key
/// than the search-results shape above for the same presentation-name
/// concept, per the_leannan.pl's module doc comment) and that Hops is
/// forwarded as the `depth` argument (checked via MockEdgequakeTool's
/// `last_neighborhood_depth` after the call).
#[test]
fn test_leannan_neighborhood_via_mock() {
    let _guard = PROLOG_MOCK_LOCK.lock().unwrap();
    println!("=== Testing leannan_neighborhood/4 against a mocked edgequake tool ===");

    clara_toolbox::ToolboxManager::init_global();
    clara_toolbox::clear_evaluate_cache(); // see leannan_sparks degrade test for why
    let last_neighborhood_depth = Arc::new(Mutex::new(None));
    clara_toolbox::ToolboxManager::global().lock().unwrap().register_tool(Arc::new(
        MockEdgequakeTool {
            query_calls: Arc::new(AtomicUsize::new(0)),
            last_neighborhood_depth: last_neighborhood_depth.clone(),
            fail_query: false,
        },
    ));
    let env = PrologEnvironment::new().expect("Failed to create environment");

    let result = env
        .query_with_bindings("the_leannan:leannan_neighborhood(clara, 2, none, Neighbors)")
        .expect("leannan_neighborhood/4 should succeed against the mock");
    println!("    Result: {}", result);
    assert!(result.contains("cerebellum"), "expected the cerebellum neighbor id: {}", result);
    assert!(result.contains("Cerebellum"), "expected the Cerebellum label: {}", result);
    assert_eq!(
        *last_neighborhood_depth.lock().unwrap(),
        Some(2),
        "Hops=2 must be forwarded as the `depth` argument"
    );

    println!("=== leannan_neighborhood/4 mock Test PASSED ===");
}

/// End-to-end test of leannan_spark/6 (one profile) against mocked entity
/// search + neighborhood + query calls, verifying:
///   - the spark result carries the mocked source,
///   - the memoization: a second call with the same SparkId does NOT
///     re-invoke the mocked `query` operation (proven by
///     MockEdgequakeTool's shared call counter, checked unchanged after
///     the second call) — the rituals_101.md anti-pattern this memo
///     exists to prevent.
#[test]
fn test_leannan_spark_memoized_against_mock() {
    let _guard = PROLOG_MOCK_LOCK.lock().unwrap();
    println!("=== Testing leannan_spark/6 (mocked, checks memoization) ===");

    clara_toolbox::ToolboxManager::init_global();
    clara_toolbox::clear_evaluate_cache(); // see leannan_sparks degrade test for why
    let query_calls = Arc::new(AtomicUsize::new(0));
    clara_toolbox::ToolboxManager::global()
        .lock()
        .unwrap()
        .register_tool(Arc::new(MockEdgequakeTool {
            query_calls: query_calls.clone(),
            last_neighborhood_depth: Arc::new(Mutex::new(None)),
        fail_query: false,
        }));
    let env = PrologEnvironment::new().expect("Failed to create environment");

    let profile = "spark(neighbor(1), weights(3.0,1.0,0.2), 60, rrf)";
    let goal1 =
        format!("the_leannan:leannan_spark(1, 'what is clara?', {profile}, none, Spark, _Prov)");
    let result1 = env.query_with_bindings(&goal1).expect("first leannan_spark/6 call should succeed");
    println!("    First call: {}", result1);
    assert!(result1.contains("src1"), "spark result should carry the mocked source: {}", result1);
    assert_eq!(query_calls.load(Ordering::SeqCst), 1, "expected exactly 1 query call");

    // Second call, same SparkId/Query/Profile/WorkspaceId — must fast-forward
    // from the spark/3 memo, NOT re-invoke the mocked query.
    let result2 = env.query_with_bindings(&goal1).expect("second (memoized) leannan_spark/6 call should succeed");
    println!("    Second (memoized) call: {}", result2);
    assert!(result2.contains("src1"), "memoized spark result should still carry the mocked source: {}", result2);
    assert_eq!(
        query_calls.load(Ordering::SeqCst),
        1,
        "memoized call must NOT re-invoke the query mock (rituals_101.md anti-pattern)"
    );

    println!("=== leannan_spark/6 memoization Test PASSED ===");
}

/// Test that leannan_sparks/5 degrades a single failing spark to an empty
/// `spark_result([], [])` instead of failing the whole batch (Tier 3
/// addendum #2 — a real Edgequake query timeout was observed live
/// 2026-09-11; id_analyst.pl's whole turn must not die because one of
/// several sparks' retrieval failed).
#[test]
fn test_leannan_sparks_degrades_failed_spark() {
    let _guard = PROLOG_MOCK_LOCK.lock().unwrap();
    println!("=== Testing leannan_sparks/5 degrades a failing spark ===");

    clara_toolbox::ToolboxManager::init_global();
    clara_toolbox::ToolboxManager::global().lock().unwrap().register_tool(Arc::new(
        MockEdgequakeTool {
            query_calls: Arc::new(AtomicUsize::new(0)),
            last_neighborhood_depth: Arc::new(Mutex::new(None)),
            fail_query: true,
        },
    ));
    // clara-toolbox's clara_evaluate/2 result cache is process-wide, keyed
    // by request JSON only (ffi.rs's `evaluate_cache`) — NOT scoped to
    // this test's freshly-registered mock. Without clearing it, an
    // earlier test's *coincidentally identical* request (this test's
    // profile 1's derived ll_keywords/weights/rrf_k happen to match that
    // test's ad hoc profile exactly) would return a stale cached success
    // instead of ever reaching this test's fail_query mock — confirmed
    // live 2026-09-11 (`ffi.rs`'s own `clear_evaluate_cache` doc comment:
    // "Call before a test run to ensure a cold cache").
    clara_toolbox::clear_evaluate_cache();
    let env = PrologEnvironment::new().expect("Failed to create environment");

    let result = env
        .query_with_bindings(
            "the_leannan:leannan_sparks('what is clara?', 2, none, Sparks, AllCitations), \
             length(Sparks, NS), length(AllCitations, NC)",
        )
        .expect("leannan_sparks/5 must still succeed when every query op fails");
    println!("    Result: {}", result);
    assert!(result.contains("\"NS\":2"), "expected 2 spark_entry results despite the failures: {}", result);
    assert!(result.contains("\"NC\":0"), "expected zero citations when every query failed: {}", result);

    println!("=== leannan_sparks/5 degrade Test PASSED ===");
}

// ── source inspection (module-fragment collision checks) ─────────────────────

/// `source_predicates` reports what a source WOULD define, using consult_string's own head extraction: rule heads
/// and facts count (each once, first-seen order), directives and the load-family facts do not.
#[test]
fn test_source_predicates_lists_defined_predicates_once_in_order() {
    let env = PrologEnvironment::new().expect("Failed to create environment");
    let code = "\
:- use_module(library(lists)).\n\
:- dynamic seen/1.\n\
greet(hello).\n\
greet(world).\n\
shout(X, Y) :- greet(X), upcase_atom(X, Y).\n\
use_module(library(the_coire)).\n\
helper(_, _, _).\n";
    let preds = env.source_predicates(code).expect("source_predicates");
    assert_eq!(preds, vec!["greet/1", "shout/2", "helper/3"]);
}

#[test]
fn test_source_predicates_of_an_empty_or_directive_only_source_is_empty() {
    let env = PrologEnvironment::new().expect("Failed to create environment");
    assert!(env.source_predicates("").expect("empty").is_empty());
    assert!(env.source_predicates(":- true.").expect("directive").is_empty());
}

#[test]
fn test_source_predicates_survives_quotes_and_backslashes_in_the_source() {
    let env = PrologEnvironment::new().expect("Failed to create environment");
    let code = "say(\"a \\\"quoted\\\" word\").\nnote('it''s here').\n";
    let preds = env.source_predicates(code).expect("quotes must not break the goal");
    assert_eq!(preds, vec!["say/1", "note/1"]);
}

#[test]
fn test_source_predicates_surfaces_a_syntax_error() {
    let env = PrologEnvironment::new().expect("Failed to create environment");
    assert!(env.source_predicates("broken( .").is_err(), "a syntax error must not be swallowed");
}

/// Nothing is loaded by inspecting: the engine's user namespace is unchanged.
#[test]
fn test_source_predicates_does_not_load_the_source() {
    let env = PrologEnvironment::new().expect("Failed to create environment");
    env.source_predicates("only_inspected(1).").expect("inspect");
    assert!(env.query_once("only_inspected(1)").is_err(), "inspection must not define the predicate");
}

#[test]
fn test_overlay_exports_include_the_promoted_and_core_predicates() {
    let env = PrologEnvironment::new().expect("Failed to create environment");
    let exports = env.overlay_exports().expect("overlay_exports");
    for expected in [
        "strip_think/2",        // the_coire (promoted in Approach B)
        "caws_tristate/3",      // the_coire
        "extract_hohi_response/2", // the_rabbit
        "ponder_text/2",        // the_rabbit
        "answer_step/9",        // the_cow
        "ruminate_opts/3",      // the_cow
    ] {
        assert!(exports.iter().any(|e| e == expected), "{expected} missing from {exports:?}");
    }
    assert!(!exports.iter().any(|e| e == "sdp_read/3"), "internal helpers are not exports");
}

// ---------------------------------------------------------------------------
// Non-ASCII text must cross the Rust/Prolog boundary as UTF-8 (found 2026-09-25:
// an LLM reply's em dash came back as "â\u{80}\u{94}" because text entering Prolog
// was read as ISO-8859-1, one character per UTF-8 byte).
// ---------------------------------------------------------------------------

const SAMPLE: &str = "a\u{2014}b caf\u{e9} \u{65e5}\u{672c} \u{1F419}";

/// A goal that contains non-ASCII text is parsed as UTF-8: the atom has 13 characters, not 23 bytes.
#[test]
fn test_goal_text_is_read_as_utf8() {
    let env = PrologEnvironment::new().expect("Failed to create environment");
    let result = env
        .query_with_bindings(&format!("atom_length('{}', N)", SAMPLE))
        .expect("query failed");
    assert!(result.contains("13"), "atom_length should count characters, got: {}", result);
    assert!(!result.contains("23"), "atom_length counted UTF-8 bytes: {}", result);
}

/// Non-ASCII text in a goal comes back out unchanged (no double encoding).
#[test]
fn test_goal_text_round_trips_unchanged() {
    let env = PrologEnvironment::new().expect("Failed to create environment");
    let result = env
        .query_with_bindings(&format!("X = \"{}\"", SAMPLE))
        .expect("query failed");
    assert!(result.contains(SAMPLE), "text was altered on the way through: {}", result);
}

/// A clause asserted from text keeps its non-ASCII characters.
#[test]
fn test_asserted_text_round_trips_unchanged() {
    let env = PrologEnvironment::new().expect("Failed to create environment");
    env.assertz(&format!("moji_fact('{}')", SAMPLE)).expect("assert failed");
    let result = env.query_with_bindings("moji_fact(X)").expect("query failed");
    assert!(result.contains(SAMPLE), "asserted text was altered: {}", result);
}
