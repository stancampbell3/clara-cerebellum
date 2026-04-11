//! `baloroptik replay` — offline trace playback from a `tableau_changes` dump.
//!
//! Reads a deduction snapshot JSON file (for the Prolog clauses) and a
//! `tableau_changes` JSON export (from `GET /deduce/{id}/trace/export`), then
//! re-generates colorized DOT graphs locally — no running server required.
//!
//! The export format is a plain JSON array of `TableauChange` objects as
//! returned by the clara-api export endpoint.

use std::path::PathBuf;

use clara_cycle::{coloring_from_entries, generate_dot, parse_prolog_rules, DotOptions, PredicateEntry};
use serde::Deserialize;

use crate::render::{write_outputs, Format, HtmlPhase, write_html};

// ── Local deserialization types ────────────────────────────────────────────────

/// Snapshot fields needed for replay (same double-encoding as `file_mode`).
#[derive(Debug, Deserialize)]
struct SnapshotSlim {
    deduction_id:  String,
    prolog_clauses: String,
    initial_goal:  Option<String>,
    status:        String,
    cycles_run:    u32,
}

/// Mirrors `clara_coire::TableauChange` for JSON deserialization.
/// Remaining fields (event_origin, event_type, event_data) are ignored.
#[derive(Debug, Deserialize)]
struct TableauChange {
    #[allow(dead_code)]
    change_id:      String,
    deduction_id:   String,
    cycle_num:      u32,
    phase:          String,
    entries_json:   String,
    recorded_at_ms: i64,
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub struct ReplayArgs {
    pub snapshot_path: PathBuf,
    pub changes_path:  PathBuf,
    pub out_dir:       PathBuf,
    pub format:        Format,
    pub link_shared:   bool,
}

pub fn run(args: ReplayArgs) {
    // 1. Load and decode snapshot.
    let snap_raw = read_file(&args.snapshot_path);
    let snap: SnapshotSlim = serde_json::from_str(&snap_raw).unwrap_or_else(|e| {
        eprintln!("Error parsing snapshot '{}': {}", args.snapshot_path.display(), e);
        std::process::exit(1);
    });

    let clauses: Vec<String> = serde_json::from_str(&snap.prolog_clauses).unwrap_or_else(|e| {
        eprintln!("Error decoding prolog_clauses: {}", e);
        std::process::exit(1);
    });

    // 2. Load and decode tableau changes.
    let changes_raw = read_file(&args.changes_path);
    let mut changes: Vec<TableauChange> = serde_json::from_str(&changes_raw).unwrap_or_else(|e| {
        eprintln!("Error parsing changes '{}': {}", args.changes_path.display(), e);
        std::process::exit(1);
    });

    // Filter to matching deduction_id (defensive).
    changes.retain(|c| c.deduction_id == snap.deduction_id);
    if changes.is_empty() {
        eprintln!(
            "Warning: no tableau changes found for deduction {} in '{}'.",
            snap.deduction_id,
            args.changes_path.display()
        );
    }

    // Sort by cycle then recording time.
    changes.sort_by_key(|c| (c.cycle_num, c.recorded_at_ms));

    // 3. Parse Prolog rules once (shared across all phases).
    let source = clauses.join("\n");
    let rules = parse_prolog_rules(&source);
    let opts = DotOptions { link_shared_conditions: args.link_shared };

    let stem = &snap.deduction_id[..snap.deduction_id.len().min(8)];
    let mut html_phases: Vec<HtmlPhase> = Vec::new();

    println!("Deduction: {}", snap.deduction_id);
    println!("Status:    {} ({} cycles)", snap.status, snap.cycles_run);
    if let Some(g) = &snap.initial_goal { println!("Goal:      {}", g); }
    println!();
    println!(
        "  {:<4} {:<6} {:<24} {}",
        "#", "Cycle", "Phase", "File"
    );
    println!(
        "  {:<4} {:<6} {:<24} {}",
        "───", "─────", "──────────────────────", "──────────────────────────────────────"
    );

    // 4. Generate one DOT per phase.
    for (i, change) in changes.iter().enumerate() {
        let entries: Vec<PredicateEntry> = serde_json::from_str(&change.entries_json)
            .unwrap_or_else(|e| {
                eprintln!("Warning: could not decode entries_json for phase '{}': {}", change.phase, e);
                Vec::new()
            });

        let coloring = coloring_from_entries(&entries);
        let dot_src = generate_dot(&rules, Some(&coloring), &opts);

        let phase_slug = change.phase.replace('/', "_");
        let filename_stem = format!("{}_{}_{}", stem, format!("{:03}", i), phase_slug);

        if args.format == Format::Html {
            html_phases.push(HtmlPhase {
                cycle_num:  change.cycle_num,
                phase_name: change.phase.clone(),
                dot_src,
            });
            println!("  {:<4} {:<6} {:<24} (buffered for HTML)", i, change.cycle_num, change.phase);
        } else {
            let base_path = args.out_dir.join(&filename_stem);
            let written = write_outputs(&dot_src, &base_path, args.format);
            let display = written.first()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| format!("{}.dot", filename_stem));
            println!("  {:<4} {:<6} {:<24} {}", i, change.cycle_num, change.phase, display);
        }
    }

    // 5. Write HTML viewer if requested.
    if args.format == Format::Html && !html_phases.is_empty() {
        let html_path = args.out_dir.join(format!("{}_replay.html", stem));
        write_html(&html_phases, &html_path);
    }
}

fn read_file(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("Error reading '{}': {}", path.display(), e);
        std::process::exit(1);
    })
}
