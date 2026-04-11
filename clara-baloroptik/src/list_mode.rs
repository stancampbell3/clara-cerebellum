//! `baloroptik list` — enumerate persisted deductions from a running clara-api.
//!
//! Calls `GET /deduce?limit=N` and prints a summary table so the user can
//! pick a UUID to pass to `baloroptik trace` or `baloroptik watch`.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ListResponse {
    deductions: Vec<DeductionSummary>,
}

#[derive(Debug, Deserialize)]
struct DeductionSummary {
    deduction_id:  String,
    status:        String,
    cycles_run:    u32,
    initial_goal:  Option<String>,
    created_at_ms: i64,
}

pub struct ListArgs {
    pub api_base: String,
    pub limit:    u32,
}

pub fn run(args: ListArgs) {
    let client = reqwest::blocking::Client::new();
    let base = args.api_base.trim_end_matches('/');
    let url = format!("{}/deduce?limit={}", base, args.limit);

    let resp: ListResponse = client
        .get(&url)
        .send()
        .unwrap_or_else(|e| die(&format!("GET {}: {}", url, e)))
        .error_for_status()
        .unwrap_or_else(|e| die(&format!("GET {} returned error: {}", url, e)))
        .json()
        .unwrap_or_else(|e| die(&format!("parse list response: {}", e)));

    if resp.deductions.is_empty() {
        println!("No persisted deductions found.");
        return;
    }

    // Header
    println!(
        "  {:<38} {:<14} {:<7} {:<28} {}",
        "Deduction ID", "Status", "Cycles", "Goal", "Created (UTC)"
    );
    println!(
        "  {:<38} {:<14} {:<7} {:<28} {}",
        "──────────────────────────────────────",
        "────────────",
        "──────",
        "──────────────────────────",
        "───────────────────────"
    );

    for d in &resp.deductions {
        let goal = d.initial_goal.as_deref().unwrap_or("—");
        let goal_short = if goal.len() > 26 {
            format!("{}…", &goal[..25])
        } else {
            goal.to_string()
        };
        let created = format_ms(d.created_at_ms);
        println!(
            "  {:<38} {:<14} {:<7} {:<28} {}",
            d.deduction_id,
            truncate(&d.status, 13),
            d.cycles_run,
            goal_short,
            created,
        );
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn format_ms(ms: i64) -> String {
    // Simple UTC formatting without pulling in chrono.
    let secs = ms / 1000;
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;
    // Approximate Gregorian date from days since 1970-01-01.
    let (y, mo, d) = days_to_ymd(days_since_epoch as u32);
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}Z", y, mo, d, h, m, s)
}

fn days_to_ymd(mut days: u32) -> (u32, u32, u32) {
    // Gregorian calendar — good enough for display purposes.
    let mut y = 1970u32;
    loop {
        let leap = is_leap(y);
        let dy = if leap { 366 } else { 365 };
        if days < dy { break; }
        days -= dy;
        y += 1;
    }
    let month_days = [31, if is_leap(y) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut mo = 1u32;
    for &md in &month_days {
        if days < md { break; }
        days -= md;
        mo += 1;
    }
    (y, mo, days + 1)
}

fn is_leap(y: u32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max { format!("{}…", &s[..max.saturating_sub(1)]) } else { s.to_string() }
}

fn die(msg: &str) -> ! {
    eprintln!("Error: {}", msg);
    std::process::exit(1);
}
