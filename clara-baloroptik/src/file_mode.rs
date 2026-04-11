//! `baloroptik file` — offline mode.
//!
//! Reads a persisted deduction snapshot JSON file (the format written by
//! `clara-coire`'s snapshot store) and produces a single colorized DOT graph
//! representing the final reasoning state recorded in the snapshot's Dagda
//! tableau.
//!
//! # Snapshot encoding quirk
//!
//! `prolog_clauses` and `tableau_entries` are stored as **double-encoded**
//! JSON strings — i.e. the field value is a JSON string whose contents are
//! themselves a JSON array.  We decode each in two passes:
//!
//! ```text
//! snap.prolog_clauses  : String  →  serde_json::from_str → Vec<String>
//! snap.tableau_entries : String  →  serde_json::from_str → Vec<PredicateEntry>
//! ```

use std::path::PathBuf;

use clara_cycle::{coloring_from_entries, generate_dot, parse_prolog_rules, DotOptions, PredicateEntry, TruthValue};
use serde::Deserialize;

use crate::render::{print_file_summary, write_html, write_outputs, Format, HtmlPhase};

// ── Snapshot deserialisation ───────────────────────────────────────────────────

/// Partial representation of a deduction snapshot JSON file.
/// Fields not needed for visualization are ignored.
#[derive(Debug, Deserialize)]
struct DeductionSnapshot {
    deduction_id: String,
    /// Double-encoded JSON array of Prolog clause strings.
    prolog_clauses: String,
    initial_goal: Option<String>,
    status: String,
    cycles_run: u32,
    /// Double-encoded JSON array of `PredicateEntry` objects.
    tableau_entries: String,
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub struct FileArgs {
    pub snapshot_path: PathBuf,
    pub out_dir: PathBuf,
    pub format: Format,
    pub link_shared: bool,
}

pub fn run(args: FileArgs) {
    // 1. Read and deserialize snapshot.
    let raw = match std::fs::read_to_string(&args.snapshot_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading '{}': {}", args.snapshot_path.display(), e);
            std::process::exit(1);
        }
    };

    let snap: DeductionSnapshot = match serde_json::from_str(&raw) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Error parsing snapshot '{}': {}",
                args.snapshot_path.display(),
                e
            );
            std::process::exit(1);
        }
    };

    // 2. Decode double-encoded prolog_clauses.
    let clauses: Vec<String> = match serde_json::from_str(&snap.prolog_clauses) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error decoding prolog_clauses: {}", e);
            std::process::exit(1);
        }
    };

    // 3. Decode double-encoded tableau_entries.
    let entries: Vec<PredicateEntry> = match serde_json::from_str(&snap.tableau_entries) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error decoding tableau_entries: {}", e);
            std::process::exit(1);
        }
    };

    // 4. Parse Prolog rules and build colorized DOT.
    let source = clauses.join("\n");
    let rules = parse_prolog_rules(&source);
    let coloring = coloring_from_entries(&entries);
    let opts = DotOptions { link_shared_conditions: args.link_shared };
    let dot = generate_dot(&rules, Some(&coloring), &opts);

    // 5. Determine output path.
    let stem = &snap.deduction_id[..snap.deduction_id.len().min(8)];
    let base = args.out_dir.join(format!("{}_final", stem));

    // 6. Write output(s).
    let out_path = if args.format == Format::Html {
        let html_path = base.with_extension("html");
        write_html(&[HtmlPhase {
            cycle_num:  snap.cycles_run,
            phase_name: snap.status.clone(),
            dot_src:    dot,
        }], &html_path);
        html_path
    } else {
        let written = write_outputs(&dot, &base, args.format);
        written.into_iter().next().unwrap_or_else(|| base.with_extension("dot"))
    };

    // 7. Print summary.
    let (kt, kf, ku, unk) = truth_counts(&entries);
    print_file_summary(
        &snap.deduction_id,
        &snap.status,
        snap.cycles_run,
        snap.initial_goal.as_deref(),
        entries.len(),
        kt,
        kf,
        ku,
        unk,
        &out_path,
    );
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn truth_counts(entries: &[PredicateEntry]) -> (usize, usize, usize, usize) {
    let mut kt = 0usize;
    let mut kf = 0usize;
    let mut ku = 0usize;
    let mut unk = 0usize;
    for e in entries {
        match e.truth_value {
            TruthValue::KnownTrue        => kt  += 1,
            TruthValue::KnownFalse       => kf  += 1,
            TruthValue::KnownUnresolved  => ku  += 1,
            TruthValue::Unknown          => unk += 1,
        }
    }
    (kt, kf, ku, unk)
}
