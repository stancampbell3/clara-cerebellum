//! Prolog → CLIPS transduction CLI.
//!
//! Usage: transduction [--decorate] <input.pl> [output.clp]
//!
//! Without `--decorate`:
//!   Reads Prolog source from `<input.pl>`, generates CLIPS defrules, and writes
//!   the result to `<output.clp>` (or stdout if omitted).
//!
//! With `--decorate`:
//!   Parses `<input.pl>` once, then writes two output files derived from the
//!   input stem (e.g. `rules.pl` → `rules_clara.pl` + `rules_clara.clp`):
//!     - `<stem>_clara.pl`  — Prolog rules decorated with `coire_publish_assert(Head)`
//!     - `<stem>_clara.clp` — CLIPS defrules for speculative forward chaining
//!   Stdout is not used when `--decorate` is active.
//!
//! Exits with code 1 on any I/O or argument error.

use std::path::Path;
use std::process;

use clara_cycle::transduction::{decorate_rules, parse_prolog_rules, transduce};

fn main() {
    let raw: Vec<String> = std::env::args().collect();

    let decorate = raw.iter().skip(1).any(|a| a == "--decorate");
    let positional: Vec<&str> = raw.iter().skip(1)
        .filter(|a| !a.starts_with("--"))
        .map(|a| a.as_str())
        .collect();

    if positional.is_empty() {
        eprintln!("Usage: transduction [--decorate] <input.pl> [output.clp]");
        eprintln!();
        eprintln!("  --decorate  Write decorated Prolog and CLIPS as <stem>_clara.{{pl,clp}}");
        eprintln!("              Output files go beside the input file; stdout is not used.");
        process::exit(1);
    }

    let input_path = positional[0];
    let source = match std::fs::read_to_string(input_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading '{}': {}", input_path, e);
            process::exit(1);
        }
    };

    if decorate {
        let path = Path::new(input_path);
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        let stem = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");

        let pl_out  = dir.join(format!("{}_clara.pl",  stem));
        let clp_out = dir.join(format!("{}_clara.clp", stem));

        // Single parse pass shared by both outputs.
        let rules = parse_prolog_rules(&source);
        let decorated_pl = decorate_rules(&rules);
        let clp = transduce(&rules);

        write_file(&pl_out.to_string_lossy(), &decorated_pl);
        write_file(&clp_out.to_string_lossy(), &clp);
    } else {
        let clp = clara_cycle::transduction::transduce_source(&source);

        if positional.len() >= 2 {
            write_file(positional[1], &clp);
        } else {
            print!("{}", clp);
        }
    }
}

fn write_file(path: &str, content: &str) {
    if let Err(e) = std::fs::write(path, content) {
        eprintln!("Error writing '{}': {}", path, e);
        process::exit(1);
    }
    eprintln!("Wrote '{}'", path);
}
