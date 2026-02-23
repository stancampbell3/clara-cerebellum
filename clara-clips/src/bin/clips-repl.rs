// CLIPS REPL with full Rust callback support

use clara_clips::ClipsEnvironment;
use clara_toolbox::{ClaraSplinteredMindTool, EvaluateTool, ToolboxManager};
use demonic_voice::DemonicVoice;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::env;
use std::sync::Arc;

/// Load .env file from current directory if present, setting any vars not already in env
fn load_dotenv() {
    let path = std::path::Path::new(".env");
    if let Ok(contents) = std::fs::read_to_string(path) {
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"');
                if std::env::var(key).is_err() {
                    std::env::set_var(key, value);
                }
            }
        }
    }
}

/// Count net open parentheses in a line, ignoring string literals and ; comments.
fn paren_depth(s: &str) -> i32 {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            if c == '\\' {
                chars.next(); // skip escaped char
            } else if c == '"' {
                in_string = false;
            }
        } else {
            match c {
                '"' => in_string = true,
                ';' => break, // rest of line is a comment
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
        }
    }
    depth
}

/// Returns true if the input is a CLIPS construct definition that needs Build(), not Eval().
fn is_construct(s: &str) -> bool {
    let s = s.trim();
    if !s.starts_with('(') {
        return false;
    }
    let keyword = s[1..].trim_start().split_whitespace().next().unwrap_or("");
    matches!(
        keyword,
        "defrule"
            | "deftemplate"
            | "deffacts"
            | "defglobal"
            | "deffunction"
            | "defclass"
            | "defmessage-handler"
            | "defgeneric"
            | "defmethod"
            | "defmodule"
            | "definstances"
    )
}

fn main() {
    load_dotenv();

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

    println!("Clara-CLIPS REPL");
    println!("================");
    println!();

    // Initialize global Coire (shared event mailbox)
    println!("Initializing Coire...");
    clara_coire::init_global().expect("Failed to initialize Coire");

    // Initialize the global ToolboxManager
    println!("Initializing ToolboxManager...");
    ToolboxManager::init_global();

    // Register tools
    let fierypit_url =
        env::var("FIERYPIT_URL").unwrap_or_else(|_| "http://localhost:6666".to_string());
    {
        let mut manager = ToolboxManager::global().lock().unwrap();
        let daemon_voice = Arc::new(DemonicVoice::new(&fierypit_url));
        manager.register_tool(Arc::new(EvaluateTool::new(daemon_voice)));
        manager.register_tool(Arc::new(ClaraSplinteredMindTool::with_url(&fierypit_url)));
        manager.set_default_evaluator(&default_evaluator);
    }
    println!("FieryPit URL: {}", fierypit_url);

    {
        let manager = ToolboxManager::global().lock().unwrap();
        println!("Registered tools: {}", manager.list_tools().join(", "));
        println!("Default evaluator: {}", manager.get_default_evaluator());
    }

    println!();
    println!("Creating CLIPS environment...");

    let mut env = match ClipsEnvironment::new() {
        Ok(env) => env,
        Err(e) => {
            eprintln!("Failed to create CLIPS environment: {}", e);
            std::process::exit(1);
        }
    };

    println!("CLIPS environment ready!");
    println!("Type CLIPS expressions. 'help' for commands, Ctrl+D or (exit) to quit.");
    println!();

    let mut rl = DefaultEditor::new().expect("Failed to create line editor");

    let history_path = dirs::home_dir()
        .map(|h| h.join(".clara-clips-history"))
        .unwrap_or_else(|| ".clara-clips-history".into());
    let _ = rl.load_history(&history_path);

    'outer: loop {
        // Read the first (or only) line of a command
        let first_line = match rl.readline("CLIPS> ") {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                continue;
            }
            Err(ReadlineError::Eof) => {
                println!("Goodbye!");
                break;
            }
            Err(e) => {
                eprintln!("Error reading input: {}", e);
                continue;
            }
        };

        let trimmed = first_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed == "(exit)" || trimmed == "exit" || trimmed == "quit" {
            println!("Goodbye!");
            break;
        }

        if trimmed == "help" || trimmed == "(help)" {
            print_help();
            continue;
        }

        if trimmed == "tools" || trimmed == "(tools)" {
            let manager = ToolboxManager::global().lock().unwrap();
            println!("Available tools:");
            for tool in manager.list_tools() {
                println!("  - {}", tool);
            }
            continue;
        }

        // Accumulate lines until parentheses are balanced (supports multi-line paste)
        let mut full_input = trimmed.to_string();
        let mut depth = paren_depth(trimmed);

        while depth > 0 {
            match rl.readline("    -> ") {
                Ok(cont) => {
                    depth += paren_depth(&cont);
                    full_input.push('\n');
                    full_input.push_str(cont.trim_end());
                }
                Err(ReadlineError::Interrupted) => {
                    println!("^C");
                    continue 'outer;
                }
                Err(ReadlineError::Eof) => {
                    println!("Goodbye!");
                    break 'outer;
                }
                Err(e) => {
                    eprintln!("Error reading input: {}", e);
                    continue 'outer;
                }
            }
        }

        let _ = rl.add_history_entry(&full_input);

        // Route constructs to Build(), everything else to Eval()
        if is_construct(&full_input) {
            match env.build(&full_input) {
                Ok(()) => {} // CLIPS prints its own confirmation for constructs
                Err(e) => eprintln!("Error: {}", e),
            }
        } else {
            match env.eval(&full_input) {
                Ok(result) => {
                    if !result.trim().is_empty() {
                        println!("{}", result);
                    }
                }
                Err(e) => eprintln!("Error: {}", e),
            }
        }
    }

    let _ = rl.save_history(&history_path);
}

fn print_help() {
    println!("Clara-CLIPS REPL Help");
    println!("====================");
    println!();
    println!("Special Commands:");
    println!("  help, (help)           Show this help message");
    println!("  tools, (tools)         List available callback tools");
    println!("  (exit), exit, quit     Exit the REPL");
    println!();
    println!("CLIPS built-ins work as usual:");
    println!("  (reset)                Reset the CLIPS environment");
    println!("  (clear)                Clear the CLIPS environment");
    println!("  (run)                  Run the agenda");
    println!("  (assert (fact value))  Assert a fact");
    println!();
    println!("Construct definitions route automatically to Build():");
    println!("  (defrule ...)          Define a rule");
    println!("  (deftemplate ...)      Define a template");
    println!("  (deffacts ...)         Define initial facts");
    println!("  (defglobal ...)        Define a global variable");
    println!("  (deffunction ...)      Define a function");
    println!();
    println!("Multi-line input is supported — paste freely.");
    println!("Unbalanced ( triggers a continuation prompt '    -> '.");
    println!();
    println!("Evaluation Callback:");
    println!("  (clara-evaluate JSON-STRING)");
    println!();
    println!("  Simple (uses default evaluator):");
    println!("    (clara-evaluate \"{{\\\"question\\\":\\\"what is 2+2?\\\"}}\")");
    println!();
    println!("  Explicit tool selection:");
    println!("    (clara-evaluate \"{{\\\"tool\\\":\\\"echo\\\",\\\"arguments\\\":{{\\\"message\\\":\\\"hello\\\"}}}}\")");
    println!();
}
