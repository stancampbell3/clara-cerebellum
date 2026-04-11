//! Shared output utilities: write DOT/SVG/HTML files and print summary tables.

use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Output format requested by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    Dot,
    Svg,
    Both,
    /// Emit a single self-contained HTML step-through viewer (viz.js via CDN).
    /// For multi-phase subcommands (`trace`, `replay`) the caller accumulates
    /// all phases and calls `write_html` once at the end.
    Html,
}

/// Write a DOT string to `path`.  Exits with code 1 on I/O error.
pub fn write_dot(dot_src: &str, path: &Path) {
    if let Err(e) = std::fs::write(path, dot_src) {
        eprintln!("Error writing '{}': {}", path.display(), e);
        std::process::exit(1);
    }
    eprintln!("Wrote '{}'", path.display());
}

/// Render `dot_src` to SVG by shelling out to the system `dot` binary.
///
/// Returns the SVG bytes on success, or a human-readable error string.
/// Callers should warn and continue when this returns `Err` — SVG is
/// best-effort; the `.dot` file is always written first.
pub fn render_svg(dot_src: &str) -> Result<Vec<u8>, String> {
    let mut child = Command::new("dot")
        .arg("-Tsvg")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "graphviz `dot` not found in PATH — install graphviz to render SVG".to_string()
            } else {
                format!("failed to spawn `dot`: {}", e)
            }
        })?;

    child
        .stdin
        .take()
        .unwrap()
        .write_all(dot_src.as_bytes())
        .map_err(|e| format!("write to `dot` stdin: {}", e))?;

    let output = child
        .wait_with_output()
        .map_err(|e| format!("`dot` wait_with_output: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("`dot` exited non-zero: {}", stderr.trim()));
    }

    Ok(output.stdout)
}

/// Write DOT (and optionally SVG) for a single graph.
///
/// Returns `None` for `Format::Html` — HTML callers accumulate phases and call
/// `write_html` themselves at the end.
///
/// `base_path` should be the full path **without** extension (e.g. `/tmp/eye/abc_000_initial`).
/// Returns the path(s) written.
pub fn write_outputs(dot_src: &str, base_path: &Path, format: Format) -> Vec<PathBuf> {
    if format == Format::Html {
        // HTML is written in bulk at the end; nothing to write per-phase here.
        return Vec::new();
    }

    let mut written = Vec::new();

    let dot_path = base_path.with_extension("dot");
    write_dot(dot_src, &dot_path);
    written.push(dot_path);

    if format == Format::Svg || format == Format::Both {
        match render_svg(dot_src) {
            Ok(svg_bytes) => {
                let svg_path = base_path.with_extension("svg");
                if let Err(e) = std::fs::write(&svg_path, &svg_bytes) {
                    eprintln!("Error writing '{}': {}", svg_path.display(), e);
                } else {
                    eprintln!("Wrote '{}'", svg_path.display());
                    written.push(svg_path);
                }
            }
            Err(e) => {
                eprintln!("Warning: SVG rendering skipped — {}", e);
            }
        }
    }

    written
}

// ── HTML viewer ────────────────────────────────────────────────────────────────

/// A single phase entry for the HTML viewer.
pub struct HtmlPhase {
    pub cycle_num:  u32,
    pub phase_name: String,
    pub dot_src:    String,
}

/// Write a self-contained HTML step-through viewer to `path`.
///
/// Each element of `phases` corresponds to one recorded trace step. The viewer
/// renders DOT graphs client-side using viz.js (loaded from CDN) and allows
/// stepping through phases with Prev/Next buttons or the arrow keys.
pub fn write_html(phases: &[HtmlPhase], path: &Path) {
    let html = build_html(phases);
    if let Err(e) = std::fs::write(path, html) {
        eprintln!("Error writing '{}': {}", path.display(), e);
        std::process::exit(1);
    }
    eprintln!("Wrote '{}'", path.display());
}

fn build_html(phases: &[HtmlPhase]) -> String {
    // Embed DOT sources as a JS array, escaping backticks and backslashes.
    let phases_js: Vec<String> = phases
        .iter()
        .map(|p| {
            let escaped = p.dot_src
                .replace('\\', "\\\\")
                .replace('`', "\\`")
                .replace("${", "\\${");
            format!(
                "  {{ cycle: {}, phase: {}, dot: `{}` }}",
                p.cycle_num,
                serde_json::to_string(&p.phase_name).unwrap_or_default(),
                escaped,
            )
        })
        .collect();
    let phases_array = phases_js.join(",\n");

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Baloroptik — Deduction Trace Viewer</title>
<script src="https://cdn.jsdelivr.net/npm/@viz-js/viz@3/build/viz-standalone.js"></script>
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{ font-family: monospace; background: #1a1a1a; color: #e0e0e0; display: flex; flex-direction: column; height: 100vh; }}
  #header {{ padding: 10px 16px; background: #111; border-bottom: 1px solid #333; display: flex; align-items: center; gap: 16px; flex-wrap: wrap; }}
  #header h1 {{ font-size: 14px; color: #aaa; font-weight: normal; letter-spacing: 0.05em; }}
  #phase-label {{ font-size: 15px; color: #e0e0e0; font-weight: bold; }}
  #cycle-label {{ font-size: 12px; color: #888; }}
  #nav {{ margin-left: auto; display: flex; gap: 8px; align-items: center; }}
  button {{ background: #2a2a2a; border: 1px solid #444; color: #ccc; padding: 4px 14px; cursor: pointer; font-family: monospace; font-size: 13px; border-radius: 3px; }}
  button:hover {{ background: #3a3a3a; }}
  button:disabled {{ opacity: 0.35; cursor: default; }}
  #counter {{ font-size: 12px; color: #666; min-width: 60px; text-align: center; }}
  #graph-area {{ flex: 1; overflow: auto; display: flex; align-items: center; justify-content: center; padding: 16px; }}
  #graph-area svg {{ max-width: 100%; height: auto; }}
  #strip {{ display: flex; gap: 4px; padding: 8px 12px; background: #111; border-top: 1px solid #333; overflow-x: auto; flex-shrink: 0; }}
  .tab {{ padding: 3px 10px; background: #222; border: 1px solid #333; color: #888; cursor: pointer; font-size: 11px; border-radius: 2px; white-space: nowrap; flex-shrink: 0; }}
  .tab:hover {{ background: #2a2a2a; color: #bbb; }}
  .tab.active {{ background: #2c3e50; border-color: #4a6fa5; color: #d0e4ff; }}
  #legend {{ display: flex; gap: 14px; align-items: center; font-size: 11px; color: #888; }}
  .swatch {{ display: inline-block; width: 11px; height: 11px; border-radius: 2px; vertical-align: middle; margin-right: 4px; border: 1px solid #555; }}
  #error {{ color: #f66; font-size: 13px; padding: 8px; }}
</style>
</head>
<body>
<div id="header">
  <h1>baloroptik</h1>
  <div>
    <div id="phase-label">—</div>
    <div id="cycle-label">—</div>
  </div>
  <div id="legend">
    <span><span class="swatch" style="background:#28a745"></span>KnownTrue</span>
    <span><span class="swatch" style="background:#dc3545"></span>KnownFalse</span>
    <span><span class="swatch" style="background:#ffc107"></span>KnownUnresolved</span>
    <span><span class="swatch" style="background:#adb5bd"></span>Unknown</span>
  </div>
  <div id="nav">
    <button id="btn-prev" onclick="step(-1)" disabled>&#8592; Prev</button>
    <span id="counter">— / —</span>
    <button id="btn-next" onclick="step(1)">Next &#8594;</button>
  </div>
</div>
<div id="graph-area"><div id="error"></div><div id="graph-output"></div></div>
<div id="strip"></div>

<script>
const PHASES = [
{phases_array}
];

let current = 0;
let viz = null;

Viz.instance().then(v => {{
  viz = v;
  render(0);
  buildStrip();
}}).catch(e => {{
  document.getElementById('error').textContent = 'viz.js failed to load: ' + e;
}});

function render(idx) {{
  if (!viz || idx < 0 || idx >= PHASES.length) return;
  current = idx;
  const p = PHASES[idx];
  document.getElementById('phase-label').textContent = p.phase;
  document.getElementById('cycle-label').textContent = 'Cycle ' + p.cycle + '  ·  Phase ' + (idx + 1) + ' of ' + PHASES.length;
  document.getElementById('counter').textContent = (idx + 1) + ' / ' + PHASES.length;
  document.getElementById('btn-prev').disabled = (idx === 0);
  document.getElementById('btn-next').disabled = (idx === PHASES.length - 1);
  // Highlight active tab
  document.querySelectorAll('.tab').forEach((t, i) => t.classList.toggle('active', i === idx));
  try {{
    const svg = viz.renderSVGElement(p.dot);
    const out = document.getElementById('graph-output');
    out.innerHTML = '';
    out.appendChild(svg);
  }} catch(e) {{
    document.getElementById('graph-output').innerHTML = '<pre style="color:#f66">' + e + '</pre>';
  }}
}}

function step(dir) {{ render(current + dir); }}

function buildStrip() {{
  const strip = document.getElementById('strip');
  PHASES.forEach((p, i) => {{
    const tab = document.createElement('div');
    tab.className = 'tab' + (i === 0 ? ' active' : '');
    tab.textContent = 'C' + p.cycle + ':' + p.phase;
    tab.title = 'Cycle ' + p.cycle + ' — ' + p.phase;
    tab.onclick = () => render(i);
    strip.appendChild(tab);
  }});
}}

document.addEventListener('keydown', e => {{
  if (e.key === 'ArrowRight' || e.key === 'ArrowDown')  {{ e.preventDefault(); step(1); }}
  if (e.key === 'ArrowLeft'  || e.key === 'ArrowUp')    {{ e.preventDefault(); step(-1); }}
}});
</script>
</body>
</html>"#,
        phases_array = phases_array,
    )
}

// ── Summary helpers ────────────────────────────────────────────────────────────

/// Print the header for the `trace` subcommand summary table.
pub fn print_trace_header(deduction_id: &str, status: &str, cycles: Option<u32>) {
    println!("Deduction: {}", deduction_id);
    match cycles {
        Some(n) => println!("Status:    {} ({} cycles)", status, n),
        None    => println!("Status:    {}", status),
    }
    println!();
    println!(
        "  {:<4} {:<6} {:<24} {}",
        "#", "Cycle", "Phase", "File"
    );
    println!(
        "  {:<4} {:<6} {:<24} {}",
        "───", "─────", "──────────────────────", "──────────────────────────────────────"
    );
}

/// Print one row of the `trace` summary table.
pub fn print_trace_row(index: usize, cycle: u32, phase: &str, filename: &str) {
    println!("  {:<4} {:<6} {:<24} {}", index, cycle, phase, filename);
}

/// Print the `file` subcommand summary.
pub fn print_file_summary(
    deduction_id: &str,
    status: &str,
    cycles: u32,
    goal: Option<&str>,
    entry_count: usize,
    known_true: usize,
    known_false: usize,
    unresolved: usize,
    unknown: usize,
    out_file: &Path,
) {
    println!("Deduction: {}", deduction_id);
    println!("Status:    {} ({} cycles)", status, cycles);
    if let Some(g) = goal {
        println!("Goal:      {}", g);
    }
    println!(
        "Tableau:   {} entries  (T:{} F:{} U:{} ?:{})",
        entry_count, known_true, known_false, unresolved, unknown
    );
    println!();
    println!("Wrote: {}", out_file.display());
}
