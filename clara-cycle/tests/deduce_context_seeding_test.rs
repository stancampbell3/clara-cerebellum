// deduce_context_seeding_test.rs
// -------------------------------
// Regression tests for DeductionSession::seed_context / current_context/1
// (the_rabbit.pl).
//
// Bug (confirmed live 2026-09-07): seed_context only escaped single quotes
// before splicing the serialized JSON into a Prolog single-quoted atom.
// Since the JSON text itself already contains real escape sequences (\",
// \\, \n, ...) for any message with a quote, backslash, or newline — true
// of virtually any real conversational reply — SWI-Prolog's quoted-atom
// reader "resolved" those sequences while parsing the atom (e.g. turning
// `\"` into a bare `"`), corrupting the embedded JSON before
// atom_json_dict/3 ever saw it, so current_context/1 threw
// syntax_error(json(illegal_object)) instead of returning the context.
//
// A second, compounding bug: deduce_context_json/1 was declared plain
// `dynamic` rather than `thread_local`, so it lived in the global clause
// store shared by every Prolog engine in the process — one deduction's
// seeded (or corrupted) fact was never retracted and kept satisfying every
// later, unrelated deduction's current_context/1 call regardless of that
// deduction's own context, until the process restarted.

use std::sync::{Arc, Once, atomic::AtomicBool};
use clara_cycle::{CycleController, CycleStatus, DeductionSession};
use serde_json::json;

static INIT: Once = Once::new();

fn init_globals() {
    INIT.call_once(|| {
        let _ = env_logger::builder().is_test(true).try_init();
        clara_coire::init_global().expect("Failed to initialize Coire");
        clara_prolog::init_global();
    });
}

fn run_current_context(context: &[serde_json::Value]) -> serde_json::Value {
    let mut session = DeductionSession::new().expect("DeductionSession::new failed");
    session.seed_context(context).expect("seed_context failed");

    let mut controller = CycleController::new(
        session,
        10,
        Some("current_context(Ctx)".to_string()),
        Arc::new(AtomicBool::new(false)),
    );
    let result = controller.run().expect("controller.run() returned Err");
    assert_eq!(
        result.status,
        CycleStatus::Converged,
        "expected Converged, got {:?} after {} cycle(s)",
        result.status, result.cycles,
    );
    result.prolog_solutions.unwrap_or(json!([]))
}

/// A context message containing an embedded double quote must round-trip
/// through seed_context -> current_context/1 intact, not throw a JSON
/// syntax error. This is the exact shape that broke in production: a
/// prior assistant reply like `it's arguably the most critical "treatment."`
#[test]
fn context_with_embedded_double_quote_round_trips() {
    init_globals();

    let context = vec![json!({
        "role": "assistant",
        "content": "He said \"hello\" to me.",
        "timestamp": 1,
    })];

    let solutions = run_current_context(&context);
    let solutions = solutions.as_array().expect("prolog_solutions should be an array");
    assert_eq!(solutions.len(), 1, "expected exactly one solution, got {solutions:?}");

    let ctx = &solutions[0]["Ctx"];
    assert_eq!(
        ctx.as_array().map(|a| a.len()),
        Some(1),
        "expected Ctx to be bound to the one-message context, got {ctx:?}",
    );
    assert_eq!(
        ctx[0]["content"], "He said \"hello\" to me.",
        "expected the embedded double quote to survive the round trip intact, got {ctx:?}",
    );
}

/// A context message containing a literal backslash, an embedded single
/// quote (a bare Prolog atom delimiter), and a newline together — the
/// combination most likely to break naive escaping — must also round-trip.
#[test]
fn context_with_backslash_apostrophe_and_newline_round_trips() {
    init_globals();

    let context = vec![json!({
        "role": "assistant",
        "content": "Path: C:\\temp\\file. It's a \"test\".\n\nSecond paragraph.",
        "timestamp": 1,
    })];

    let solutions = run_current_context(&context);
    let solutions = solutions.as_array().expect("prolog_solutions should be an array");
    assert_eq!(solutions.len(), 1, "expected exactly one solution, got {solutions:?}");

    let ctx = &solutions[0]["Ctx"];
    assert_eq!(
        ctx[0]["content"],
        "Path: C:\\temp\\file. It's a \"test\".\n\nSecond paragraph.",
        "expected the original content to survive the round trip byte-for-byte, got {ctx:?}",
    );
}

/// deduce_context_json/1 MUST be thread_local: a deduction that seeds no
/// context of its own must never see a PRIOR, unrelated deduction's
/// context. Confirmed live 2026-09-07 that a plain `dynamic` declaration
/// let one deduction's seeded fact leak into (and, when malformed, poison)
/// every subsequent deduction in the process regardless of its own input.
#[test]
fn context_does_not_leak_across_deductions() {
    init_globals();

    // First deduction: seeds a real, non-empty context.
    let first_context = vec![json!({
        "role": "user",
        "content": "first deduction's message",
        "timestamp": 1,
    })];
    let first_solutions = run_current_context(&first_context);
    let first_ctx = &first_solutions[0]["Ctx"];
    assert_eq!(first_ctx[0]["content"], "first deduction's message");

    // Second, unrelated deduction: seeds NO context at all. It must see an
    // empty list, never the first deduction's leftover fact.
    let second_solutions = run_current_context(&[]);
    let second_ctx = &second_solutions[0]["Ctx"];
    assert_eq!(
        second_ctx.as_array().map(|a| a.len()),
        Some(0),
        "a fresh deduction with no seeded context must see current_context([]), \
         not a previous deduction's leftover context; got {second_ctx:?}",
    );
}
