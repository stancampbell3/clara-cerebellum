//! Prolog → CLIPS + DOT transduction CLI.
//!
//! Usage: transduction <input.pl>
//!        transduction --graph <ritual-graph.json>
//!
//! Parses `<input.pl>` and writes three output files derived from the input
//! stem (e.g. `rules.pl` → `rules_clara.pl`, `rules_clara.clp`, `rules_clara.dot`):
//!
//!   - `<stem>_clara.pl`  — Original Prolog source prepended with:
//!       - `:- prolog_listen(...)` directives for every `dynamic` predicate
//!       - The `updated/3` relay rule (publishes asserted facts to CLIPS)
//!       - Comment delimiters marking the generated block
//!   - `<stem>_clara.clp` — CLIPS defrules for speculative forward chaining
//!   - `<stem>_clara.dot` — Graphviz DOT graph showing facts, rule heads,
//!       conditions, and their chaining relationships
//!
//! With `--graph`, the input is a Cobbler `graph_layout` JSON and the edges
//! are transduced into per-node snippets (`<stem>_<node>_edges.pl` /
//! `<stem>_<node>_edges.clp` beside the input) — see
//! `clara_cycle::transduction::transduce_graph`.
//!
//! Stdout is not used. Exits with code 1 on any I/O or argument error.

use std::path::Path;
use std::process;

use clara_cycle::transduction::{decorate_source, generate_dot, parse_prolog_rules, transduce, transduce_graph, DotOptions};

fn main() {
    let raw: Vec<String> = std::env::args().collect();

    let graph_mode = raw.iter().any(|a| a == "--graph");
    let positional: Vec<&str> = raw.iter().skip(1)
        .filter(|a| !a.starts_with("--"))
        .map(|a| a.as_str())
        .collect();

    if positional.is_empty() {
        eprintln!("Usage: transduction <input.pl>");
        eprintln!("       transduction --graph <ritual-graph.json>");
        eprintln!();
        eprintln!("  Writes <stem>_clara.pl, <stem>_clara.clp, and <stem>_clara.dot");
        eprintln!("  beside the input file. With --graph, writes per-node");
        eprintln!("  <stem>_<node>_edges.pl/.clp edge snippets instead.");
        process::exit(1);
    }

    if graph_mode {
        run_graph_mode(positional[0]);
        return;
    }

    let input_path = positional[0];
    let source = match std::fs::read_to_string(input_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading '{}': {}", input_path, e);
            process::exit(1);
        }
    };

    let path = Path::new(input_path);
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let pl_out  = dir.join(format!("{}_clara.pl",  stem));
    let clp_out = dir.join(format!("{}_clara.clp", stem));
    let dot_out = dir.join(format!("{}_clara.dot", stem));

    // Single parse pass shared by all outputs.
    let rules = parse_prolog_rules(&source);
    let decorated_pl = decorate_source(&source);
    let clp = transduce(&rules);
    let dot = generate_dot(&rules, None, &DotOptions::default());

    write_file(&pl_out.to_string_lossy(), &decorated_pl);
    write_file(&clp_out.to_string_lossy(), &clp);
    write_file(&dot_out.to_string_lossy(), &dot);
}

fn run_graph_mode(input_path: &str) {
    let source = match std::fs::read_to_string(input_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading '{}': {}", input_path, e);
            process::exit(1);
        }
    };

    let result = match transduce_graph(&source) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error transducing graph '{}': {}", input_path, e);
            process::exit(1);
        }
    };

    let path = Path::new(input_path);
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("graph");

    if result.per_node.is_empty() {
        eprintln!("No evaluator-to-evaluator edges found in '{}'", input_path);
        return;
    }

    let mut node_ids: Vec<&String> = result.per_node.keys().collect();
    node_ids.sort();
    for node_id in node_ids {
        let snippets = &result.per_node[node_id];
        if !snippets.prolog.is_empty() {
            let out = dir.join(format!("{}_{}_edges.pl", stem, node_id));
            write_file(&out.to_string_lossy(), &snippets.prolog);
        }
        if !snippets.clips.is_empty() {
            let out = dir.join(format!("{}_{}_edges.clp", stem, node_id));
            write_file(&out.to_string_lossy(), &snippets.clips);
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
