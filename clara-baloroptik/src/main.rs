//! baloroptik — the eye of Balor
//!
//! Deduction trace visualization CLI for the Clara reasoning system.
//! Consumes persisted deduction state and emits sequences of colored DOT
//! graphs representing each phase of the hybrid Prolog/CLIPS reasoning trace.
//!
//! # Subcommands
//!
//! ```text
//! baloroptik file   <SNAPSHOT>                     # offline: one DOT from a snapshot JSON file
//! baloroptik trace  <DEDUCTION_ID>                 # online:  full trace sequence from clara-api
//! baloroptik list                                  # online:  list persisted deductions
//! baloroptik watch  <DEDUCTION_ID>                 # online:  stream DOT files as phases arrive
//! baloroptik replay <SNAPSHOT> <CHANGES>           # offline: replay from exported tableau changes
//! ```

mod file_mode;
mod list_mode;
mod render;
mod replay_mode;
mod trace_mode;
mod watch_mode;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use render::Format;

// ── CLI definition ─────────────────────────────────────────────────────────────

#[derive(Debug, Parser)]
#[command(
    name = "baloroptik",
    about = "The eye of Balor — deduction trace visualization for Clara",
    long_about = "\
baloroptik reads persisted Clara deduction state and emits a sequence of \
colored Graphviz DOT graphs representing the reasoning trace.\n\n\
Each graph node's fill color reflects its truth value in the Dagda tableau \
at that phase of the cycle:\n\
  green  (#28a745) — KnownTrue\n\
  red    (#dc3545) — KnownFalse\n\
  amber  (#ffc107) — KnownUnresolved (mixed entries)\n\
  gray   (#adb5bd) — Unknown\n\
  structural default — not yet in tableau"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Generate a single colored DOT graph from a local deduction snapshot JSON file.
    ///
    /// Reads the final Dagda tableau stored in the snapshot and overlays truth-value
    /// colors on the Prolog rule/fact dependency graph.  No running server needed.
    File {
        /// Path to the deduction snapshot JSON file.
        snapshot: PathBuf,

        /// Directory for output files.
        #[arg(long, default_value = ".", value_name = "DIR")]
        out_dir: PathBuf,

        /// Output format.
        #[arg(long, default_value = "dot", value_name = "FORMAT")]
        format: Format,

        /// Include shared-condition edges between nodes that share the same
        /// condition label across rules.
        #[arg(long)]
        link_shared: bool,
    },

    /// Fetch and render the full reasoning trace from a running clara-api instance.
    ///
    /// Downloads one pre-colorized DOT graph per recorded trace phase and writes
    /// them as sequentially numbered files.  Requires the deduction to have been
    /// run with `trace: true`.
    Trace {
        /// Deduction UUID to visualize.
        deduction_id: String,

        /// clara-api base URL.
        #[arg(long, default_value = "http://localhost:8080", value_name = "URL")]
        api: String,

        /// Directory for output files.
        #[arg(long, default_value = ".", value_name = "DIR")]
        out_dir: PathBuf,

        /// Output format.
        #[arg(long, default_value = "dot", value_name = "FORMAT")]
        format: Format,

        /// Include shared-condition edges.
        #[arg(long)]
        link_shared: bool,
    },

    /// List persisted deductions from a running clara-api instance.
    ///
    /// Prints a summary table of recent deductions so you can pick a UUID
    /// to pass to `trace`, `watch`, or the API directly.
    List {
        /// clara-api base URL.
        #[arg(long, default_value = "http://localhost:8080", value_name = "URL")]
        api: String,

        /// Maximum number of deductions to show (server caps at 500).
        #[arg(long, default_value_t = 50, value_name = "N")]
        limit: u32,
    },

    /// Live-poll a running deduction and write DOT files as each phase is recorded.
    ///
    /// Exits once the deduction reaches a terminal status (converged, interrupted,
    /// or error).  Use `--poll-ms` to tune the polling interval.
    Watch {
        /// Deduction UUID to watch.
        deduction_id: String,

        /// clara-api base URL.
        #[arg(long, default_value = "http://localhost:8080", value_name = "URL")]
        api: String,

        /// Directory for output files.
        #[arg(long, default_value = ".", value_name = "DIR")]
        out_dir: PathBuf,

        /// Output format (html not supported in watch mode; falls back to dot).
        #[arg(long, default_value = "dot", value_name = "FORMAT")]
        format: Format,

        /// Polling interval in milliseconds.
        #[arg(long, default_value_t = 500, value_name = "MS")]
        poll_ms: u64,
    },

    /// Replay a deduction trace offline from a snapshot and an exported changes file.
    ///
    /// The changes file is a JSON array of TableauChange objects, obtained via
    /// `GET /deduce/{id}/trace/export`.  Generates DOT graphs locally without
    /// a running server.
    Replay {
        /// Path to the deduction snapshot JSON file.
        snapshot: PathBuf,

        /// Path to the tableau_changes export JSON file.
        changes: PathBuf,

        /// Directory for output files.
        #[arg(long, default_value = ".", value_name = "DIR")]
        out_dir: PathBuf,

        /// Output format.
        #[arg(long, default_value = "dot", value_name = "FORMAT")]
        format: Format,

        /// Include shared-condition edges in DOT graph.
        #[arg(long)]
        link_shared: bool,
    },
}

// ── Entry point ────────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::File { snapshot, out_dir, format, link_shared } => {
            ensure_dir(&out_dir);
            file_mode::run(file_mode::FileArgs {
                snapshot_path: snapshot,
                out_dir,
                format,
                link_shared,
            });
        }

        Commands::Trace { deduction_id, api, out_dir, format, link_shared } => {
            ensure_dir(&out_dir);
            trace_mode::run(trace_mode::TraceArgs {
                deduction_id,
                api_base: api,
                out_dir,
                format,
                _link_shared: link_shared,
            });
        }

        Commands::List { api, limit } => {
            list_mode::run(list_mode::ListArgs {
                api_base: api,
                limit,
            });
        }

        Commands::Watch { deduction_id, api, out_dir, format, poll_ms } => {
            ensure_dir(&out_dir);
            watch_mode::run(watch_mode::WatchArgs {
                deduction_id,
                api_base: api,
                out_dir,
                format,
                poll_ms,
            });
        }

        Commands::Replay { snapshot, changes, out_dir, format, link_shared } => {
            ensure_dir(&out_dir);
            replay_mode::run(replay_mode::ReplayArgs {
                snapshot_path: snapshot,
                changes_path:  changes,
                out_dir,
                format,
                link_shared,
            });
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn ensure_dir(dir: &PathBuf) {
    if !dir.exists() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("Error creating output directory '{}': {}", dir.display(), e);
            std::process::exit(1);
        }
    }
}
