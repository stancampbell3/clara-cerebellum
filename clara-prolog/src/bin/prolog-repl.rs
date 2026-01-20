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

        // Execute the goal
        match env.query(goal) {
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
                                    println!("{}", serde_json::to_string_pretty(solution).unwrap());
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
