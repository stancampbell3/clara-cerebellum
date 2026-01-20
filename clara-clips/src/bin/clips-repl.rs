// CLIPS REPL with full Rust callback support

use clara_clips::ClipsEnvironment;
use clara_toolbox::{ClaraSplinteredMindTool, EvaluateTool, ToolboxManager};
use demonic_voice::DemonicVoice;
use std::env;
use std::io::{self, Write};
use std::sync::Arc;

fn main() {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let default_evaluator = if args.len() > 1 && args[1] == "--evaluator" && args.len() > 2 {
        args[2].clone()
    } else if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        println!("Clara-CLIPS REPL");
        println!();
        println!("Usage: clips-repl [--evaluator TOOL]");
        println!();
        println!("Options:");
        println!("  --evaluator TOOL    Set default evaluator tool (default: evaluate)");
        println!("                      Use 'echo' for testing without network calls");
        println!("  --help, -h          Show this help message");
        println!();
        println!("Examples:");
        println!("  clips-repl                      # Use 'evaluate' (DemonicVoice)");
        println!("  clips-repl --evaluator echo     # Use 'echo' for testing");
        std::process::exit(0);
    } else {
        "evaluate".to_string()
    };

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

    // Register the EvaluateTool with a DemonicVoice client
    let fierypit_url =
        env::var("FIERYPIT_URL").unwrap_or_else(|_| "http://localhost:6666".to_string());
    {
        let mut manager = ToolboxManager::global().lock().unwrap();
        let daemon_voice = Arc::new(DemonicVoice::new(&fierypit_url));
        manager.register_tool(Arc::new(EvaluateTool::new(daemon_voice)));

        // Register SplinteredMind tool for FieryPit API access
        manager.register_tool(Arc::new(ClaraSplinteredMindTool::with_url(&fierypit_url)));

        // Set the default evaluator
        manager.set_default_evaluator(&default_evaluator);
    }
    println!("FieryPit URL: {}", fierypit_url);

    let manager = ToolboxManager::global().lock().unwrap();
    let tools = manager.list_tools();
    let current_default = manager.get_default_evaluator();
    println!("Registered tools: {}", tools.join(", "));
    println!("Default evaluator: {}", current_default);
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
    println!("Evaluation Callback:");
    println!("  (clara-evaluate JSON-STRING)");
    println!();
    println!("  Two modes:");
    println!("  1. Simple (uses default evaluator):");
    println!("     {{\"question\":\"what is 2+2?\"}}");
    println!();
    println!("  2. Explicit tool selection:");
    println!("     {{\"tool\":\"TOOL_NAME\",\"arguments\":{{...}}}}");
    println!();
    println!("Examples:");
    println!("  ; Use default evaluator (DemonicVoice at localhost:8000)");
    println!("  (clara-evaluate \"{{\\\"question\\\":\\\"what is the weather?\\\"}}\")");
    println!();
    println!("  ; Explicitly use echo tool (for testing)");
    println!("  (clara-evaluate \"{{\\\"tool\\\":\\\"echo\\\",\\\"arguments\\\":{{\\\"message\\\":\\\"hello\\\"}}}}\")");
    println!();
    println!("  ; Basic CLIPS");
    println!("  (+ 1 2)");
    println!("  (printout t \"Hello, World!\" crlf)");
    println!("  (assert (fact value))");
    println!("  (run)");
    println!();
}
