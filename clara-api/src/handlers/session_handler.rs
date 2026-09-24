use actix_web::{web, HttpResponse};
use clara_session::SessionManager;
use clara_ritual::RitualRegistry;
use crate::fierypit_registry::FieryPitRegistry;
use crate::subprocess::SubprocessPool;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use uuid::Uuid;
use clara_cycle::{CycleStatus, DeductionResult};

use crate::models::{
    ApiError, CreateSessionRequest, SaveSessionRequest, ResourceInfo, SessionResponse,
    TerminateResponse, LoadRulesRequest, LoadFactsRequest, RunRequest, RunResponse, QueryFactsResponse
};

/// A cached FieryPit service JWT with its expiry `Instant`.
///
/// Stored in `AppState::fiery_pit_token_cache` and refreshed lazily when the
/// token is within `FIERYPIT_TOKEN_MARGIN_S` seconds of expiry, or immediately
/// when a participant returns `401 Unauthorized` during auto-bootstrap.
pub struct CachedToken {
    pub token:      String,
    pub expires_at: Instant,
}

/// In-flight or completed deduction run tracked in `AppState::deductions`.
pub struct DeductionEntry {
    pub status:            CycleStatus,
    /// Why a non-converged run ended: `interrupted`, `deadline`, `max_cycles`
    /// or `error`. `None` while running and on convergence.
    pub reason:            Option<String>,
    pub result:            Option<DeductionResult>,
    pub cycles:            u32,
    pub interrupt:         Arc<AtomicBool>,
    pub created_at:        std::time::Instant,
    /// Ritual the run was attached to (`ritual_id` on the request), if any.
    pub ritual_id:         Option<Uuid>,
    /// The anonymous Performance minted when the run joined that Ritual.
    /// Written from the blocking task right after `join` succeeds; `None` for
    /// runs without a Ritual, a failed join, and resumed runs (resume does not
    /// re-join).
    pub performance_id:    Option<Uuid>,
    /// Cycle budget the run was started with.
    pub max_cycles:        u32,
    /// Resolved (clamped) wall-clock budget in ms; `None` = unbounded.
    pub deadline_ms:       Option<u64>,
    /// Wall-clock start, for the API (`created_at` is monotonic, for the reaper).
    pub started_at_ms:     i64,
    /// Set by the finishing task, the single writer of the final state. The
    /// reaper ages terminal entries from here, not from `created_at`.
    pub completed_at:      Option<std::time::Instant>,
    pub completed_at_ms:   Option<i64>,
    /// For a resumed run: the deduction it continues.
    pub resumed_from:      Option<Uuid>,
    /// Set as soon as the `DeductionSession` is created inside `spawn_blocking`,
    /// before `run()` starts. Used to track live sessions in `active_coire_sessions`.
    pub prolog_session_id: Option<Uuid>,
    pub clips_session_id:  Option<Uuid>,
}

fn wall_clock_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl DeductionEntry {
    /// A freshly started run: `Running`, nothing completed yet.
    pub fn new_running(
        interrupt:    Arc<AtomicBool>,
        ritual_id:    Option<Uuid>,
        max_cycles:   u32,
        deadline_ms:  Option<u64>,
        resumed_from: Option<Uuid>,
    ) -> Self {
        Self {
            status:            CycleStatus::Running,
            reason:            None,
            result:            None,
            cycles:            0,
            interrupt,
            created_at:        std::time::Instant::now(),
            ritual_id,
            performance_id:    None,
            max_cycles,
            deadline_ms,
            started_at_ms:     wall_clock_ms(),
            completed_at:      None,
            completed_at_ms:   None,
            resumed_from,
            prolog_session_id: None,
            clips_session_id:  None,
        }
    }

    /// Stamp completion. Called once by the finishing task after it has
    /// written the final `status`/`reason`/`result`.
    pub fn mark_completed(&mut self) {
        self.completed_at    = Some(std::time::Instant::now());
        self.completed_at_ms = Some(wall_clock_ms());
    }

    /// The status a poll should report. `DELETE /deduce/{id}` only sets the
    /// interrupt flag (it no longer rewrites `status`), so a still-running
    /// entry whose flag is set is reported as `interrupted` here — the same
    /// immediate answer callers always got — while `status` itself stays
    /// `Running` until the task, the single writer, finalizes it.
    pub fn effective_status(&self) -> CycleStatus {
        if matches!(self.status, CycleStatus::Running)
            && self.interrupt.load(std::sync::atomic::Ordering::SeqCst)
        {
            CycleStatus::Interrupted
        } else {
            self.status.clone()
        }
    }
}

/// Application state
#[derive(Clone)]
pub struct AppState {
    pub session_manager: SessionManager,
    pub subprocess_pool: SubprocessPool,
    /// Map of deduction_id → entry for all active / completed deductions.
    pub deductions: Arc<RwLock<HashMap<Uuid, DeductionEntry>>>,
    /// Optional persistent Coire store. When `Some`, every `CycleController`
    /// will save both engine mailboxes at the end of its run.
    pub coire_store: Option<clara_cycle::CoireStore>,
    /// Session UUIDs (prolog + CLIPS) for deductions that are currently
    /// running. Read by the carrion-picker to avoid deleting live mailboxes.
    pub active_coire_sessions: Arc<RwLock<HashSet<Uuid>>>,
    /// TTL in milliseconds for [`DeductionSnapshot`] entries. Used when
    /// saving a snapshot after a `persist: true` deduction request.
    pub snapshot_ttl_ms: i64,
    /// Wall-clock deadline resolution for `/deduce` (request > server default,
    /// clamped to the server ceiling).
    pub deadline_policy: crate::deadline::DeadlinePolicy,
    /// Registry of all active Rituals. Initialized with `InMemoryBroker` until
    /// Phase 5 wires in the real `RsKafkaClient`.
    pub ritual_registry: Arc<RitualRegistry>,
    /// Dis domain identifier (e.g. "dis.local"). Used when bootstrapping
    /// FieryPit participant Kafka consumers at Ritual creation.
    pub dis_domain: String,
    /// Kafka bootstrap servers address (e.g. "localhost:9092"). Forwarded to
    /// FieryPit participants at Ritual creation so they can connect to the
    /// same broker. `None` when using the in-memory broker (tests / dev).
    pub kafka_bootstrap: Option<String>,
    /// Cached service JWT for calling `POST /ritual/join` on FieryPit
    /// participants during auto-bootstrap.  Populated lazily; invalidated on
    /// `401 Unauthorized` from any participant.
    pub fiery_pit_token_cache: Arc<Mutex<Option<CachedToken>>>,
    /// Registry of FieryPits that have registered/heartbeated with this Dis
    /// instance (`PUT /fierypits/{id}` / `GET /fierypits` / `DELETE
    /// /fierypits/{id}`). In-memory only, no CoireStore persistence —
    /// registrations are ephemeral and naturally repopulate as FieryPits
    /// heartbeat again after a restart, unlike `RitualRegistry`, which
    /// persists because a live Ritual is not something a heartbeat can
    /// recreate. Scope: registration + discovery only — NOT consulted by
    /// `activate_ritual_config`'s node-to-FieryPit resolution (lildaemon-
    /// side `partition_nodes_by_target`) or any auto-placement; that's an
    /// explicit, separate follow-up. See `docs/fierypit_registration_plan.md`.
    pub fiery_pit_registry: Arc<FieryPitRegistry>,
}

/// Periodically evicts terminal-status entries from `AppState.deductions`
/// older than `ttl`. Added 2026-08-26: this map grows unconditionally for
/// every `/deduce` call, forever — unlike the CoireStore-backed layer
/// `CarrionPicker` (clara-coire) already sweeps, which only receives data
/// when a request sets `persist: true`. Deliberately NOT part of
/// `CarrionPicker` itself: that type is scoped to `CoireStore`/DuckDB and
/// lives in the lower-level `clara-coire` crate, which shouldn't depend on
/// `clara-api`'s own `AppState`/`DeductionEntry` types.
///
/// Only entries whose `status` is NOT `CycleStatus::Running` are eligible —
/// a deduction that's still running is never evicted regardless of age
/// (mirrors `CarrionPicker`'s own "never touch anything in the active set"
/// discipline, just checked directly via `status` here instead of a
/// separate active-set, since `DeductionEntry` already carries it).
/// `created_at` is "when this entry was first inserted" (deduction start),
/// not "when it reached its terminal status" — a fine approximation given
/// typical deduction runtimes are seconds-to-minutes and TTLs here are
/// sized in hours; not worth a new field to make exact.
///
/// This is a memory-leak fix, not a durable archive — a future "pull
/// completed work" worker should read from `persist: true` deductions'
/// `DeductionSnapshot` rows (already TTL'd by `CarrionPicker` on a much
/// longer, configurable horizon) instead of this in-memory map.
pub fn spawn_deduction_reaper(
    deductions: Arc<RwLock<HashMap<Uuid, DeductionEntry>>>,
    ttl: std::time::Duration,
    interval: std::time::Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        log::info!(
            "Deduction reaper: started (ttl={}s, interval={}s)",
            ttl.as_secs(),
            interval.as_secs(),
        );
        loop {
            tokio::time::sleep(interval).await;
            let evicted = reap_deductions(&deductions, ttl);
            if evicted > 0 {
                log::info!("Deduction reaper: evicted {} completed deduction(s)", evicted);
            } else {
                log::debug!("Deduction reaper: sweep complete, nothing to evict");
            }
        }
    })
}

/// One sweep pass, factored out of `spawn_deduction_reaper`'s loop so it's
/// synchronously unit-testable (mirrors `CarrionPicker::sweep`'s own
/// separation from `CarrionPicker::spawn`'s tokio loop). Returns the number
/// of entries evicted.
fn reap_deductions(
    deductions: &Arc<RwLock<HashMap<Uuid, DeductionEntry>>>,
    ttl: std::time::Duration,
) -> usize {
    let mut map = deductions.write().unwrap();
    let before = map.len();
    map.retain(|_id, entry| {
        // Aged from completion, never from creation, and only once the task
        // has stamped it: a run still in flight (even an interrupted one whose
        // task has not finished) is never evicted.
        let expired = entry.completed_at.map_or(false, |t| t.elapsed() >= ttl);
        !expired
    });
    before - map.len()
}

/// Convert a clara-session::Session to API SessionResponse
fn session_to_response(session: &clara_session::Session) -> SessionResponse {
    SessionResponse {
        session_id: session.session_id.to_string(),
        user_id: session.user_id.clone(),
        started: format_timestamp(session.created_at),
        touched: format_timestamp(session.touched_at),
        status: session.status.to_string(),
        resources: ResourceInfo {
            facts: session.resources.facts,
            rules: session.resources.rules,
            objects: session.resources.objects,
            memory_mb: None,
        },
        limits: Some(ResourceInfo {
            facts: session.limits.max_facts,
            rules: session.limits.max_rules,
            objects: 0,
            memory_mb: Some(session.limits.max_memory_mb),
        }),
    }
}

fn format_timestamp(secs: u64) -> String {
    // Convert Unix timestamp to ISO8601 string
    chrono::DateTime::from_timestamp(secs as i64, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

/// POST /sessions - Create a new session
pub async fn create_session(
    state: web::Data<AppState>,
    req: web::Json<CreateSessionRequest>,
) -> Result<HttpResponse, ApiError> {
    log::info!("Creating session for user: {}", req.user_id);

    // Build resource limits from config if provided
    let limits = req.config.as_ref().map(|cfg| {
        clara_session::ResourceLimits {
            max_facts: cfg.max_facts.unwrap_or(1000),
            max_rules: cfg.max_rules.unwrap_or(500),
            max_memory_mb: cfg.max_memory_mb.unwrap_or(128),
        }
    });

    let session = state
        .session_manager
        .create_session_with_name(req.user_id.clone(), req.name.clone(), limits)
        .map_err(ApiError::from)?;

    let response = session_to_response(&session);
    Ok(HttpResponse::Created().json(response))
}

/// POST /sessions/{session_id}/save - Save session state (facts and rules)
pub async fn save_session(
    state: web::Data<AppState>,
    req: web::Json<SaveSessionRequest>,
) -> Result<HttpResponse, ApiError> {
    let session_id = clara_session::SessionId(req.session_id.clone());
    log::info!("Saving session: {}", req.session_id);

    state
        .session_manager
        .save_session(&session_id)
        .map_err(ApiError::from)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({"status": "saved"})))
}

/// GET /sessions/{session_id} - Get session details
pub async fn get_session(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let session_id = path.into_inner();
    log::info!("Getting session: {}", session_id);

    let session_id = clara_session::SessionId(session_id);
    let session = state
        .session_manager
        .get_session(&session_id)
        .map_err(ApiError::from)?;

    let response = session_to_response(&session);
    Ok(HttpResponse::Ok().json(response))
}

/// GET /sessions/user/{user_id} - List sessions for a user
pub async fn list_user_sessions(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let user_id = path.into_inner();
    log::info!("Listing sessions for user: {}", user_id);

    let sessions = state
        .session_manager
        .get_user_sessions(&user_id)
        .map_err(ApiError::from)?;

    let responses: Vec<SessionResponse> = sessions
        .iter()
        .map(session_to_response)
        .collect();

    Ok(HttpResponse::Ok().json(responses))
}

/// DELETE /sessions/{session_id} - Terminate a session
pub async fn terminate_session(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let session_id_str = path.into_inner();
    log::info!("Terminating session: {}", session_id_str);

    let session_id = clara_session::SessionId(session_id_str.clone());
    let session = state
        .session_manager
        .terminate_session(&session_id)
        .map_err(ApiError::from)?;

    // With transactional model, no persistent subprocess to terminate
    // Each eval spawns and cleans up its own process

    let response = TerminateResponse {
        session_id: session.session_id.to_string(),
        status: "terminated".to_string(),
        saved: false,
    };

    Ok(HttpResponse::Ok().json(response))
}

/// POST /sessions/{session_id}/rules - Load rules into a session
pub async fn load_rules(
    state: web::Data<AppState>,
    path: web::Path<String>,
    req: web::Json<LoadRulesRequest>,
) -> Result<HttpResponse, ApiError> {
    let session_id = clara_session::SessionId(path.into_inner());
    log::info!("Loading {} rules into session: {}", req.rules.len(), session_id);

    // Verify session exists
    let _session = state
        .session_manager
        .get_session(&session_id)
        .map_err(ApiError::from)?;

    // Load each rule via CLIPS environment. A "rule" string may contain a
    // single construct/expression or several (e.g. a whole .clp file's worth),
    // so split it into individual top-level forms first, then route each one
    // to build() (constructs: defrule, deffunction, ...) or eval() (plain
    // expressions like (assert ...)) based on its leading keyword.
    for rule in &req.rules {
        state
            .session_manager
            .with_clips_env(&session_id, |env| -> Result<(), String> {
                for construct in clara_clips::split_clips_constructs(rule) {
                    if clara_clips::is_construct(&construct) {
                        env.build(&construct)?;
                    } else {
                        env.eval(&construct)?;
                    }
                }
                Ok(())
            })
            .map_err(ApiError::from)?;
    }

    // Touch session to update last activity
    state
        .session_manager
        .touch_session(&session_id)
        .map_err(ApiError::from)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "rules_loaded",
        "count": req.rules.len()
    })))
}

/// POST /sessions/{session_id}/facts - Load facts into a session
pub async fn load_facts(
    state: web::Data<AppState>,
    path: web::Path<String>,
    req: web::Json<LoadFactsRequest>,
) -> Result<HttpResponse, ApiError> {
    let session_id = clara_session::SessionId(path.into_inner());
    log::info!("Loading {} facts into session: {}", req.facts.len(), session_id);

    // Verify session exists
    let _session = state
        .session_manager
        .get_session(&session_id)
        .map_err(ApiError::from)?;

    // Load each fact via CLIPS environment
    for fact in &req.facts {
        let assert_cmd = format!("(assert {})", fact);
        state
            .session_manager
            .with_clips_env(&session_id, |env| {
                env.eval(&assert_cmd)
            })
            .map_err(ApiError::from)?;
    }

    // Touch session to update last activity
    state
        .session_manager
        .touch_session(&session_id)
        .map_err(ApiError::from)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "facts_loaded",
        "count": req.facts.len()
    })))
}

/// POST /sessions/{session_id}/run - Run rules in a session
pub async fn run_rules(
    state: web::Data<AppState>,
    path: web::Path<String>,
    req: web::Json<RunRequest>,
) -> Result<HttpResponse, ApiError> {
    let session_id = clara_session::SessionId(path.into_inner());
    log::info!("Running rules in session: {} with max_iterations: {}", session_id, req.max_iterations);

    // Verify session exists
    let _session = state
        .session_manager
        .get_session(&session_id)
        .map_err(ApiError::from)?;

    let start = std::time::Instant::now();

    // Run rules via CLIPS environment. Calls the `Run()` C API directly
    // (ClipsEnvironment::run_rules) rather than eval("(run)") — the
    // CLIPS-language `run` function is void-returning, so its fire count
    // can never be recovered by parsing eval's textual output.
    let fired = state
        .session_manager
        .with_clips_env(&session_id, |env| {
            env.run_rules(req.max_iterations)
        })
        .map_err(ApiError::from)?;

    let elapsed_ms = start.elapsed().as_millis() as u64;

    let rules_fired = fired.max(0) as u64;

    // Touch session to update last activity
    state
        .session_manager
        .touch_session(&session_id)
        .map_err(ApiError::from)?;

    let response = RunResponse {
        rules_fired,
        status: "completed".to_string(),
        runtime_ms: elapsed_ms,
    };

    Ok(HttpResponse::Ok().json(response))
}

/// GET /sessions/{session_id}/facts - Query facts in a session
pub async fn query_facts(
    state: web::Data<AppState>,
    path: web::Path<String>,
    _query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse, ApiError> {
    let session_id = clara_session::SessionId(path.into_inner());

    log::info!("Querying facts in session: {}", session_id);

    // Verify session exists
    let _session = state
        .session_manager
        .get_session(&session_id)
        .map_err(ApiError::from)?;

    // Query facts via CLIPS environment. `(find-all-facts ((?f)) TRUE)` (the
    // previous command here) is invalid CLIPS syntax — the fact-set query
    // functions require at least one deftemplate name per bound variable
    // (`(<var> <template-name>+)`), and there's no "any template" wildcard,
    // so `((?f))` alone always 500s with a PRNTUTIL2 syntax error. `(facts)`
    // lists every fact (relation + slot values) across all deftemplates
    // without that restriction.
    let result = state
        .session_manager
        .with_clips_env(&session_id, |env| env.eval("(facts)"))
        .map_err(ApiError::from)?;

    // Parse result into list of facts, one per line. Drop the trailing
    // "For a total of N facts." summary line `(facts)` appends — it isn't a
    // fact.
    let matches: Vec<String> = result
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with("For a total of"))
        .map(|s| s.to_string())
        .collect();

    let count = matches.len();

    let response = QueryFactsResponse {
        matches,
        count,
    };

    Ok(HttpResponse::Ok().json(response))
}

/// GET /sessions - List all sessions
pub async fn list_all_sessions(
    state: web::Data<AppState>,
) -> Result<HttpResponse, ApiError> {
    log::info!("Listing all sessions");

    let sessions = state
        .session_manager
        .list_all_sessions()
        .map_err(ApiError::from)?;

    let responses: Vec<SessionResponse> = sessions
        .iter()
        .map(session_to_response)
        .collect();

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "sessions": responses,
        "total": responses.len()
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    #[test]
    fn test_format_timestamp() {
        let ts = 1729700580; // 2024-10-23 17:03:00 UTC
        let formatted = format_timestamp(ts);
        assert!(formatted.contains("2024-10-23"));
    }

    /// An entry whose completion (if terminal) happened `age` ago.
    fn make_entry(status: CycleStatus, age: Duration) -> DeductionEntry {
        let mut e = DeductionEntry::new_running(Arc::new(AtomicBool::new(false)), None, 10, None, None);
        e.created_at = Instant::now() - age;
        if !matches!(status, CycleStatus::Running) {
            e.completed_at = Some(Instant::now() - age);
        }
        e.status = status;
        e
    }

    #[test]
    fn test_reap_deductions_evicts_terminal_entries_past_ttl() {
        let deductions = Arc::new(RwLock::new(HashMap::new()));
        deductions
            .write()
            .unwrap()
            .insert(Uuid::new_v4(), make_entry(CycleStatus::Converged, Duration::from_secs(3600)));

        let evicted = reap_deductions(&deductions, Duration::from_secs(60));

        assert_eq!(evicted, 1);
        assert!(deductions.read().unwrap().is_empty());
    }

    #[test]
    fn test_reap_deductions_keeps_running_entries_regardless_of_age() {
        let deductions = Arc::new(RwLock::new(HashMap::new()));
        deductions
            .write()
            .unwrap()
            .insert(Uuid::new_v4(), make_entry(CycleStatus::Running, Duration::from_secs(3600)));

        let evicted = reap_deductions(&deductions, Duration::from_secs(60));

        assert_eq!(evicted, 0);
        assert_eq!(deductions.read().unwrap().len(), 1);
    }

    #[test]
    fn test_reap_deductions_keeps_terminal_entries_within_ttl() {
        let deductions = Arc::new(RwLock::new(HashMap::new()));
        deductions
            .write()
            .unwrap()
            .insert(Uuid::new_v4(), make_entry(CycleStatus::Converged, Duration::from_secs(1)));

        let evicted = reap_deductions(&deductions, Duration::from_secs(3600));

        assert_eq!(evicted, 0);
        assert_eq!(deductions.read().unwrap().len(), 1);
    }

    #[test]
    fn test_reap_deductions_evicts_error_and_interrupted_too() {
        let deductions = Arc::new(RwLock::new(HashMap::new()));
        {
            let mut map = deductions.write().unwrap();
            map.insert(
                Uuid::new_v4(),
                make_entry(CycleStatus::Error("boom".to_string()), Duration::from_secs(3600)),
            );
            map.insert(
                Uuid::new_v4(),
                make_entry(CycleStatus::Interrupted, Duration::from_secs(3600)),
            );
        }

        let evicted = reap_deductions(&deductions, Duration::from_secs(60));

        assert_eq!(evicted, 2);
        assert!(deductions.read().unwrap().is_empty());
    }

    #[test]
    fn test_reap_ages_from_completion_not_creation() {
        // Created long ago but finished just now: must survive its TTL.
        let deductions = Arc::new(RwLock::new(HashMap::new()));
        let mut e = make_entry(CycleStatus::Converged, Duration::from_secs(0));
        e.created_at = Instant::now() - Duration::from_secs(7200);
        deductions.write().unwrap().insert(Uuid::new_v4(), e);

        assert_eq!(reap_deductions(&deductions, Duration::from_secs(60)), 0);
        assert_eq!(deductions.read().unwrap().len(), 1);
    }

    #[test]
    fn test_reap_keeps_an_interrupted_entry_whose_task_has_not_finished() {
        // DELETE only sets the flag; status stays Running and there is no
        // completed_at, so the entry must never be evicted mid-flight.
        let deductions = Arc::new(RwLock::new(HashMap::new()));
        let e = make_entry(CycleStatus::Running, Duration::from_secs(7200));
        e.interrupt.store(true, std::sync::atomic::Ordering::SeqCst);
        deductions.write().unwrap().insert(Uuid::new_v4(), e);

        assert_eq!(reap_deductions(&deductions, Duration::from_secs(60)), 0);
        assert_eq!(deductions.read().unwrap().len(), 1);
    }

    #[test]
    fn test_effective_status_reports_interrupted_without_mutating_status() {
        let e = make_entry(CycleStatus::Running, Duration::from_secs(0));
        assert_eq!(e.effective_status(), CycleStatus::Running);
        e.interrupt.store(true, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(e.effective_status(), CycleStatus::Interrupted);
        assert_eq!(e.status, CycleStatus::Running, "status itself is left to the task");
    }

    #[test]
    fn test_effective_status_does_not_override_a_final_status() {
        let mut e = make_entry(CycleStatus::Converged, Duration::from_secs(0));
        e.interrupt.store(true, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(e.effective_status(), CycleStatus::Converged);
        e.status = CycleStatus::Expired;
        assert_eq!(e.effective_status(), CycleStatus::Expired);
    }

    #[test]
    fn test_mark_completed_stamps_both_clocks() {
        let mut e = make_entry(CycleStatus::Running, Duration::from_secs(0));
        assert!(e.completed_at.is_none() && e.completed_at_ms.is_none());
        e.mark_completed();
        assert!(e.completed_at.is_some());
        assert!(e.completed_at_ms.unwrap() >= e.started_at_ms);
    }
}
