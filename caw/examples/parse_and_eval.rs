/// Example of parsing and evaluating a simple CAW program
use caw::{CawParser, Runtime};

fn main() -> caw::CawResult<()> {
    println!("=== CAW Language Demo ===\n");

    // Simple CAW program
    let program_text = r#"
type Particle = {
  type: String,
  state: String
}

feather radium: Particle = {
  type: "radium",
  state: "unstable"
}

let albert = Expert(Physics.Nuclear._)
    "#;

    println!("Parsing CAW program...\n");
    println!("Source:\n{}\n", program_text);

    // Parse the program
    match CawParser::parse_program(program_text) {
        Ok(program) => {
            println!("✓ Parsed successfully!\n");
            println!("Program structure:");
            println!("  Statements: {}\n", program.statements.len());

            // Execute the program
            println!("Executing program...\n");
            let mut runtime = Runtime::new();
            match runtime.execute_program(&program) {
                Ok(result) => {
                    println!("✓ Execution successful!\n");
                    match serde_json::to_string_pretty(&result) {
                        Ok(result_str) => println!("Result: {}\n", result_str),
                        Err(e) => println!("Result (serialization issue): {}\n", e),
                    }

                    // Print runtime state
                    println!("Runtime State:");
                    println!("  Facts: {}", runtime.facts().len());
                    for fact in runtime.facts() {
                        println!("    - {}: {}", fact.name, fact.data);
                    }

                    println!("  Rules: {}", runtime.rules().len());
                    for rule in runtime.rules() {
                        println!("    - {}", rule.name);
                    }

                    println!("  Agents: {}", runtime.agents().len());
                    for (name, agent) in runtime.agents() {
                        println!("    - {}: {}", name, agent.domain);
                    }
                }
                Err(e) => {
                    eprintln!("✗ Execution failed: {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("✗ Parse error: {}", e);
        }
    }

    Ok(())
}
