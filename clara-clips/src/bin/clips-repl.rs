// CLIPS REPL with full Rust callback support

use clara_clips::ClipsEnvironment;
use clara_toolbox::ToolboxManager;
use std::io::{self, Write};

fn main() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    println!("Clara-CLIPS REPL with Callback Support");
    println!("======================================");
    println!();

    // Initialize the global ToolboxManager
    println!("Initializing ToolboxManager...");
    ToolboxManager::init_global();

    let manager = ToolboxManager::global().lock().unwrap();
    let tools = manager.list_tools();
    println!("Registered tools: {}", tools.join(", "));
    drop(manager);

    println!();
    println!("Creating CLIPS environment...");

    // Create CLIPS environment
    let mut env = match ClipsEnvironment::new() {
        Ok(env) => env,
        Err(e) => {
            eprintln!("Failed to create CLIPS environment: {}", e);
            std::process::exit(1);
        }
    };

    println!("CLIPS environment ready!");
    println!();
    println!("Available callbacks:");
    println!("  (clara-evaluate \"{{\\\"tool\\\":\\\"echo\\\",\\\"arguments\\\":{{\\\"message\\\":\\\"hello\\\"}}}}\")");
    println!();
    println!("Type CLIPS expressions or commands. Ctrl+C or (exit) to quit.");
    println!();

    let stdin = io::stdin();
    let mut input = String::new();
    let mut line_count = 0;

    loop {
        // Print prompt
        print!("CLIPS[{}]> ", line_count);
        io::stdout().flush().unwrap();

        // Read line
        input.clear();
        match stdin.read_line(&mut input) {
            Ok(0) => {
                // EOF
                println!();
                println!("Goodbye!");
                break;
            }
            Ok(_) => {
                let trimmed = input.trim();

                // Skip empty lines
                if trimmed.is_empty() {
                    continue;
                }

                // Check for exit
                if trimmed == "(exit)" || trimmed == "exit" || trimmed == "quit" {
                    println!("Goodbye!");
                    break;
                }

                // Special commands
                if trimmed == "(reset)" {
                    match env.reset() {
                        Ok(_) => println!("Environment reset."),
                        Err(e) => eprintln!("Error resetting: {}", e),
                    }
                    continue;
                }

                if trimmed == "(clear)" {
                    match env.clear() {
                        Ok(_) => println!("Environment cleared."),
                        Err(e) => eprintln!("Error clearing: {}", e),
                    }
                    continue;
                }

                if trimmed == "help" || trimmed == "(help)" {
                    print_help();
                    continue;
                }

                if trimmed == "tools" || trimmed == "(tools)" {
                    let manager = ToolboxManager::global().lock().unwrap();
                    let tools = manager.list_tools();
                    println!("Available tools:");
                    for tool in tools {
                        println!("  - {}", tool);
                    }
                    continue;
                }

                // Evaluate the expression
                match env.eval(trimmed) {
                    Ok(result) => {
                        if !result.trim().is_empty() {
                            println!("{}", result);
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }

                line_count += 1;
            }
            Err(e) => {
                eprintln!("Error reading input: {}", e);
                break;
            }
        }
    }
}

fn print_help() {
    println!("Clara-CLIPS REPL Help");
    println!("====================");
    println!();
    println!("Special Commands:");
    println!("  help, (help)     - Show this help message");
    println!("  tools, (tools)   - List available callback tools");
    println!("  (reset)          - Reset the CLIPS environment");
    println!("  (clear)          - Clear the CLIPS environment");
    println!("  (exit), exit, quit - Exit the REPL");
    println!();
    println!("Callback Usage:");
    println!("  (clara-evaluate JSON-STRING)");
    println!();
    println!("  JSON format: {{\"tool\":\"TOOL_NAME\",\"arguments\":{{...}}}}");
    println!();
    println!("Examples:");
    println!("  ; Echo tool example");
    println!("  (clara-evaluate \"{{\\\"tool\\\":\\\"echo\\\",\\\"arguments\\\":{{\\\"message\\\":\\\"hello\\\"}}}}\")");
    println!();
    println!("  ; Basic CLIPS");
    println!("  (+ 1 2)");
    println!("  (printout t \"Hello, World!\" crlf)");
    println!("  (assert (fact value))");
    println!("  (run)");
    println!();
}
