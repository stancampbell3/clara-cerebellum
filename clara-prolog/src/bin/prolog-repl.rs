//! Simple REPL for testing the Prolog integration

use clara_prolog::PrologEnvironment;
use clara_toolbox::{ClaraSplinteredMindTool, ToolboxManager};
use std::io::{self, BufRead, Write};
use std::sync::Arc;

fn main() {
    env_logger::init();

    println!("Clara-Prolog REPL (LilDevils)");
    println!("Type Prolog goals to execute, or 'quit' to exit.");
    println!("--------------------------------------------");

    // Initialize the global ToolboxManager with default tools (echo, etc.)
    ToolboxManager::init_global();

    // Register the SplinteredMind tool for FieryPit API access
    let fierypit_url =
        std::env::var("FIERYPIT_URL").unwrap_or_else(|_| "http://localhost:6666".to_string());
    {
        let mut manager = ToolboxManager::global().lock().unwrap();
        manager.register_tool(Arc::new(ClaraSplinteredMindTool::with_url(&fierypit_url)));
    }
    println!("SplinteredMind tool registered (FieryPit: {})", fierypit_url);

    // Initialize Prolog
    clara_prolog::init_global();

    // Create an environment for the REPL session
    let env = match PrologEnvironment::new() {
        Ok(env) => env,
        Err(e) => {
            eprintln!("Failed to create Prolog environment: {}", e);
            std::process::exit(1);
        }
    };

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("?- ");
        stdout.flush().unwrap();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error reading input: {}", e);
                continue;
            }
        }

        let goal = line.trim();

        if goal.is_empty() {
            continue;
        }

        if goal == "quit" || goal == "halt" || goal == "exit" {
            println!("Goodbye!");
            break;
        }

        // Handle special commands
        if goal.starts_with("consult(") || goal.starts_with("['") {
            // File loading
            match env.query_once(goal) {
                Ok(_) => println!("true."),
                Err(e) => println!("Error: {}", e),
            }
            continue;
        }

        // Execute the goal with variable bindings for REPL-style output
        match env.query_with_bindings(goal) {
            Ok(result) => {
                // Parse and pretty-print the JSON result
                match serde_json::from_str::<serde_json::Value>(&result) {
                    Ok(json) => {
                        if let Some(arr) = json.as_array() {
                            if arr.is_empty() {
                                println!("false.");
                            } else {
                                for (i, solution) in arr.iter().enumerate() {
                                    if i > 0 {
                                        println!(" ;");
                                    }
                                    // Format variable bindings nicely
                                    if let Some(obj) = solution.as_object() {
                                        if obj.is_empty() {
                                            print!("true");
                                        } else {
                                            let bindings: Vec<String> = obj
                                                .iter()
                                                .map(|(name, val)| {
                                                    format!("{} = {}", name, format_value(val))
                                                })
                                                .collect();
                                            print!("{}", bindings.join(",\n"));
                                        }
                                    } else if solution.as_bool() == Some(true) {
                                        print!("true");
                                    } else {
                                        print!("{}", solution);
                                    }
                                }
                                println!(".");
                            }
                        } else {
                            println!("{}", serde_json::to_string_pretty(&json).unwrap());
                        }
                    }
                    Err(_) => {
                        println!("{}", result);
                    }
                }
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
    }
}

/// Format a JSON value for Prolog-style output
fn format_value(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "_".to_string(),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(format_value).collect();
            format!("[{}]", items.join(", "))
        }
        serde_json::Value::Object(obj) => {
            // Check if it's a functor representation
            if let (Some(functor), Some(args)) = (obj.get("functor"), obj.get("args")) {
                if let (Some(f), Some(a)) = (functor.as_str(), args.as_array()) {
                    let formatted_args: Vec<String> = a.iter().map(format_value).collect();
                    return format!("{}({})", f, formatted_args.join(", "));
                }
            }
            // Otherwise format as Prolog-style key-value pairs
            let items: Vec<String> = obj
                .iter()
                .map(|(k, v)| format!("{}-{}", k, format_value(v)))
                .collect();
            format!("[{}]", items.join(", "))
        }
    }
}
