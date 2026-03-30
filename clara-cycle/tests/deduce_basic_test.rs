// deduce_basic_test.rs
// --------------------
// Integration test: run the clara cycle controller through a full
// Prolog → relay → CLIPS → relay → convergence loop.
//
// The scenario:
//   - Prolog source: deduce_basic_test_clara.pl  (omelette recipe rules)
//   - CLIPS source:  deduce_basic_test_clara.clp (transduced CLIPS rules)
//   - Dynamic facts asserted: visitor(bob), egg(unbroken)
//   - Initial goal: omelette(bob, X)
//
// Expected cycle path:
//   Cycle 0: omelette(bob,X) fails (egg(broken) not yet derived).
//            visitor(bob) and egg(unbroken) relay to CLIPS.
//   Cycle 1: CLIPS fires transduced-break_some-on-egg-0 → publishes
//            goal "break_some" back to Prolog via Coire.
//            Prolog's coire_consume executes break_some →
//            assertz(egg(broken)) → new Coire event relayed to CLIPS.
//   Cycle 2: No new assertions produced; both mailboxes drain to zero;
//            CLIPS agenda empties → fixed point → CONVERGED.
//
// The controller must converge within 3 cycles.

use std::sync::{Arc, Once, atomic::AtomicBool};
use clara_cycle::{CycleController, CycleStatus, DeductionSession, TruthValue};

static INIT: Once = Once::new();

fn init_globals() {
    INIT.call_once(|| {
        let _ = env_logger::builder().is_test(true).try_init();
        clara_coire::init_global().expect("Failed to initialize Coire");
        clara_prolog::init_global();
    });
}

/// Full end-to-end deduction cycle: omelette(bob, X) converges after the
/// break_some rule fires via the Prolog ↔ CLIPS relay.
#[test]
fn deduce_basic_converges() {
    init_globals();

    let manifest = env!("CARGO_MANIFEST_DIR");
    let pl_path  = format!("{}/tests/resources/deduce_basic_test_clara.pl",  manifest);
    let clp_path = format!("{}/tests/resources/deduce_basic_test_clara.clp", manifest);

    // --- Seed the session ---------------------------------------------------

    let pl_source = std::fs::read_to_string(&pl_path)
        .unwrap_or_else(|e| panic!("cannot read {pl_path}: {e}"));

    let mut session = DeductionSession::new()
        .expect("DeductionSession::new failed");

    // Load the Prolog rules + Clara integration hooks.
    session.seed_prolog(&[pl_source])
        .expect("seed_prolog failed");

    // Assert the two dynamic predicates that drive the deduction.
    // These fire prolog_listen hooks, which publish Coire events that
    // relay_prolog_to_clips will forward to CLIPS in cycle 0.
    session.prolog.assertz("visitor(bob)")
        .expect("assertz visitor(bob) failed");
    session.prolog.assertz("egg(unbroken)")
        .expect("assertz egg(unbroken) failed");

    // Load the transduced CLIPS rules.
    session.seed_clips_file(&clp_path)
        .expect("seed_clips_file failed");

    // --- Run the controller -------------------------------------------------

    let interrupt = Arc::new(AtomicBool::new(false));
    let mut controller = CycleController::new(
        session,
        5,                                     // max_cycles — must converge within
        Some("omelette(bob, X)".to_string()),  // initial goal
        interrupt,
    );

    let result = controller.run()
        .expect("controller.run() returned Err — max cycles exceeded without convergence");

    // --- Assertions ---------------------------------------------------------

    // 1. The cycle must converge (not time out or be interrupted).
    assert_eq!(
        result.status,
        CycleStatus::Converged,
        "expected Converged, got {:?} after {} cycle(s)",
        result.status,
        result.cycles,
    );

    // 2. break_some must have fired: egg(broken) must be KnownTrue in the
    //    tableau, proving that the Prolog→CLIPS→Prolog relay worked end-to-end.
    let tableau = result.tableau
        .expect("DeductionResult.tableau should be Some after convergence");

    let egg_broken_entry = tableau.iter().find(|e| {
        e.functor == "egg"
            && e.args.len() == 1
            && e.args[0] == "broken"
            && e.truth_value == TruthValue::KnownTrue
    });

    assert!(
        egg_broken_entry.is_some(),
        "expected egg(broken) to be KnownTrue in the tableau after convergence.\n\
         egg entries in tableau: {:#?}",
        tableau.iter()
            .filter(|e| e.functor == "egg")
            .collect::<Vec<_>>(),
    );

    // Our initial goal should also be KnownTrue with Dish bound to lovely_fluffy_goodness
    let omelette_entry = tableau.iter().find(|e| {
        e.functor == "omelette"
            && e.args.len() == 2
            && e.args[0] == "bob"
            && e.truth_value == TruthValue::KnownTrue
    });

    assert!(
        omelette_entry.is_some(),
        "expected omelette(bob, X) to be KnownTrue in the tableau after convergence.\n\"
         omelette entries in tableau: {:#?}",
        tableau.iter()
            .filter(|e| e.functor == "omelette")
            .collect::<Vec<_>>(),
    )
}
