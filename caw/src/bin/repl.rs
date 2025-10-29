/// CAW Language REPL Binary
/// Interactive shell for the CAW language

use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use colored::*;
use caw::{ReplSession, ReplCommand, repl};

fn main() {
    println!("{}", "\n╔═══════════════════════════════════════════╗".bright_cyan());
    println!("{}", "║  CAW Language REPL v0.1.0                ║".bright_cyan());
    println!("{}", "║  Type :help for commands, :exit to quit   ║".bright_cyan());
    println!("{}", "╚═══════════════════════════════════════════╝".bright_cyan());
    println!();

    // Initialize session
    let mut session = ReplSession::new();

    // Try to load history from file
    let history_file = format!("{}/.caw_history", std::env::var("HOME").unwrap_or_else(|_| ".".to_string()));

    let mut rl = match DefaultEditor::new() {
        Ok(editor) => editor,
        Err(e) => {
            eprintln!("{} Failed to initialize readline: {}", "✗".red(), e);
            return;
        }
    };

    // Try to load history
    let _ = rl.load_history(&history_file);

    loop {
        // Build prompt showing session stats
        let facts_count = session.runtime().facts().len();
        let rules_count = session.runtime().rules().len();
        let agents_count = session.runtime().agents().len();

        let prompt = format!(
            "{} {} {} {} > ",
            "caw".bright_magenta(),
            format!("[f:{}]", facts_count).cyan(),
            format!("[r:{}]", rules_count).cyan(),
            format!("[a:{}]", agents_count).cyan()
        );

        // Read input
        let readline = rl.readline(&prompt);
        match readline {
            Ok(line) => {
                let trimmed = line.trim();

                // Skip empty lines
                if trimmed.is_empty() {
                    continue;
                }

                // Add to history
                let _ = rl.add_history_entry(line.as_str());

                // Parse and execute command
                match ReplCommand::parse(trimmed) {
                    Some(cmd) => {
                        match repl::execute_command(&mut session, cmd) {
                            Ok(true) => {
                                // Continue
                            }
                            Ok(false) => {
                                // Exit requested
                                break;
                            }
                            Err(e) => {
                                println!("{} {}", "✗".red(), e.red());
                            }
                        }
                    }
                    None => {
                        // Empty or unknown command
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("{}", "^C".yellow());
                continue;
            }
            Err(ReadlineError::Eof) => {
                println!();
                break;
            }
            Err(err) => {
                println!("{} {}", "✗".red(), err);
                break;
            }
        }
    }

    // Save history
    if let Err(e) = rl.save_history(&history_file) {
        eprintln!("{} Failed to save history: {}", "⚠".yellow(), e);
    }

    println!();
    println!("{}", "Goodbye! 👋".bright_cyan());
    println!();
}
