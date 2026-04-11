//! `baloroptik watch` — live-poll a running deduction and stream DOT files.
//!
//! Polls `GET /deduce/{id}/trace` at a configurable interval, downloading each
//! newly recorded phase's DOT as it appears and writing it to disk immediately.
//! Exits cleanly once the deduction reaches a terminal status.
//!
//! `--format html` is not supported for watch mode (stream semantics don't
//! compose with a single combined output file). A warning is printed and the
//! format falls back to `dot`.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

use crate::render::{write_outputs, Format};

#[derive(Debug, Deserialize)]
struct TraceListResponse {
    trace: Vec<TracePhase>,
}

#[derive(Debug, Deserialize)]
struct TracePhase {
    change_id:  String,
    #[allow(dead_code)]
    cycle_num:  u32,
    phase:      String,
}

#[derive(Debug, Deserialize)]
struct StatusResponse {
    status: String,
    cycles: Option<u32>,
}

pub struct WatchArgs {
    pub deduction_id: String,
    pub api_base:     String,
    pub out_dir:      PathBuf,
    pub format:       Format,
    pub poll_ms:      u64,
}

pub fn run(args: WatchArgs) {
    let format = if args.format == Format::Html {
        eprintln!("Warning: --format html is not supported in watch mode; using dot instead.");
        Format::Dot
    } else {
        args.format
    };

    let client = reqwest::blocking::Client::new();
    let base = args.api_base.trim_end_matches('/');
    let stem = &args.deduction_id[..args.deduction_id.len().min(8)];

    let mut seen: HashSet<String> = HashSet::new();
    let mut phase_index: usize = 0;
    let poll_dur = Duration::from_millis(args.poll_ms);

    eprintln!(
        "Watching deduction {} (poll every {}ms) — Ctrl-C to abort",
        args.deduction_id, args.poll_ms
    );

    loop {
        // 1. Check status.
        let status_url = format!("{}/deduce/{}", base, args.deduction_id);
        let status: StatusResponse = match client.get(&status_url).send() {
            Ok(r) => match r.json() {
                Ok(s)  => s,
                Err(e) => { eprintln!("Warning: parse status: {}", e); std::thread::sleep(poll_dur); continue; }
            },
            Err(e) => { eprintln!("Warning: GET {}: {}", status_url, e); std::thread::sleep(poll_dur); continue; }
        };

        // 2. Fetch trace phase list.
        let trace_url = format!("{}/deduce/{}/trace", base, args.deduction_id);
        let trace_resp: TraceListResponse = match client.get(&trace_url).send() {
            Ok(r) => match r.json() {
                Ok(t)  => t,
                Err(e) => { eprintln!("Warning: parse trace list: {}", e); std::thread::sleep(poll_dur); continue; }
            },
            Err(e) => { eprintln!("Warning: GET {}: {}", trace_url, e); std::thread::sleep(poll_dur); continue; }
        };

        // 3. Download any new phases.
        let mut new_count = 0usize;
        for phase in &trace_resp.trace {
            if seen.contains(&phase.change_id) { continue; }
            seen.insert(phase.change_id.clone());

            let dot_url = format!("{}/deduce/{}/trace/{}/dot", base, args.deduction_id, phase.change_id);
            let dot_src = match client.get(&dot_url).send() {
                Ok(r) => match r.text() {
                    Ok(t)  => t,
                    Err(e) => { eprintln!("Warning: read DOT body: {}", e); continue; }
                },
                Err(e) => { eprintln!("Warning: GET {}: {}", dot_url, e); continue; }
            };

            let phase_slug = phase.phase.replace('/', "_");
            let filename_stem = format!("{}_{}_{}", stem, format_index(phase_index), phase_slug);
            let base_path = args.out_dir.join(&filename_stem);
            write_outputs(&dot_src, &base_path, format);
            phase_index += 1;
            new_count += 1;
        }

        // 4. Status line.
        eprint!(
            "\rPolling... cycle {:?} | {} phases written | status: {}    ",
            status.cycles.unwrap_or(0),
            phase_index,
            status.status,
        );

        // 5. Exit on terminal status.
        let s = &status.status;
        if s == "converged" || s == "interrupted" || s.starts_with("error:") {
            eprintln!(); // newline after the status line
            println!(
                "\nDone. Deduction {} ({} phases written)",
                args.deduction_id, phase_index
            );
            break;
        }

        if new_count == 0 {
            std::thread::sleep(poll_dur);
        }
    }
}

fn format_index(i: usize) -> String {
    format!("{:03}", i)
}
