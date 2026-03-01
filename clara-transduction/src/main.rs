//! Prolog → CLIPS transduction CLI.
//!
//! Usage: transduction <input.pl> [output.clp]
//!
//! Reads Prolog source from `<input.pl>`, generates CLIPS defrules via
//! speculative forward-chaining transduction, and writes the result to
//! `<output.clp>` (or stdout if omitted). Exits with code 1 on error.

use std::process;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: transduction <input.pl> [output.clp]");
        process::exit(1);
    }

    let input_path = &args[1];
    let source = match std::fs::read_to_string(input_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading '{}': {}", input_path, e);
            process::exit(1);
        }
    };

    let clp = clara_cycle::transduction::transduce_source(&source);

    if args.len() >= 3 {
        let output_path = &args[2];
        if let Err(e) = std::fs::write(output_path, &clp) {
            eprintln!("Error writing '{}': {}", output_path, e);
            process::exit(1);
        }
        eprintln!("Wrote '{}'", output_path);
    } else {
        print!("{}", clp);
    }
}
