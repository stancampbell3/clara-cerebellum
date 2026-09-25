//! Text handling at the CLIPS boundary (checked 2026-09-25 after the Prolog FFI turned out to read incoming text as ISO-8859-1).
//! CLIPS itself is UTF-8 clean: string functions count characters and printout/facts keep the bytes. The fault was in the Coire
//! library, whose `coire-publish` built its JSON payload without escaping.

use clara_clips::ClipsEnvironment;

const SAMPLE: &str = "a\u{2014}b caf\u{e9} \u{65e5}\u{672c} \u{1F419}";

fn env() -> ClipsEnvironment {
    let mut env = ClipsEnvironment::new().expect("env");
    env.load_coire_library().expect("coire library");
    env
}

#[test]
fn clips_counts_characters_and_keeps_utf8_text() {
    let mut env = env();
    assert_eq!(env.eval(&format!("(str-length \"{SAMPLE}\")")).unwrap(), "13");
    assert_eq!(env.eval(&format!("(str-cat \"{SAMPLE}\" \"!\")")).unwrap(), format!("\"{SAMPLE}!\""));
    assert_eq!(env.eval(&format!("(sub-string 1 3 \"{SAMPLE}\")")).unwrap(), "\"a\u{2014}b\"");
    assert!(env.eval(&format!("(printout t \"{SAMPLE}\" crlf)")).unwrap().contains(SAMPLE));
}

#[test]
fn a_long_multibyte_string_is_not_split_or_dropped_on_output() {
    let mut env = env();
    let long: String = std::iter::repeat('\u{2014}').take(5000).collect();
    let out = env.eval(&format!("(printout t \"{long}\" crlf)")).unwrap();
    assert_eq!(out.trim(), long);
}

#[test]
fn a_fact_keeps_its_non_ascii_text() {
    let mut env = env();
    env.eval(&format!("(assert (moji \"{SAMPLE}\"))")).unwrap();
    assert!(env.eval("(facts)").unwrap().contains(SAMPLE));
}

#[test]
fn coire_json_escape_escapes_what_json_forbids_and_nothing_else() {
    let mut env = env();
    // quote and backslash
    assert_eq!(env.eval(r#"(coire-json-escape "a\"b\\c")"#).unwrap(), r#""a\"b\\c""#);
    // a newline and a tab become \n and \t; the letter t (which CLIPS would also read from a backslash-t escape) is untouched
    assert_eq!(env.eval("(coire-json-escape (format nil \"x%ny\"))").unwrap(), r#""x\ny""#);
    assert_eq!(env.eval("(coire-json-escape \"a\tb\")").unwrap(), r#""a\tb""#); // a real TAB character
    assert_eq!(env.eval("(coire-json-escape \"attest\")").unwrap(), "\"attest\"");
    // non-ASCII text passes through whole
    assert_eq!(env.eval(&format!("(coire-json-escape \"{SAMPLE}\")")).unwrap(), format!("\"{SAMPLE}\""));
}

#[test]
fn coire_publish_emits_valid_json_for_a_goal_with_quotes() {
    // Before the escaping fix this payload was invalid JSON, so coire-emit dropped the event without an error.
    let mut env = env();
    env.eval("(bind ?*coire-session-id* \"00000000-0000-0000-0000-000000000001\")").unwrap();
    let payload = env
        .eval(r#"(str-cat "{\"type\":\"goal\",\"data\":\"" (coire-json-escape "assertz(echoed(\"x\"))") "\"}")"#)
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(payload.trim_matches('"').replace("\\\\", "\\").as_str())
        .unwrap_or_else(|e| panic!("payload is not valid JSON: {e}: {payload}"));
    assert_eq!(json["data"], "assertz(echoed(\"x\"))");
}
