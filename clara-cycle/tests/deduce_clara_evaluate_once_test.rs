// deduce_clara_evaluate_once_test.rs
// ------------------------------------
// Integration tests: clara_evaluate/2 FFI must be executed exactly ONCE per
// deduction run even when multiple predicates (echo1, echo2) each call it
// with the same JSON query string.
//
// Memoization is provided by a Rust-level result cache in clara-toolbox::ffi.
//
// Scenario:
//   echo1(R1) :- Q = '{"tool":"echo","arguments":{"message":"startup test"}}',
//                clara_evaluate(Q, R1).
//   echo2(R2) :- Q = '{"tool":"echo","arguments":{"message":"startup test"}}',
//                clara_evaluate(Q, R2).
//   duh_dun :- echo1(_), echo2(_).
//
// Expected convergence path (goal "duh_dun", no trailing period):
//   Cycle 0: duh_dun → echo1 → clara_evaluate (cache miss, count=1, cached)
//                    → echo2 → clara_evaluate (cache hit,  count=1)
//            duh_dun succeeds.
//            echo1/echo2 are rules (not dynamic assertz), so no Coire events
//            are published → both mailboxes empty → CLIPS fires nothing →
//            agenda empty → root_goal_resolved → CONVERGED in 1 cycle.

use std::sync::{Arc, Mutex, Once, atomic::AtomicBool};
use clara_cycle::{CycleController, CycleStatus, DeductionSession, TruthValue};
use clara_toolbox::{
    ToolboxManager,
    ffi::{clear_evaluate_cache, get_evaluate_call_count, reset_evaluate_call_count},
};

static INIT: Once = Once::new();

// The evaluate call counter and cache are process-global statics. With the
// cache namespaced per deduction, each test's runs execute for real — so
// tests asserting exact counts must not run concurrently. (Before the
// per-deduction scoping they were accidentally isolated: every test hit the
// same shared cache entry.)
static COUNT_TEST_LOCK: Mutex<()> = Mutex::new(());

fn lock_counts() -> std::sync::MutexGuard<'static, ()> {
    // Recover from poison: a panicked test already reported its failure; the
    // next test resets the counter and cache itself.
    COUNT_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn init_globals() {
    INIT.call_once(|| {
        let _ = env_logger::builder().is_test(true).try_init();
        clara_coire::init_global().expect("Failed to initialize Coire");
        clara_prolog::init_global();
        ToolboxManager::init_global(); // registers echo tool used by clara_evaluate
    });
}

/// Build a fresh session loaded with the evaluate-once test resources.
fn make_session(manifest: &str) -> DeductionSession {
    let pl_path  = format!("{manifest}/tests/resources/deduce_clara_evaluate_once_test_clara.pl");
    let clp_path = format!("{manifest}/tests/resources/deduce_clara_evaluate_once_test_clara.clp");

    let pl_source = std::fs::read_to_string(&pl_path)
        .unwrap_or_else(|e| panic!("cannot read {pl_path}: {e}"));

    let mut session = DeductionSession::new()
        .expect("DeductionSession::new failed");
    session.seed_prolog(&[pl_source]).expect("seed_prolog failed");
    session.seed_clips_file(&clp_path).expect("seed_clips_file failed");
    session
}

// ── Primary test ──────────────────────────────────────────────────────────────

/// Full end-to-end deduction: duh_dun converges in one cycle and
/// clara_evaluate is invoked exactly once despite two predicates calling it
/// with identical arguments (echo2 is served from cache).
#[test]
fn clara_evaluate_called_once_per_deduction() {
    init_globals();
    let _count_guard = lock_counts();
    reset_evaluate_call_count();
    clear_evaluate_cache();

    let manifest = env!("CARGO_MANIFEST_DIR");
    let mut controller = CycleController::new(
        make_session(manifest),
        5,
        Some("duh_dun".to_string()),
        Arc::new(AtomicBool::new(false)),
    );

    let result = controller.run()
        .expect("controller.run() returned Err — max cycles exceeded without convergence");

    // 1. The cycle must converge.
    assert_eq!(
        result.status,
        CycleStatus::Converged,
        "expected Converged, got {:?} after {} cycle(s)",
        result.status, result.cycles,
    );

    // 2. duh_dun must be KnownTrue in the tableau.
    let tableau = result.tableau
        .expect("DeductionResult.tableau should be Some after convergence");

    assert!(
        tableau.iter().any(|e| e.functor == "duh_dun" && e.truth_value == TruthValue::KnownTrue),
        "expected duh_dun to be KnownTrue in the tableau after convergence.\n\
         duh_dun entries in tableau: {:#?}",
        tableau.iter().filter(|e| e.functor == "duh_dun").collect::<Vec<_>>(),
    );

    // 3. The FFI tool must have been invoked exactly once: echo1 triggers a
    //    real execution (cache miss), echo2 is served from the result cache.
    assert_eq!(
        get_evaluate_call_count(), 1,
        "expected clara_evaluate FFI execution count=1 (echo2 memoised), \
         but it was called {} time(s)",
        get_evaluate_call_count(),
    );
}

// ── Per-deduction cache scoping ───────────────────────────────────────────────

/// The evaluate cache is namespaced by deduction id (docs/
/// typed_edges_followups.md #2): a second deduction issuing the identical
/// request must RE-EXECUTE the FFI, never be served the previous
/// deduction's entry — stale LLM answers and side-effecting operations
/// (set_evaluator) must not leak across runs. Within each deduction the
/// once-only memoization still holds (echo2 stays a cache hit — asserted
/// implicitly by the +1-per-run counts here and explicitly by the test
/// above). `clear_evaluate_cache` coverage lives in the clara-toolbox ffi
/// unit tests.
#[test]
fn clara_evaluate_cache_scoped_per_deduction() {
    init_globals();
    let _count_guard = lock_counts();

    let manifest = env!("CARGO_MANIFEST_DIR");

    // First deduction: cold cache → exactly 1 real FFI call.
    reset_evaluate_call_count();
    clear_evaluate_cache();
    CycleController::new(
        make_session(manifest),
        5,
        Some("duh_dun".to_string()),
        Arc::new(AtomicBool::new(false)),
    )
    .run()
    .expect("first run failed");
    let count_after_first = get_evaluate_call_count();

    // Second deduction WITHOUT clearing: its own namespace → 1 more call.
    CycleController::new(
        make_session(manifest),
        5,
        Some("duh_dun".to_string()),
        Arc::new(AtomicBool::new(false)),
    )
    .run()
    .expect("second run failed");
    let count_after_second = get_evaluate_call_count();

    assert_eq!(
        count_after_first, 1,
        "first run: expected 1 FFI call (echo2 memoised), got {}", count_after_first,
    );
    assert_eq!(
        count_after_second, 2,
        "a fresh deduction must re-execute rather than reuse the previous \
         deduction's cached result; expected 2 total FFI calls, got {}",
        count_after_second,
    );
}
