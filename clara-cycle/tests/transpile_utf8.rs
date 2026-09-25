//! The Prolog <-> CLIPS transpiler must carry non-ASCII text through unchanged (found 2026-09-25: it read strings byte by byte, so an
//! em dash became three garbage characters; `café(x)` was silently cut to `(caf)`; and anything after the term was ignored).

use clara_cycle::transpile::{clips_fact_to_prolog, prolog_to_clips_fact, prolog_to_clips_retract};

#[test]
fn a_prolog_string_keeps_its_non_ascii_characters() {
    assert_eq!(
        prolog_to_clips_fact(r#"greeting("a—b café 日本")"#).unwrap(),
        r#"(greeting "a—b café 日本")"#
    );
}

#[test]
fn a_quoted_prolog_atom_keeps_its_non_ascii_characters() {
    // a quoted atom that is also a valid CLIPS symbol becomes a bare symbol (as 'bob' -> bob) and converts back to a quoted atom
    assert_eq!(prolog_to_clips_fact("parent('José', bob)").unwrap(), "(parent José bob)");
    assert_eq!(clips_fact_to_prolog("(parent José bob)").unwrap(), "parent('José',bob)");
    assert_eq!(prolog_to_clips_fact("says(bob, 'a—b')").unwrap(), r#"(says bob "a—b")"#);
}

#[test]
fn escapes_still_work_around_non_ascii_text() {
    assert_eq!(
        prolog_to_clips_fact(r#"quote("he said \"hi — ok\"")"#).unwrap(),
        r#"(quote "he said \"hi — ok\"")"#
    );
}

#[test]
fn an_unquoted_atom_may_contain_and_start_with_non_ascii_letters() {
    assert_eq!(prolog_to_clips_fact("café(x)").unwrap(), "(café x)");
    assert_eq!(prolog_to_clips_fact("note(日本)").unwrap(), "(note 日本)");
    assert_eq!(prolog_to_clips_fact("naïve_fact(été)").unwrap(), "(naïve_fact été)");
}

#[test]
fn a_variable_may_start_with_a_non_ascii_capital() {
    assert_eq!(prolog_to_clips_fact("p(Émile)").unwrap(), "(p ?Émile)");
}

#[test]
fn text_after_the_term_is_an_error_not_silently_dropped() {
    assert!(prolog_to_clips_fact("foo(bar) baz").is_err());
    assert!(prolog_to_clips_fact("foo(bar), baz(x)").is_err());
    // one closing full stop is ordinary Prolog syntax
    assert_eq!(prolog_to_clips_fact("foo(bar).").unwrap(), "(foo bar)");
}

#[test]
fn a_clips_string_keeps_its_non_ascii_characters() {
    assert_eq!(
        clips_fact_to_prolog(r#"(greeting "a—b café 日本")"#).unwrap(),
        r#"greeting("a—b café 日本")"#
    );
    assert_eq!(clips_fact_to_prolog("(note 日本 \"x—y\")").unwrap(), "note(日本,\"x—y\")");
}

#[test]
fn a_retract_pattern_keeps_its_non_ascii_characters() {
    let out = prolog_to_clips_retract(r#"greeting("a—b café")"#).unwrap();
    assert!(out.contains(r#""a—b café""#), "{out}");
}

#[test]
fn a_string_round_trips_prolog_to_clips_and_back() {
    let clips = prolog_to_clips_fact(r#"m("日本 — café 🐙")"#).unwrap();
    assert_eq!(clips_fact_to_prolog(&clips).unwrap(), r#"m("日本 — café 🐙")"#);
}
