/// REPL (Read-Eval-Print-Loop) for the CAW language
use crate::{CawParser, Runtime, Statement, CawResult, ClipsTranspiler};
use std::time::Instant;

/// REPL session state
pub struct ReplSession {
    runtime: Runtime,
    statement_count: usize,
}

impl ReplSession {
    /// Create a new REPL session
    pub fn new() -> Self {
        Self {
            runtime: Runtime::new(),
            statement_count: 0,
        }
    }

    /// Execute a single statement
    pub fn execute_statement(&mut self, input: &str) -> CawResult<String> {
        let start = Instant::now();

        // Parse the statement
        let statement = CawParser::parse_statement_interactive(input)?;

        // Execute it
        self.runtime.execute_program(&crate::Program {
            statements: vec![statement.clone()],
        })?;

        self.statement_count += 1;
        let elapsed = start.elapsed().as_millis() as u64;

        // Generate output based on statement type
        let output = match statement {
            Statement::TypeDecl(td) => {
                format!("✓ Type '{}' declared", td.name.bright_cyan())
            }
            Statement::FeatherDecl(fd) => {
                format!("✓ Fact '{}' created ({})", fd.name.bright_cyan(), fd.type_name.bright_white())
            }
            Statement::AgentDecl(ad) => {
                format!("✓ Agent '{}' created at domain {}", ad.name.bright_cyan(), ad.domain.to_string().bright_white())
            }
            Statement::RuneDecl(rd) => {
                format!("✓ Rule '{}' defined", rd.name.bright_cyan())
            }
            Statement::Expression(_) => {
                format!("✓ Expression evaluated")
            }
        };

        Ok(format!("{} [{}ms]", output, elapsed))
    }

    /// Get number of statements executed
    pub fn statement_count(&self) -> usize {
        self.statement_count
    }

    /// Get runtime reference
    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// Get runtime mutable reference
    pub fn runtime_mut(&mut self) -> &mut Runtime {
        &mut self.runtime
    }

    /// Export current state to CLIPS
    pub fn export_to_clips(&self) -> String {
        let _transpiler = ClipsTranspiler::new();

        let mut output = String::new();
        output.push_str("; Generated CAW program exported to CLIPS\n");
        output.push_str("; CAW REPL Session Export\n\n");

        // Export all types and facts as a program
        // For now, just provide basic structure
        output.push_str("(batch *)\n");
        output.push_str("; TODO: Add facts and rules from session\n");

        output
    }
}

impl Default for ReplSession {
    fn default() -> Self {
        Self::new()
    }
}

use colored::*;

/// REPL command type
pub enum ReplCommand {
    Help,
    Facts,
    Rules,
    Agents,
    Clear,
    Export,
    Exit,
    Statement(String),
}

impl ReplCommand {
    /// Parse a REPL command from user input
    pub fn parse(input: &str) -> Option<Self> {
        let trimmed = input.trim();

        if trimmed.is_empty() {
            return None;
        }

        match trimmed {
            ":help" => Some(ReplCommand::Help),
            ":facts" => Some(ReplCommand::Facts),
            ":rules" => Some(ReplCommand::Rules),
            ":agents" => Some(ReplCommand::Agents),
            ":clear" => Some(ReplCommand::Clear),
            ":export" => Some(ReplCommand::Export),
            ":exit" => Some(ReplCommand::Exit),
            _ if trimmed.starts_with(':') => {
                eprintln!("{} Unknown command: {}", "✗".red(), trimmed);
                None
            }
            _ => Some(ReplCommand::Statement(trimmed.to_string())),
        }
    }
}

/// Execute a REPL command
pub fn execute_command(session: &mut ReplSession, cmd: ReplCommand) -> Result<bool, String> {
    match cmd {
        ReplCommand::Help => {
            print_help();
            Ok(true)
        }
        ReplCommand::Facts => {
            print_facts(session);
            Ok(true)
        }
        ReplCommand::Rules => {
            print_rules(session);
            Ok(true)
        }
        ReplCommand::Agents => {
            print_agents(session);
            Ok(true)
        }
        ReplCommand::Clear => {
            println!("{}", "✓ Session cleared".green());
            *session = ReplSession::new();
            Ok(true)
        }
        ReplCommand::Export => {
            let clips_code = session.export_to_clips();
            println!("{}", clips_code);
            Ok(true)
        }
        ReplCommand::Exit => {
            Ok(false)
        }
        ReplCommand::Statement(input) => {
            match session.execute_statement(&input) {
                Ok(output) => {
                    println!("{}", output);
                    Ok(true)
                }
                Err(e) => {
                    Err(format!("{}", e))
                }
            }
        }
    }
}

fn print_help() {
    println!("{}", "\n--- CAW Language REPL Help ---".bold());
    println!();
    println!("{}", "Built-in Commands:".bold().underline());
    println!("  {}  - Show this help message", ":help".cyan());
    println!("  {}  - List all facts in the session", ":facts".cyan());
    println!("  {}  - List all rules in the session", ":rules".cyan());
    println!("  {} - List all agents in the session", ":agents".cyan());
    println!("  {} - Clear the session state", ":clear".cyan());
    println!("  {}- Export current state to CLIPS", ":export".cyan());
    println!("  {}  - Exit the REPL", ":exit".cyan());
    println!();
    println!("{}", "CAW Statements:".bold().underline());
    println!("  {:<45} - Define a type", "type Name = {{ field: Type }}".cyan());
    println!("  {:<45} - Declare a fact", "feather name: Type = {{ ... }}".cyan());
    println!("  {:<45} - Define a rule", "rune \"name\" when ... then ...".cyan());
    println!("  {:<45} - Create an agent", "let name = Expert(domain._)".cyan());
    println!();
    println!("{}", "Examples:".bold().underline());
    println!("  caw> type Person = {{ name: String, age: Number }}");
    println!("  caw> feather alice: Person = {{ name: \"Alice\", age: 30 }}");
    println!("  caw> let expert = Expert(AI.Knowledge._)");
    println!("  caw> :facts");
    println!();
}

fn print_facts(session: &ReplSession) {
    let facts = session.runtime().facts();
    if facts.is_empty() {
        println!("{}", "No facts defined".dimmed());
    } else {
        println!("{}", "\n--- Facts ---".bold().underline());
        for fact in facts {
            println!("  {}: {}", fact.name.cyan(), fact.data);
        }
        println!();
    }
}

fn print_rules(session: &ReplSession) {
    let rules = session.runtime().rules();
    if rules.is_empty() {
        println!("{}", "No rules defined".dimmed());
    } else {
        println!("{}", "\n--- Rules ---".bold().underline());
        for rule in rules {
            println!("  {}", rule.name.cyan());
        }
        println!();
    }
}

fn print_agents(session: &ReplSession) {
    let agents = session.runtime().agents();
    if agents.is_empty() {
        println!("{}", "No agents defined".dimmed());
    } else {
        println!("{}", "\n--- Agents ---".bold().underline());
        for (name, agent) in agents {
            println!("  {}: {}", name.cyan(), agent.domain);
        }
        println!();
    }
}
