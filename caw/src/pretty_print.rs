/// Pretty printing utilities for REPL output
use colored::*;
use serde_json::Value;
use crate::{Literal, Expression};

/// Pretty print a literal value
pub fn print_literal(lit: &Literal) -> String {
    match lit {
        Literal::String(s) => format!("\"{}\"", s).green().to_string(),
        Literal::Number(n) => n.to_string().cyan().to_string(),
        Literal::Boolean(b) => b.to_string().yellow().to_string(),
    }
}

/// Pretty print an expression
pub fn print_expression(expr: &Expression) -> String {
    match expr {
        Expression::Literal(lit) => print_literal(lit),
        Expression::Identifier(id) => id.magenta().to_string(),
        Expression::FunctionCall(fc) => {
            let args = fc.args.iter()
                .map(print_expression)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({})", fc.name.magenta(), args)
        }
        Expression::Record(rec) => {
            let fields = rec.fields.iter()
                .map(|(k, v)| format!("{}: {}", k.magenta(), print_expression(v)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{ {} }}", fields)
        }
        Expression::AgentCall(ac) => {
            let args = ac.args.iter()
                .map(print_expression)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}.{}({})", ac.agent.magenta(), ac.method.magenta(), args)
        }
        Expression::MessageSend(lhs, rhs) => {
            format!("{} {} {}", print_expression(lhs), "!".red(), print_expression(rhs))
        }
    }
}

/// Format a fact nicely
pub fn format_fact(name: &str, data: &Value) -> String {
    format!("{}: {}", name.cyan(), serde_json::to_string_pretty(data).unwrap_or_default())
}

/// Print success message
pub fn success(msg: &str) -> String {
    format!("{} {}", "✓".green(), msg)
}

/// Print error message
pub fn error(msg: &str) -> String {
    format!("{} {}", "✗".red(), msg)
}

/// Print info message
pub fn info(msg: &str) -> String {
    format!("{} {}", "ℹ".blue(), msg)
}

/// Print help text
pub fn print_help() {
    println!("{}", "CAW Language REPL - Help".bold());
    println!();
    println!("{}", "Commands:".bold().underline());
    println!("  {}     - Show this help message", ":help".cyan());
    println!("  {}    - List all facts", ":facts".cyan());
    println!("  {}    - List all rules", ":rules".cyan());
    println!("  {}   - List all agents", ":agents".cyan());
    println!("  {}   - Clear session state", ":clear".cyan());
    println!("  {}  - Export current state to CLIPS", ":export".cyan());
    println!("  {}    - Exit the REPL", ":exit".cyan());
    println!();
    println!("{}", "Statements:".bold().underline());
    println!("  {}                          - Declare a type", "type Name = <type>".cyan());
    println!("  {}                   - Declare a fact", "feather name: Type = {{...}}".cyan());
    println!("  {}         - Declare a rule", "rune \"name\" when ... then ...".cyan());
    println!("  {}                  - Create an agent", "let name = Expert(domain._)".cyan());
    println!();
    println!("{}", "Examples:".bold().underline());
    println!("  caw> type Person = {{ name: String, age: Number }}");
    println!("  caw> feather alice: Person = {{ name: \"Alice\", age: 30 }}");
    println!("  caw> let expert = Expert(AI.Knowledge._)");
    println!("  caw> :facts");
}

/// Format a duration nicely
pub fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else {
        format!("{:.2}s", ms as f64 / 1000.0)
    }
}

/// Pretty print a section header
pub fn section(title: &str) -> String {
    format!("\n{}", title.bold().underline())
}

/// Print metrics for execution
pub fn print_metrics(facts_count: usize, rules_count: usize, agents_count: usize, elapsed_ms: u64) {
    println!("{}", section("Metrics"));
    println!("  Facts:  {}", facts_count.to_string().cyan());
    println!("  Rules:  {}", rules_count.to_string().cyan());
    println!("  Agents: {}", agents_count.to_string().cyan());
    println!("  Time:   {}", format_duration(elapsed_ms).yellow());
}
