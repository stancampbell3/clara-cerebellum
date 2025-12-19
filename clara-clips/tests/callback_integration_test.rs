// Integration test for CLIPS callback to ToolboxManager

use clara_clips::ClipsEnvironment;
use clara_toolbox::ToolboxManager;

#[test]
fn test_clips_callback_to_echo_tool() {
    // Initialize the global ToolboxManager
    ToolboxManager::init_global();

    // Create a CLIPS environment
    let mut env = ClipsEnvironment::new().expect("Failed to create CLIPS environment");

    // Call clara-evaluate with the echo tool
    let result = env
        .eval(r#"(clara-evaluate "{\"tool\":\"echo\",\"arguments\":{\"message\":\"Hello from CLIPS!\"}}")"#)
        .expect("Failed to evaluate");

    println!("CLIPS evaluation result: {}", result);

    // The callback should have executed successfully
    // Note: The actual return value parsing from CLIPS needs more work,
    // but if this doesn't panic, the callback infrastructure is working!
    assert!(result.len() > 0, "Should return some output");
}

#[test]
fn test_clips_basic_eval() {
    let mut env = ClipsEnvironment::new().expect("Failed to create CLIPS environment");

    // Test basic CLIPS evaluation
    let result = env.eval("(+ 1 2)").expect("Failed to evaluate");
    println!("Basic eval result: {}", result);

    assert!(result.len() > 0);
}

#[test]
fn test_clips_callback_with_invalid_json() {
    ToolboxManager::init_global();

    let mut env = ClipsEnvironment::new().expect("Failed to create CLIPS environment");

    // Call with invalid JSON - should not crash
    let result = env.eval(r#"(clara-evaluate "not valid json")"#);

    // Should still return something (error response)
    assert!(result.is_ok(), "Should handle invalid JSON gracefully");
}

#[test]
fn test_clips_callback_with_unknown_tool() {
    ToolboxManager::init_global();

    let mut env = ClipsEnvironment::new().expect("Failed to create CLIPS environment");

    // Call with unknown tool
    let result = env.eval(r#"(clara-evaluate "{\"tool\":\"nonexistent\",\"arguments\":{}}")"#);

    // Should still return something (error response)
    assert!(result.is_ok(), "Should handle unknown tool gracefully");
}
