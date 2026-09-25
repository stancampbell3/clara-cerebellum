// Non-ASCII text must survive a full Prolog -> CLIPS -> Prolog deduction cycle unchanged (found 2026-09-25: the transpiler read
// strings byte by byte, and Prolog read incoming text as ISO-8859-1, so an em dash arrived as several garbage characters).
//
//   Cycle 0: note("…") relays to CLIPS (through the transpiler).
//   Cycle 1: CLIPS builds assertz(echoed("…")) from the note's text and publishes it back as a goal.
//   Cycle 2: Prolog asserts echoed("…"); the cycle converges.

use std::sync::{atomic::AtomicBool, Arc, Once};

use clara_cycle::{CycleController, CycleStatus, DeductionSession};

static INIT: Once = Once::new();

fn init_globals() {
    INIT.call_once(|| {
        let _ = env_logger::builder().is_test(true).try_init();
        clara_coire::init_global().expect("Failed to initialize Coire");
        clara_prolog::init_global();
    });
}

const TEXT: &str = "a\u{2014}b caf\u{e9} \u{65e5}\u{672c} \u{1F419}";

#[test]
fn non_ascii_text_round_trips_through_the_prolog_clips_relay() {
    init_globals();
    let manifest = env!("CARGO_MANIFEST_DIR");
    let pl = std::fs::read_to_string(format!("{manifest}/tests/resources/utf8_test_clara.pl")).expect("pl");
    let clp = format!("{manifest}/tests/resources/utf8_test_clara.clp");

    let mut session = DeductionSession::new().expect("session");
    session.seed_prolog(&[pl]).expect("seed_prolog");
    session.seed_clips_file(&clp).expect("seed_clips_file");
    session.prolog.assertz(&format!("note(\"{TEXT}\")")).expect("assertz note");

    let interrupt = Arc::new(AtomicBool::new(false));
    let mut controller = CycleController::new(session, 6, Some("echoed(X)".to_string()), interrupt);
    let result = controller.run().expect("the deduction did not converge");
    assert_eq!(result.status, CycleStatus::Converged, "got {:?} after {} cycle(s)", result.status, result.cycles);

    let tableau = result.tableau.expect("tableau");
    let echoed: Vec<_> = tableau.iter().filter(|e| e.functor == "echoed").collect();
    assert!(!echoed.is_empty(), "no echoed/1 entry reached the tableau: {:#?}", tableau);
    assert!(
        echoed.iter().any(|e| e.args.iter().any(|a| a.contains(TEXT))),
        "the text did not survive the round trip unchanged: {:#?}",
        echoed
    );
}
