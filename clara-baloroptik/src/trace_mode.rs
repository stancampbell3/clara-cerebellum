//! `baloroptik trace` — online mode.
//!
//! Queries a running `clara-api` instance to fetch the full ordered sequence
//! of trace phases for a given deduction, then downloads the pre-colorized
//! DOT graph for each phase and writes it to a sequentially numbered file.
//!
//! The API does all parsing and coloring; we are purely a downloader +
//! file organizer here.
//!
//! # File naming
//!
//! Output files are named:
//! ```text
//! <stem>_<i:03>_<phase_slug>.<ext>
//! ```
//! where `stem` is the first 8 characters of the deduction UUID, `i` is a
//! zero-based sequential index (padded to 3 digits), and `phase_slug` is the
//! phase string with `/` replaced by `_`.
//!
//! When `--format html`, a single `<stem>_trace.html` viewer is written instead.

use std::path::PathBuf;

use serde::Deserialize;

use crate::render::{print_trace_header, print_trace_row, write_outputs, write_html, Format, HtmlPhase};

// ── API response types ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TraceListResponse {
    trace: Vec<TracePhase>,
}

#[derive(Debug, Deserialize)]
struct TracePhase {
    change_id: String,
    cycle_num: u32,
    phase:     String,
}

#[derive(Debug, Deserialize)]
struct DeduceStatusResponse {
    status: String,
    cycles: Option<u32>,
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub struct TraceArgs {
    pub deduction_id:  String,
    pub api_base:      String,
    pub out_dir:       PathBuf,
    pub format:        Format,
    pub _link_shared:  bool,
}

pub fn run(args: TraceArgs) {
    let client = reqwest::blocking::Client::new();
    let base = args.api_base.trim_end_matches('/');

    // 1. Fetch deduction status for the summary header.
    let status_url = format!("{}/deduce/{}", base, args.deduction_id);
    let status_resp: DeduceStatusResponse = client
        .get(&status_url)
        .send()
        .unwrap_or_else(|e| die(&format!("GET {}: {}", status_url, e)))
        .error_for_status()
        .unwrap_or_else(|e| die(&format!("GET {} returned error: {}", status_url, e)))
        .json()
        .unwrap_or_else(|e| die(&format!("parse status response: {}", e)));

    // 2. Fetch trace phase list.
    let trace_url = format!("{}/deduce/{}/trace", base, args.deduction_id);
    let trace_resp: TraceListResponse = client
        .get(&trace_url)
        .send()
        .unwrap_or_else(|e| die(&format!("GET {}: {}", trace_url, e)))
        .error_for_status()
        .unwrap_or_else(|e| die(&format!("GET {} returned error: {}", trace_url, e)))
        .json()
        .unwrap_or_else(|e| die(&format!("parse trace list response: {}", e)));

    if trace_resp.trace.is_empty() {
        eprintln!(
            "No trace phases found for deduction {}.\n\
             Re-run the deduction with `trace: true` to record phases.",
            args.deduction_id
        );
        std::process::exit(1);
    }

    // 3. Print summary header.
    print_trace_header(&args.deduction_id, &status_resp.status, status_resp.cycles);

    let stem = &args.deduction_id[..args.deduction_id.len().min(8)];
    let mut html_phases: Vec<HtmlPhase> = Vec::new();

    // 4. Download and write each phase.
    for (i, phase) in trace_resp.trace.iter().enumerate() {
        let dot_url = format!(
            "{}/deduce/{}/trace/{}/dot",
            base, args.deduction_id, phase.change_id
        );

        let dot_src = client
            .get(&dot_url)
            .send()
            .unwrap_or_else(|e| die(&format!("GET {}: {}", dot_url, e)))
            .error_for_status()
            .unwrap_or_else(|e| die(&format!("GET {} returned error: {}", dot_url, e)))
            .text()
            .unwrap_or_else(|e| die(&format!("read DOT body: {}", e)));

        let phase_slug = phase.phase.replace('/', "_");
        let filename_stem = format!("{}_{}_{}", stem, format_index(i), phase_slug);

        if args.format == Format::Html {
            html_phases.push(HtmlPhase {
                cycle_num:  phase.cycle_num,
                phase_name: phase.phase.clone(),
                dot_src,
            });
            print_trace_row(i, phase.cycle_num, &phase.phase, "(buffered for HTML)");
        } else {
            let base_path = args.out_dir.join(&filename_stem);
            let written = write_outputs(&dot_src, &base_path, args.format);
            let display_name = written
                .first()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| format!("{}.dot", filename_stem));
            print_trace_row(i, phase.cycle_num, &phase.phase, &display_name);
        }
    }

    // 5. Write HTML viewer if requested.
    if args.format == Format::Html && !html_phases.is_empty() {
        let html_path = args.out_dir.join(format!("{}_trace.html", stem));
        write_html(&html_phases, &html_path);
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn format_index(i: usize) -> String {
    format!("{:03}", i)
}

fn die(msg: &str) -> ! {
    eprintln!("Error: {}", msg);
    std::process::exit(1);
}
