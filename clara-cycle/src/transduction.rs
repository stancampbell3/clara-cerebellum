//! Prolog → CLIPS transduction: speculative forward-chaining rule generation,
//! DOT graph visualization, and per-phase tableau coloring.
//!
//! # Transduction
//!
//! For each body goal in a Prolog rule, emits a CLIPS `defrule` that fires
//! when the corresponding fact is asserted into CLIPS and publishes the head
//! goal back to Prolog via `coire-publish-goal`.
//!
//! **Positive** goals `foo(X)` trigger on `(foo ?X)`.
//! **Negative** goals `\+ foo(X)` / `not(foo(X))` trigger on `(not_foo ?X)` —
//! a positive CLIPS fact representing a constructively-determined negation
//! relayed from Prolog.
//!
//! ## Example
//!
//! Prolog source:
//! ```prolog
//! fire(Where) :- smoke(Where).
//! lemonade(Drink) :- sour(Drink), sweet(Drink).
//! ```
//!
//! Generated CLIPS:
//! ```clips
//! (defrule transduced-fire-on-smoke-0
//!     (smoke ?Where)
//!     =>
//!     (coire-publish-goal (str-cat "fire(" ?Where ")")))
//!
//! (defrule transduced-lemonade-on-sour-1
//!     (sour ?Drink)
//!     =>
//!     (coire-publish-goal (str-cat "lemonade(" ?Drink ")")))
//!
//! (defrule transduced-lemonade-on-sweet-2
//!     (sweet ?Drink)
//!     =>
//!     (coire-publish-goal (str-cat "lemonade(" ?Drink ")")))
//! ```
//!
//! # DOT graph generation
//!
//! [`generate_dot`] converts a parsed `&[PrologRule]` into a Graphviz DOT
//! string that visualizes the rule/fact dependency graph.  It accepts an
//! optional [`NodeColoring`] to overlay per-node truth values from a Dagda
//! tableau snapshot, enabling step-by-step trace visualization.
//!
//! Layout: `rankdir=LR` — rule heads appear to the left of their conditions.
//!
//! ## Node types
//!
//! | Shape | Fill (default) | Meaning |
//! |-------|---------------|---------|
//! | Ellipse | `#d4edda` (green) | Bare fact — no body goals |
//! | Box | `#cfe2ff` (blue) | Rule head with at least one body goal |
//! | Dashed ellipse | `#fff3cd` (amber) | Leaf condition — not bridged to another head |
//!
//! ## Edge types
//!
//! | Style | Color | Label | Meaning |
//! |-------|-------|-------|---------|
//! | Solid | Black | `requires` | Rule head → leaf condition |
//! | Solid | Blue | *(none)* | Assert-bridge: head A → head B when A's condition is asserted by B |
//! | Dashed | Blue | `chains-to` | Condition → rule head whose functor/arity it matches |
//! | Dashed | Gray | `satisfies` | Fact → condition whose functor/arity it matches |
//! | Dashed | Gray | *(none, undirected)* | Shared-condition link (when `opts.link_shared_conditions = true`) |
//!
//! ## Truth-value color palette
//!
//! When a [`NodeColoring`] is supplied (derived from a tableau via
//! [`coloring_from_entries`]), structural fill colors are replaced by:
//!
//! | Truth value | Fill |
//! |-------------|------|
//! | `KnownTrue` | `#28a745` (green) |
//! | `KnownFalse` | `#dc3545` (red) |
//! | `KnownUnresolved` | `#ffc107` (amber — mixed or conflict) |
//! | `Unknown` | `#adb5bd` (gray) |
//!
//! Nodes absent from the tableau keep their structural defaults.
//!
//! # Trace visualization pipeline
//!
//! The intended usage for per-phase reasoning traces:
//!
//! 1. **Register** Prolog source via `POST /source`.
//! 2. **Run** a deduction with `trace: true` — each phase records a
//!    `tableau_changes` row in the Coire store.
//! 3. **List** phases via `GET /deduce/{id}/trace`.
//! 4. **Fetch DOT** for a phase via `GET /deduce/{id}/trace/{change_id}/dot`.
//!    The handler calls [`coloring_from_entries`] on the stored
//!    [`PredicateEntry`] slice and passes the resulting [`NodeColoring`] to
//!    [`generate_dot`].  The `"parsed_rules"` artifact (JSON-serialized
//!    `Vec<PrologRule>`) is cached in `source_artifacts` after the first call —
//!    subsequent requests skip re-parsing.
//! 5. **Render** the DOT string with `@viz-js/viz` (Cobbler GUI) or any
//!    Graphviz-compatible renderer.
//!
//! # Serialization
//!
//! [`PrologRule`] and its constituent types ([`BodyGoal`], [`Term`]) derive
//! `serde::Serialize` / `serde::Deserialize`, enabling JSON round-trips for the
//! `"parsed_rules"` source artifact cache.  The cache avoids re-parsing source
//! text on every trace request.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::transpile::{render_clips_fact, render_clips_field, render_prolog_term, Term};
use clara_dagda::TruthValue;

// ── Public types ──────────────────────────────────────────────────────────────

/// A single goal in a Prolog rule body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BodyGoal {
    /// A positive condition — can trigger a CLIPS defrule.
    Positive(Term),
    /// A negation-as-failure condition (`\+` / `not/1`).
    /// Generates a CLIPS defrule triggered by a `not_<functor>` fact —
    /// a positive assertion of a constructively-determined negation.
    Negative(Term),
}

/// A parsed Prolog rule: `head :- body.` or a bare fact `head.`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrologRule {
    pub head: Term,
    pub body: Vec<BodyGoal>,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Parse Prolog rules from source text.
///
/// `%` line-comments and blank lines are skipped. Clauses that fail to parse
/// are silently skipped (the parser advances to the next `.`).
pub fn parse_prolog_rules(source: &str) -> Vec<PrologRule> {
    let mut parser = RuleParser::new(source);
    let mut rules = Vec::new();
    loop {
        match parser.parse_clause() {
            Ok(Some(rule)) => rules.push(rule),
            Ok(None) => break,
            Err(_) => parser.skip_to_period(),
        }
    }
    rules
}

/// Generate CLIPS `defrule`s from parsed Prolog rules.
///
/// For each body goal in each rule, one defrule is emitted:
///
/// - **Positive** goal `foo(X, bar)` → LHS `(foo ?X bar)`
/// - **Negative** goal `\+ foo(X, bar)` / `not(foo(X, bar))` → LHS `(not_foo ?X bar)`
///
/// The `not_<functor>` pattern expects the relay to assert a positive CLIPS
/// fact whenever Prolog constructively proves the negation. This lets CLIPS
/// forward-chain on definite negative evidence rather than discarding it.
///
/// Variables bound by the LHS become CLIPS variable references in the `str-cat`
/// RHS; unbound head variables are emitted as literal strings.
/// Facts (rules with empty bodies) are silently skipped.
///
/// Rules where the trigger does not bind all variables in the head are also
/// skipped — the generated goal would contain free Prolog variables, which
/// causes instantiation errors when Prolog attempts to call them.
pub fn transduce(rules: &[PrologRule]) -> String {
    let mut out = String::new();
    let mut counter = 0usize;

    for rule in rules {
        if rule.body.is_empty() {
            continue;
        }

        let head_functor = term_functor_name(&rule.head);
        let head_vars = collect_vars(&rule.head);

        for goal in &rule.body {
            match goal {
                BodyGoal::Negative(trigger) => {
                    let rule_name = format!(
                        "transduced-{}-on-not_{}-{}",
                        head_functor,
                        term_functor_name(trigger),
                        counter,
                    );
                    counter += 1;

                    let bound_vars = collect_vars(trigger);
                    // Skip if any head variable would be free in the generated goal.
                    if !head_vars.is_subset(&bound_vars) {
                        continue;
                    }
                    let lhs = render_not_clips_pattern(trigger);
                    let rhs_expr = render_head_goal_expr(&rule.head, &bound_vars);
                    let comment = format_rule_comment(rule);

                    out.push_str(&format!(
                        "; Transduced from: {}\n(defrule {}\n    {}\n    =>\n    (coire-publish-goal {}))\n\n",
                        comment, rule_name, lhs, rhs_expr
                    ));
                }
                BodyGoal::Positive(trigger) => {
                    let effective = match effective_trigger(trigger) {
                        Some(t) => t,
                        None => continue, // skip meta-predicate, no counter increment
                    };

                    let rule_name = format!(
                        "transduced-{}-on-{}-{}",
                        head_functor,
                        term_functor_name(effective),
                        counter,
                    );
                    counter += 1;

                    let bound_vars = collect_vars(effective);
                    // Skip if any head variable would be free in the generated goal.
                    if !head_vars.is_subset(&bound_vars) {
                        continue;
                    }
                    let lhs = render_clips_fact(effective);
                    let rhs_expr = render_head_goal_expr(&rule.head, &bound_vars);
                    let comment = format_rule_comment(rule);

                    out.push_str(&format!(
                        "; Transduced from: {}\n(defrule {}\n    {}\n    =>\n    (coire-publish-goal {}))\n\n",
                        comment, rule_name, lhs, rhs_expr
                    ));
                }
            }
        }
    }

    out
}

/// Full pipeline: Prolog source text → CLIPS defrule source text.
pub fn transduce_source(prolog_source: &str) -> String {
    transduce(&parse_prolog_rules(prolog_source))
}

/// Full pipeline: prepend Clara integration preamble to Prolog source.
///
/// Scans the source for `:- dynamic(Functor/Arity).` directives, then prepends:
///
/// 1. A `:- prolog_listen(Functor/Arity, updated(Functor/Arity)).` directive for
///    each dynamic predicate found.
/// 2. The `updated/3` rule that relays asserted facts to CLIPS via
///    `coire_publish_assert`.
/// 3. Comment delimiters marking the generated block.
///
/// The original source is appended unchanged after the preamble.
pub fn decorate_source(prolog_source: &str) -> String {
    let indicators = extract_dynamic_indicators(prolog_source);
    let rules = parse_prolog_rules(prolog_source);
    let synthetic_groups = detect_multi_clause_groups(&rules, &indicators);
    let preamble = generate_listen_preamble(&indicators, &synthetic_groups);
    let imports = missing_module_imports(prolog_source);
    format!("{}{}\n{}", imports, preamble, prolog_source)
}

/// Standard Clara library imports (`the_rabbit`, `the_rat`, `the_coire`)
/// not already present in the source — prepended by `decorate_source` so
/// hand-authored/transduced rules can always reach `ponder_text_with_context/3`,
/// `clara_fy/3`, `current_context/1`, `caws_consult/4`, etc.
fn missing_module_imports(prolog_source: &str) -> String {
    let mut out = String::new();
    for lib in ["the_rabbit", "the_rat", "the_coire"] {
        let directive = format!("use_module(library({}))", lib);
        if !prolog_source.contains(&directive) {
            out.push_str(&format!(":- {}.\n", directive));
        }
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

// ── Graph (edge) transduction ─────────────────────────────────────────────────

/// Generated Prolog/CLIPS snippets for one graph node.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct NodeSnippets {
    pub prolog: String,
    pub clips: String,
}

/// Per-node code generated from a Ritual graph's edges.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct GraphTransduction {
    /// Keyed by graph node id.
    pub per_node: HashMap<String, NodeSnippets>,
}

/// Generate per-node Prolog/CLIPS from a Ritual graph's edges
/// (docs/deduction_redux.md: an edge is an originating push from the source
/// and an asynchronous event on the target).
///
/// Input is the Cobbler `graph_layout` JSON: `nodes` with
/// `id`/`type`/`evaluatorName`/`label`/`prologSource`/`clipsSource`, `edges`
/// with `id`/`source`/`target`/`msgType` (or legacy `envelopeLabel`)/
/// `qualifierKind`/`qualifierValue`/`topicSuffix`.
///
/// For each `offering` edge S → T between evaluator-bearing daemon nodes:
///
/// - **S Prolog**: a `consult_<target>/2` helper wrapping
///   `caws_consult/4` — addressed to T's node id on logical channel
///   `consults/<edge-id>` (or the edge's `topicSuffix`). A `boolean`
///   qualifier compiles to a guard on the helper: a natural-language value
///   (contains whitespace) goes through `clara_fy/2`, anything else is
///   spliced as a literal Prolog goal.
/// - **S CLIPS**: a reaction rule matching the correlated reply
///   `(coire-event (origin "ritual/hohi") (topic ...))` that asserts
///   `edge_replied('<edge-id>')` into Prolog — a forward-chaining hook.
/// - **T Prolog** (an `assertion` qualifier): the declared fact is emitted
///   as a ground clause in T's snippet, seeding its deductions with the
///   edge's "dynamic incoming declaration".
///
/// Edges with other message types are recorded as comments only (their
/// runtime semantics ride on the envelope label; no code is generated yet).
pub fn transduce_graph(graph_json: &str) -> Result<GraphTransduction, String> {
    let graph: serde_json::Value =
        serde_json::from_str(graph_json).map_err(|e| format!("invalid graph JSON: {}", e))?;

    let nodes: Vec<&serde_json::Value> = graph
        .get("nodes")
        .and_then(|n| n.as_array())
        .map(|a| {
            a.iter()
                .filter(|n| {
                    n.get("type").and_then(|t| t.as_str()) == Some("daemon")
                        && n.get("evaluatorName").and_then(|e| e.as_str()).is_some()
                })
                .collect()
        })
        .unwrap_or_default();
    let node_by_id: HashMap<&str, &serde_json::Value> = nodes
        .iter()
        .filter_map(|n| n.get("id").and_then(|i| i.as_str()).map(|id| (id, *n)))
        .collect();

    let mut result = GraphTransduction::default();
    let empty = Vec::new();
    let edges = graph
        .get("edges")
        .and_then(|e| e.as_array())
        .unwrap_or(&empty);

    for edge in edges {
        let str_field =
            |key: &str| edge.get(key).and_then(|v| v.as_str()).map(|s| s.to_string());
        let (Some(edge_id), Some(source_id), Some(target_id)) =
            (str_field("id"), str_field("source"), str_field("target"))
        else {
            continue;
        };
        if source_id == target_id {
            continue;
        }
        let (Some(_source), Some(target)) = (
            node_by_id.get(source_id.as_str()),
            node_by_id.get(target_id.as_str()),
        ) else {
            continue; // edge into/out of a non-evaluator node — nothing to run
        };

        let msg_type = str_field("msgType")
            .or_else(|| str_field("envelopeLabel"))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "offering".to_string());

        let target_label = target
            .get("label")
            .and_then(|l| l.as_str())
            .filter(|l| !l.is_empty())
            .unwrap_or(target_id.as_str());
        let qualifier_kind = str_field("qualifierKind").unwrap_or_else(|| "none".into());
        let qualifier_value = str_field("qualifierValue").unwrap_or_default();
        // Absent = manual: legacy graphs keep the consult-helper-only behavior.
        let pipe_mode = str_field("pipeMode")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "manual".to_string());

        // A boolean qualifier compiles to a guard goal shared by the source
        // helper (consult/emit) and, in auto mode, the pipe/tee wrapper.
        let guard_goal = match (qualifier_kind.as_str(), qualifier_value.trim()) {
            ("boolean", guard) if !guard.is_empty() => {
                Some(if guard.split_whitespace().count() > 1 {
                    // Natural-language claim — classify via Clara LLM.
                    format!("clara_fy(\"{}\", true)", guard.replace('"', "\\\""))
                } else {
                    guard.trim_end_matches('.').to_string()
                })
            }
            _ => None,
        };
        let rule_id = sanitize_identifier(&edge_id);

        let source_snippets = result.per_node.entry(source_id.clone()).or_default();

        match msg_type.as_str() {
            "offering" => {
        let helper = format!("consult_{}", sanitize_identifier(target_label));
        let topic = str_field("topicSuffix")
            .filter(|s| !s.is_empty())
            .map(|s| format!("consults/{}", sanitize_topic(&s)))
            .unwrap_or_else(|| format!("consults/{}", sanitize_topic(&edge_id)));

        // ── Source Prolog: consult helper (+ boolean guard) ──────────────
        source_snippets.prolog.push_str(&format!(
            "% Edge {}: offering {} -> {} ({})\n",
            edge_id, source_id, target_id, target_label
        ));
        match &guard_goal {
            Some(guard) => {
                source_snippets.prolog.push_str(&format!(
                    "{}(Payload, Result) :-\n    {},\n    caws_consult('{}', '{}', Payload, Result).\n\n",
                    helper, guard, target_id, topic
                ));
            }
            None => {
                source_snippets.prolog.push_str(&format!(
                    "{}(Payload, Result) :-\n    caws_consult('{}', '{}', Payload, Result).\n\n",
                    helper, target_id, topic
                ));
            }
        }

        // ── Source CLIPS: forward-chaining hook on the correlated reply ──
        source_snippets.clips.push_str(&format!(
            "; Edge {}: react to {}'s reply on {}\n\
             (defrule edge-{}-on-reply\n    \
             (coire-event (origin \"ritual/hohi\") (topic \"{}\"))\n    \
             =>\n    \
             (coire-publish-assert \"edge_replied('{}')\"))\n\n",
            edge_id, target_label, topic,
            rule_id, topic, edge_id
        ));

        // ── Source CLIPS: typed reply dispatch → edge_result/3 + hooks ───
        // The timeout rule carries no topic constraint — timeout events have
        // only a correlation id; the Prolog side attributes them to this edge
        // via caws_edge_offer/2.
        source_snippets.clips.push_str(&format!(
            "; Edge {eid}: dispatch {tl}'s typed replies to Prolog (edge_result/3 + hooks)\n\
             (defrule edge-{rid}-on-hohi-result\n    \
             (coire-event (origin \"ritual/hohi\") (topic \"{topic}\") (correlation ?cid&~\"\"))\n    \
             =>\n    \
             (coire-publish-goal (str-cat \"caws_edge_reply('{eid}', hohi, '\" ?cid \"')\")))\n\n\
             (defrule edge-{rid}-on-tabu-result\n    \
             (coire-event (origin \"ritual/tabu\") (topic \"{topic}\") (correlation ?cid&~\"\"))\n    \
             =>\n    \
             (coire-publish-goal (str-cat \"caws_edge_reply('{eid}', tabu, '\" ?cid \"')\")))\n\n\
             (defrule edge-{rid}-on-timeout-result\n    \
             (coire-event (origin \"ritual/tabu-timeout\") (correlation ?cid&~\"\"))\n    \
             =>\n    \
             (coire-publish-goal (str-cat \"caws_edge_reply('{eid}', tabu_timeout, '\" ?cid \"')\")))\n\n",
            eid = edge_id,
            rid = rule_id,
            tl = target_label,
            topic = topic,
        ));

        // ── Auto-pipe: forward incoming Offerings along the edge ─────────
        if pipe_mode == "auto" {
            let pipe_functor = format!("caws_auto_pipe_{}", rule_id);
            source_snippets.prolog.push_str(&format!(
                "% Edge {}: auto-pipe incoming Offerings to {} ({})\n",
                edge_id, target_id, target_label
            ));
            match &guard_goal {
                Some(guard) => {
                    source_snippets.prolog.push_str(&format!(
                        "{f}(Cid) :-\n    {},\n    caws_pipe('{}', '{}', '{}', Cid).\n{f}(_).\n\n",
                        guard, edge_id, target_id, topic, f = pipe_functor
                    ));
                }
                None => {
                    source_snippets.prolog.push_str(&format!(
                        "{f}(Cid) :-\n    caws_pipe('{}', '{}', '{}', Cid).\n{f}(_).\n\n",
                        edge_id, target_id, topic, f = pipe_functor
                    ));
                }
            }
            source_snippets.clips.push_str(&format!(
                "; Edge {eid}: auto-pipe — forward every incoming Offering to {tl}\n\
                 (defrule edge-{rid}-auto-pipe\n    \
                 (coire-event (origin \"ritual/offering\") (correlation ?cid&~\"\"))\n    \
                 =>\n    \
                 (coire-publish-goal (str-cat \"{f}('\" ?cid \"')\")))\n\n",
                eid = edge_id,
                rid = rule_id,
                tl = target_label,
                f = pipe_functor,
            ));
        }
            }
            "event" | "hohi" | "tabu" => {
                let kind = msg_type.as_str();
                let topic = str_field("topicSuffix")
                    .filter(|s| !s.is_empty())
                    .map(|s| format!("{}/{}", kind, sanitize_topic(&s)))
                    .unwrap_or_else(|| format!("{}/{}", kind, sanitize_topic(&edge_id)));
                let helper = format!("emit_{}_{}", sanitize_identifier(target_label), kind);

                // ── Source Prolog: manual emit helper (+ boolean guard) ──
                source_snippets.prolog.push_str(&format!(
                    "% Edge {}: {} {} -> {} ({})\n",
                    edge_id, kind, source_id, target_id, target_label
                ));
                match &guard_goal {
                    Some(guard) => {
                        source_snippets.prolog.push_str(&format!(
                            "{}(Payload) :-\n    {},\n    caws_emit('{}', '{}', {}, Payload).\n\n",
                            helper, guard, target_id, topic, kind
                        ));
                    }
                    None => {
                        source_snippets.prolog.push_str(&format!(
                            "{}(Payload) :-\n    caws_emit('{}', '{}', {}, Payload).\n\n",
                            helper, target_id, topic, kind
                        ));
                    }
                }

                // Target CLIPS: dispatch the message → edge_message/3 + hook.
                // Built here (source_id/rule_id/kind/topic are all in scope)
                // but only appended to `result.per_node` at the end of this
                // arm — `source_snippets` above is a live mutable borrow of
                // the same map and must not overlap with a second entry().
                let target_dispatch_rule = format!(
                    "; Edge {eid}: dispatch {src}'s '{kind}' message to Prolog (edge_message/3 + hook)\n\
                     (defrule edge-{rid}-on-message\n    \
                     (coire-event (origin \"ritual/{kind}\") (topic \"{topic}\") (correlation ?cid&~\"\"))\n    \
                     =>\n    \
                     (coire-publish-goal (str-cat \"caws_edge_message('{eid}', {kind}, '\" ?cid \"')\")))\n\n",
                    eid = edge_id,
                    rid = rule_id,
                    src = source_id,
                    kind = kind,
                    topic = topic,
                );

                // ── Auto-tee: forward chaining along the edge ────────────
                if pipe_mode == "auto" {
                    let tee_functor = format!("caws_auto_tee_{}", rule_id);
                    source_snippets.prolog.push_str(&format!(
                        "% Edge {}: auto-tee incoming '{}' messages to {} ({})\n",
                        edge_id, kind, target_id, target_label
                    ));
                    match &guard_goal {
                        Some(guard) => {
                            source_snippets.prolog.push_str(&format!(
                                "{f}(Cid) :-\n    {},\n    caws_tee('{}', '{}', '{}', {}, Cid).\n{f}(_).\n\n",
                                guard, edge_id, target_id, topic, kind, f = tee_functor
                            ));
                        }
                        None => {
                            source_snippets.prolog.push_str(&format!(
                                "{f}(Cid) :-\n    caws_tee('{}', '{}', '{}', {}, Cid).\n{f}(_).\n\n",
                                edge_id, target_id, topic, kind, f = tee_functor
                            ));
                        }
                    }
                    // Trigger origins to react to: hohi/event forward the
                    // matching single wire label; tabu forwards both the
                    // direct reply and the local timeout event, but the
                    // wrapper always tees with Kind=tabu (W4 — there is no
                    // wire-level "tabu-timeout" label).
                    let trigger_origins: &[(&str, &str)] = match kind {
                        "hohi" => &[("hohi", "ritual/hohi")],
                        "tabu" => &[
                            ("tabu", "ritual/tabu"),
                            ("tabu-timeout", "ritual/tabu-timeout"),
                        ],
                        _ => &[("event", "ritual/event")],
                    };
                    for (rule_suffix, origin) in trigger_origins {
                        source_snippets.clips.push_str(&format!(
                            "; Edge {eid}: auto-tee — forward every incoming '{origin}' message to {tl}\n\
                             (defrule edge-{rid}-auto-tee-{suffix}\n    \
                             (coire-event (origin \"{origin}\") (correlation ?cid&~\"\"))\n    \
                             =>\n    \
                             (coire-publish-goal (str-cat \"{f}('\" ?cid \"')\")))\n\n",
                            eid = edge_id,
                            rid = rule_id,
                            tl = target_label,
                            suffix = rule_suffix,
                            origin = origin,
                            f = tee_functor,
                        ));
                    }
                }

                // Append the target dispatch rule built above — the last
                // touch of `result.per_node` in this arm, once `source_snippets`
                // (the other live entry into the same map) is done being used.
                result.per_node.entry(target_id.clone()).or_default()
                    .clips.push_str(&target_dispatch_rule);
            }
            _ => {
                source_snippets.prolog.push_str(&format!(
                    "% Edge {} carries '{}' messages — no consult helper generated \
                     (typed listener semantics ride on the envelope label).\n",
                    edge_id, msg_type
                ));
                continue;
            }
        }

        // ── Target Prolog: assertion qualifier seeds ground truth ────────
        // Shared by offering and message (event/hohi/tabu) edges — unknown
        // msgTypes `continue`d above and never reach this.
        if qualifier_kind == "assertion" && !qualifier_value.trim().is_empty() {
            let target_snippets = result.per_node.entry(target_id.clone()).or_default();
            let mut fact = qualifier_value.trim().to_string();
            if !fact.ends_with('.') {
                fact.push('.');
            }
            target_snippets.prolog.push_str(&format!(
                "% Edge {}: dynamic incoming declaration from {}\n{}\n\n",
                edge_id, source_id, fact
            ));
        }
    }

    Ok(result)
}

/// Lowercase alphanumerics/underscores only — safe as a Prolog functor
/// fragment or CLIPS rule-name fragment.
fn sanitize_identifier(raw: &str) -> String {
    let mut out: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    if out.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(true) {
        out.insert(0, 'e');
    }
    out
}

/// Topic-path fragment: keep alphanumerics, `.`, `_`, `-`; replace the rest.
fn sanitize_topic(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Detect predicates defined with 2+ rule clauses that are not already declared dynamic.
///
/// These are candidates for synthetic group treatment: declaring them dynamic makes
/// their results available for forward-chaining notification when assertz'd by callers.
fn detect_multi_clause_groups(rules: &[PrologRule], existing_dynamic: &[String]) -> Vec<String> {
    let existing: HashSet<&str> = existing_dynamic.iter().map(|s| s.as_str()).collect();
    let mut counts: HashMap<String, usize> = HashMap::new();
    for rule in rules {
        if !rule.body.is_empty() {
            let (f, a) = term_functor_arity(&rule.head);
            *counts.entry(format!("{}/{}", f, a)).or_default() += 1;
        }
    }
    let mut groups: Vec<String> = counts.into_iter()
        .filter(|(k, count)| *count >= 2 && !existing.contains(k.as_str()))
        .map(|(k, _)| k)
        .collect();
    groups.sort();
    groups
}

/// Extract `Functor/Arity` indicator strings from `:- dynamic(...)` directives.
///
/// Handles single-indicator and comma-separated multi-indicator forms on one line,
/// e.g. `:- dynamic(foo/1).` or `:- dynamic(foo/1, bar/2).`
fn extract_dynamic_indicators(source: &str) -> Vec<String> {
    let mut indicators = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        let inner = trimmed
            .strip_prefix(":-")
            .map(str::trim)
            .and_then(|s| s.strip_prefix("dynamic("))
            .and_then(|s| s.strip_suffix(")."));
        if let Some(inner) = inner {
            for part in inner.split(',') {
                let ind = part.trim().to_string();
                if !ind.is_empty() {
                    indicators.push(ind);
                }
            }
        }
    }
    indicators
}

/// Build the Clara integration preamble for a set of dynamic predicate indicators
/// and synthetic group predicates (multi-clause predicates not already declared dynamic).
fn generate_listen_preamble(indicators: &[String], synthetic_groups: &[String]) -> String {
    let mut out = String::new();
    out.push_str("% ── Clara integration (auto-generated) ──────────────────────────────────────\n");
    for ind in indicators {
        out.push_str(&format!(":- prolog_listen({}, updated({})).\n", ind, ind));
    }
    if !indicators.is_empty() {
        out.push('\n');
    }
    out.push_str("updated(Pred, Action, Context) :-\n");
    out.push_str("    clause(Head, _Body, Context),\n");
    out.push_str("    coire_publish_assert(Head),\n");
    out.push_str("    format('Updated ~w with action ~w in context ~p~n', [Pred, Action, Head]).\n");
    if !synthetic_groups.is_empty() {
        out.push('\n');
        out.push_str("% ── Clara synthetic groups (multi-clause predicates) ─────────────────────────\n");
        out.push_str("% Declared dynamic so that assertz'd results trigger forward-chaining\n");
        out.push_str("% notification. Mirrored as umbrella nodes in the DOT graph.\n");
        for grp in synthetic_groups {
            out.push_str(&format!(":- dynamic({}).\n", grp));
            out.push_str(&format!(":- prolog_listen({}, updated({})).\n", grp, grp));
        }
    }
    out.push_str("% ── End Clara integration ───────────────────────────────────────────────────\n");
    out
}

// ── Code-generation helpers ───────────────────────────────────────────────────

fn term_functor_name(t: &Term) -> &str {
    match t {
        Term::Atom(s) => s.as_str(),
        Term::Compound { functor, .. } => functor.as_str(),
        _ => "term",
    }
}

/// Collect all variable names appearing in a term.
fn collect_vars(t: &Term) -> HashSet<String> {
    let mut vars = HashSet::new();
    collect_vars_rec(t, &mut vars);
    vars
}

fn collect_vars_rec(t: &Term, vars: &mut HashSet<String>) {
    match t {
        Term::Variable(s) => {
            vars.insert(s.clone());
        }
        Term::Compound { args, .. } => args.iter().for_each(|a| collect_vars_rec(a, vars)),
        _ => {}
    }
}

/// Build a CLIPS expression producing the head goal string at runtime.
///
/// Variables present in `bound_vars` (bound by the LHS condition) become CLIPS
/// `?Var` references inside a `str-cat`. Unbound variables are emitted as
/// their name as a literal string — Prolog will treat them as free variables.
/// Consecutive literal segments are merged into a single quoted string.
fn render_head_goal_expr(head: &Term, bound_vars: &HashSet<String>) -> String {
    match head {
        Term::Atom(s) => format!("\"{}\"", s),
        Term::Compound { functor, args } => {
            let any_bound = args.iter().any(|a| is_bound_var(a, bound_vars));
            if !any_bound {
                // Pure string — all args are ground or unbound.
                let args_s: Vec<_> =
                    args.iter().map(|a| render_arg_as_literal(a)).collect();
                format!("\"{}({})\"", functor, args_s.join(","))
            } else {
                // Build str-cat, merging consecutive literal segments.
                let mut parts: Vec<String> = Vec::new();
                let mut cur = format!("{}(", functor);
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        cur.push(',');
                    }
                    if is_bound_var(arg, bound_vars) {
                        if !cur.is_empty() {
                            parts.push(format!("\"{}\"", cur));
                            cur = String::new();
                        }
                        if let Term::Variable(s) = arg {
                            parts.push(format!("?{}", s));
                        }
                    } else {
                        cur.push_str(&render_arg_as_literal(arg));
                    }
                }
                cur.push(')');
                parts.push(format!("\"{}\"", cur));
                format!("(str-cat {})", parts.join(" "))
            }
        }
        _ => format!("\"{}\"", render_prolog_term(head)),
    }
}

/// Render a negative body goal as a `not_<functor>` CLIPS ordered-fact pattern.
///
/// `\+ has(X, backbone)` → `(not_has ?X backbone)`
/// `\+ alive` → `(not_alive)`
fn render_not_clips_pattern(t: &Term) -> String {
    match t {
        Term::Atom(s) => format!("(not_{})", s),
        Term::Compound { functor, args } => {
            let fields: Vec<_> = args.iter().map(render_clips_field).collect();
            format!("(not_{} {})", functor, fields.join(" "))
        }
        _ => format!("(not_{})", render_prolog_term(t)),
    }
}

fn is_bound_var(t: &Term, bound_vars: &HashSet<String>) -> bool {
    matches!(t, Term::Variable(s) if bound_vars.contains(s))
}

/// Render a term argument as a Prolog literal value (no CLIPS vars).
///
/// Variables are emitted as their bare name (unbound → Prolog will treat as
/// free variable, which the caller is responsible for avoiding).  All other
/// terms are rendered via `render_prolog_term` so that atoms that require
/// quoting (e.g. `'Greet the visitor.'`) get proper single-quote wrapping
/// rather than being emitted bare, which would cause Prolog instantiation
/// or syntax errors.
fn render_arg_as_literal(t: &Term) -> String {
    match t {
        Term::Variable(s) => s.clone(), // unbound → emit variable name
        _ => render_prolog_term(t),
    }
}

/// Resolve the effective CLIPS trigger for a positive body goal.
///
/// - Known meta-predicates (assert/retract, I/O, control, arithmetic, etc.) → `None`
///   (no meaningful CLIPS trigger; skip the rule)
/// - All other goals → `Some(t)` (use as-is)
fn effective_trigger(t: &Term) -> Option<&Term> {
    match t {
        Term::Compound { functor, .. } if is_meta_predicate(functor) => None,
        Term::Atom(s) if is_meta_predicate(s) => None,
        // Bare variables (e.g. from `Reason = "..."` parsed as `Reason`) have no
        // meaningful CLIPS fact-pattern equivalent — skip them.
        Term::Variable(_) => None,
        _ => Some(t),
    }
}

fn is_meta_predicate(name: &str) -> bool {
    matches!(
        name,
        "assert"
            | "assertz"
            | "asserta"
            | "retract"
            | "retractall"
            | "abolish"
            | "format"
            | "write"
            | "writeln"
            | "nl"
            | "print"
            | "read"
            | "copy_term"
            | "functor"
            | "arg"
            | "call"
            | "bagof"
            | "setof"
            | "findall"
            | "aggregate_all"
            | "is"
            | "succ"
            | "plus"
            | "true"
            | "fail"
            | "otherwise"
            | "coire_publish_assert"
            | "coire_publish"
            | "coire_emit"
            | "coire_poll"
    )
}

// ── DOT graph generation ──────────────────────────────────────────────────────

/// Options controlling [`generate_dot`] output.
pub struct DotOptions {
    /// When `true`, emit a dashed gray undirected edge between condition nodes
    /// that share the same functor, arity, and arguments across different rules
    /// (e.g. two `turn(left)` nodes in separate rule bodies).
    ///
    /// Useful for identifying shared sub-goals that appear in multiple rules.
    /// Defaults to `false` to keep graphs uncluttered.
    pub link_shared_conditions: bool,
}

impl Default for DotOptions {
    fn default() -> Self {
        Self { link_shared_conditions: false }
    }
}

/// Per-node truth-value coloring for deduction-phase DOT graphs.
///
/// Keys are predicate functor names (e.g. `"unlocked"`, `"tumbler_1"`).
/// When a node's functor appears in `values`, its fill color is drawn from the
/// truth-value palette instead of the structural default.
///
/// Build from a Dagda tableau snapshot with [`coloring_from_entries`], then
/// pass to [`generate_dot`].
pub struct NodeColoring {
    pub values: HashMap<String, TruthValue>,
}

/// Extract file paths from `consult(file)` bare facts in a parsed rule set.
///
/// Scans for fact nodes (rules with empty bodies) whose head is
/// `consult(atom_or_string)` and returns the inner path strings.
///
/// This is used by trace visualization to follow consulted files and include
/// their rules in the dependency graph when the registered source only contains
/// seed clauses (e.g. `consult('rules.pl').`, `day_of_week(saturday).`).
pub fn extract_consulted_files(rules: &[PrologRule]) -> Vec<String> {
    rules.iter()
        .filter(|r| r.body.is_empty())
        .filter_map(|r| {
            if let crate::transpile::Term::Compound { functor, args } = &r.head {
                if functor == "consult" && args.len() == 1 {
                    return match &args[0] {
                        crate::transpile::Term::Atom(s)
                        | crate::transpile::Term::Str(s) => Some(s.clone()),
                        _ => None,
                    };
                }
            }
            None
        })
        .collect()
}

/// Propagate truth values forward through the rule graph in a [`NodeColoring`].
///
/// Performs a fixpoint iteration over the rule set.  For each rule whose every
/// non-meta positive body-condition functor resolves to [`TruthValue::KnownTrue`]
/// in `coloring`, the rule head functor is also marked `KnownTrue`.
///
/// This lets intermediate heads like `wet_surface` and `not_sprinklers` acquire
/// green fill even when they are absent from the Dagda tableau — they are proven
/// by propagation from known-true leaf facts.
///
/// **Negative body goals** (`\+`) are treated as satisfied (skipped) during
/// propagation because their truth is tracked separately.
///
/// **Multi-clause predicates**: a head is marked `KnownTrue` as soon as any
/// single clause's entire body is `KnownTrue`.
pub fn propagate_rule_coloring(rules: &[PrologRule], coloring: &mut NodeColoring) {
    loop {
        let mut changed = false;
        for rule in rules {
            if rule.body.is_empty() {
                continue; // bare facts need no propagation
            }
            let (head_functor, _) = term_functor_arity(&rule.head);
            if let Some(TruthValue::KnownTrue) = coloring.values.get(head_functor) {
                continue; // already known true — skip
            }
            // All positive, non-meta body conditions must be KnownTrue.
            let all_satisfied = rule.body.iter().all(|goal| match goal {
                BodyGoal::Negative(_) => true, // negation-as-failure: treat as satisfied
                BodyGoal::Positive(term) => {
                    let (cf, _) = term_functor_arity(term);
                    if is_meta_predicate(cf) {
                        return true; // meta-predicates are infrastructure, not domain facts
                    }
                    matches!(coloring.values.get(cf), Some(TruthValue::KnownTrue))
                }
            });
            if all_satisfied {
                coloring.values.insert(head_functor.to_string(), TruthValue::KnownTrue);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Build a [`NodeColoring`] from a tableau snapshot.
///
/// Each [`PredicateEntry`] contributes its functor name → truth value.  When
/// multiple entries share the same functor (e.g. `tumbler/2` with different
/// concrete arguments) the values are merged using the following strategy:
///
/// - All entries for a functor have the **same** truth value → use it.
/// - Entries are a mix of any truth values → `KnownUnresolved` (amber).
///   This includes the `KnownTrue` + `KnownFalse` conflict case, which is
///   rendered as amber ("partially resolved") rather than a hard false.
///
/// Nodes absent from the tableau are left uncolored (structural defaults apply).
pub fn coloring_from_entries(entries: &[clara_dagda::PredicateEntry]) -> NodeColoring {
    // Group truth values by functor name.
    let mut by_functor: HashMap<String, Vec<TruthValue>> = HashMap::new();
    for entry in entries {
        by_functor
            .entry(entry.functor.trim_end_matches('.').to_string())
            .or_default()
            .push(entry.truth_value.clone());
    }

    let mut values = HashMap::new();
    for (functor, tvs) in by_functor {
        let first = tvs[0].clone();
        let merged = if tvs.iter().all(|tv| tv == &first) {
            first
        } else {
            TruthValue::KnownUnresolved
        };
        values.insert(functor, merged);
    }
    NodeColoring { values }
}

/// Generate a Graphviz DOT graph from parsed Prolog rules.
///
/// Layout: `rankdir=LR` — rule heads appear to the left of their conditions.
///
/// Node types:
/// - **Fact nodes** (green ellipse): bare facts with empty bodies.
/// - **Rule head nodes** (blue box): heads of rules with at least one body goal.
/// - **Condition nodes** (amber dashed ellipse): leaf body goals — those not
///   bridged to another rule head via assert or direct functor match.
///
/// Edge types:
/// - `requires` (black): rule head → leaf condition.
/// - assert-bridge (solid blue, no label): rule head A → rule head B, emitted
///   when A has a condition whose functor/arity is asserted by B.
///   Assert-bridge takes precedence over `chains-to` for the same condition.
/// - `chains-to` (dashed blue): condition node → rule head whose functor/arity
///   it directly matches. Emitted as secondary when a condition is also assert-bridged.
/// - `satisfies` (dashed gray): fact node → condition whose functor/arity it matches.
/// - shared-condition (dashed gray, undirected): between condition nodes sharing
///   the same label across rules, when `opts.link_shared_conditions` is true.
pub fn generate_dot(rules: &[PrologRule], coloring: Option<&NodeColoring>, opts: &DotOptions) -> String {
    let mut out = String::new();
    out.push_str("digraph Clara {\n");
    out.push_str("    rankdir=LR\n");
    out.push_str("    fontname=\"Helvetica\"\n");
    out.push_str("    node [fontname=\"Helvetica\" fontsize=11]\n");
    out.push_str("    edge [fontsize=9]\n\n");

    let facts: Vec<(usize, &PrologRule)> = rules.iter().enumerate()
        .filter(|(_, r)| r.body.is_empty())
        .collect();
    let rule_list: Vec<(usize, &PrologRule)> = rules.iter().enumerate()
        .filter(|(_, r)| !r.body.is_empty())
        .collect();

    // ── Fact nodes ────────────────────────────────────────────────────────────
    let mut fact_ids: Vec<(String, String, usize)> = Vec::new(); // (node_id, functor, arity)
    for (orig_i, rule) in &facts {
        let (f, a) = term_functor_arity(&rule.head);
        let node_id = format!("fact_{}_{}_{}", dot_id(f), a, orig_i);
        let label = render_prolog_term(&rule.head);
        let fill = node_fill_color(f, coloring, "#d4edda");
        out.push_str(&format!(
            "    {} [label=\"{}\" shape=ellipse style=filled fillcolor=\"{}\"]\n",
            node_id, escape_dot_label(&label), fill
        ));
        fact_ids.push((node_id, f.to_string(), a));
    }
    if !facts.is_empty() {
        out.push('\n');
    }

    // ── Build "produces" index ────────────────────────────────────────────────
    // Maps the rendered label of each asserted term → cluster indices of the rules
    // that assert it. Keyed by full term label (not just functor/arity) so that
    // `assert(tumbler(1,set))` only matches the condition `tumbler(1,set)`, not
    // every `tumbler/2` condition.
    let mut produces: HashMap<String, Vec<usize>> = HashMap::new();
    for (cluster_i, (_orig_i, rule)) in rule_list.iter().enumerate() {
        for goal in &rule.body {
            if let Some(asserted) = extract_asserted_term(goal) {
                let key = render_prolog_term(asserted);
                produces.entry(key).or_default().push(cluster_i);
            }
        }
    }

    // ── Pre-compute rule head node ids ────────────────────────────────────────
    let rule_head_ids: Vec<String> = rule_list.iter().enumerate()
        .map(|(ci, (_orig_i, rule))| {
            let (hf, ha) = term_functor_arity(&rule.head);
            format!("rule_{}_{}_{}", dot_id(hf), ha, ci)
        })
        .collect();

    // Multi-target head lookup: (functor, arity) → Vec of cluster indices.
    // Vec because multiple rules can share the same head functor/arity (e.g. admit/2).
    let mut head_by_fa: HashMap<(String, usize), Vec<usize>> = HashMap::new();
    for (ci, (_orig_i, rule)) in rule_list.iter().enumerate() {
        let (f, a) = term_functor_arity(&rule.head);
        head_by_fa.entry((f.to_string(), a)).or_default().push(ci);
    }

    // ── Rule head nodes ───────────────────────────────────────────────────────
    for (cluster_i, (_orig_i, rule)) in rule_list.iter().enumerate() {
        let (hf, _ha) = term_functor_arity(&rule.head);
        let head_id = &rule_head_ids[cluster_i];
        let head_label = render_prolog_term(&rule.head);
        let fill = node_fill_color(hf, coloring, "#cce5ff");
        out.push_str(&format!(
            "    {} [label=\"{}\" shape=box style=filled fillcolor=\"{}\" penwidth=2]\n",
            head_id, escape_dot_label(&head_label), fill
        ));
    }
    out.push('\n');

    // ── Synthetic umbrella nodes for multi-clause predicates ──────────────────
    // When 2+ rules share the same functor/arity head, emit a single "group" node
    // (darker blue, heavier border) that chains-to edges point to, fanning out via
    // "clause" edges to the concrete rule clause nodes.
    let mut synth_ids: HashMap<(String, usize), String> = HashMap::new();
    {
        let mut multi: Vec<_> = head_by_fa.iter().filter(|(_, v)| v.len() > 1).collect();
        multi.sort_by_key(|(k, _)| (*k).clone());
        let mut emitted = false;
        for ((f, a), indices) in &multi {
            let synth_id = format!("synth_{}_{}", dot_id(f), a);
            let first_label = format!("{}/{}", f, a);
            let fill = node_fill_color(f, coloring, "#7ba7d4");
            out.push_str(&format!(
                "    {} [label=\"{}\" shape=box style=filled fillcolor=\"{}\" penwidth=3]\n",
                synth_id, escape_dot_label(&first_label), fill
            ));
            for &ci in *indices {
                out.push_str(&format!(
                    "    {} -> {} [label=\"clause\" style=dotted color=\"#555555\"]\n",
                    synth_id, rule_head_ids[ci]
                ));
            }
            synth_ids.insert((f.to_string(), *a), synth_id);
            emitted = true;
        }
        if emitted { out.push('\n'); }
    }

    // ── Condition nodes, requires edges, and deferred edge collection ─────────
    // all_cond_ids[cluster_i] = Vec<(node_id, functor, arity, label)>
    let mut all_cond_ids: Vec<Vec<(String, String, usize, String)>> = Vec::new();
    let mut assert_bridge_edges: Vec<(String, String)> = Vec::new();
    let mut chains_to_edges: Vec<(String, String)> = Vec::new();

    for (cluster_i, (_orig_i, rule)) in rule_list.iter().enumerate() {
        let head_id = &rule_head_ids[cluster_i];
        let mut cond_ids: Vec<(String, String, usize, String)> = Vec::new();

        for (ci, goal) in rule.body.iter().enumerate() {
            let (term, is_neg) = match goal {
                BodyGoal::Positive(t) => (t, false),
                BodyGoal::Negative(t) => (t, true),
            };

            // Special case: findall/3 — extract the Goal arg (args[1]) as a visible
            // condition node so that callers of findall are linked to the collected
            // predicate's rule(s) in the graph.
            if !is_neg {
                if let Term::Compound { functor, args } = term {
                    if functor == "findall" && args.len() == 3 {
                        let inner = &args[1];
                        let (if_, ia) = term_functor_arity(inner);
                        if !is_meta_predicate(if_) {
                            let inner_label = render_prolog_term(inner);
                            let inner_cond_id = format!("cond_{}_{}c", cluster_i, ci);
                            let fill = node_fill_color(if_, coloring, "#fff3cd");
                            out.push_str(&format!(
                                "    {} [label=\"{}\" shape=ellipse style=\"filled,dashed\" fillcolor=\"{}\"]\n",
                                inner_cond_id, escape_dot_label(&inner_label), fill
                            ));
                            out.push_str(&format!(
                                "    {} -> {} [label=\"collects\"]\n",
                                head_id, inner_cond_id
                            ));
                            if let Some(target) = resolve_chain_target(if_, ia, &head_by_fa, &rule_head_ids, &synth_ids) {
                                chains_to_edges.push((inner_cond_id.clone(), target));
                            }
                            cond_ids.push((inner_cond_id, if_.to_string(), ia, inner_label));
                        }
                        continue;
                    }
                }
            }

            // Skip meta-predicates (assert, retract, format, etc.) — they are used
            // to build cross-rule links, not rendered as condition nodes.
            if is_meta_predicate(term_functor_name(term)) {
                continue;
            }

            let (cf, ca) = term_functor_arity(term);
            let cond_id = format!("cond_{}_{}", cluster_i, ci);
            let cond_label = if is_neg {
                format!("\\\\+ {}", render_prolog_term(term))
            } else {
                render_prolog_term(term)
            };
            let fill = node_fill_color(cf, coloring, "#fff3cd");

            // Assert-bridge lookup uses the full rendered term for exact matching;
            // chains-to lookup uses functor/arity for structural matching.
            let assert_producers = produces.get(&cond_label);
            let chain_target = resolve_chain_target(cf, ca, &head_by_fa, &rule_head_ids, &synth_ids);

            if let Some(producers) = assert_producers {
                // Assert-bridge takes precedence: emit rule-head → rule-head edges.
                // Direction: this head requires what the producing rule asserts.
                // No condition node emitted for this goal.
                // TODO: consider per-edge term labels or tooltips in a future sprint.
                for &prod_ci in producers {
                    assert_bridge_edges.push((head_id.clone(), rule_head_ids[prod_ci].clone()));
                }
                // Secondary chains-to if condition also directly matches a rule head.
                if let Some(target) = chain_target {
                    out.push_str(&format!(
                        "    {} [label=\"{}\" shape=ellipse style=\"filled,dashed\" fillcolor=\"{}\"]\n",
                        cond_id, escape_dot_label(&cond_label), fill
                    ));
                    out.push_str(&format!("    {} -> {} [label=\"requires\"]\n", head_id, cond_id));
                    chains_to_edges.push((cond_id.clone(), target));
                    cond_ids.push((cond_id, cf.to_string(), ca, cond_label));
                }
            } else if let Some(target) = chain_target {
                // Chain-bridged: condition visible, chains-to edge deferred.
                out.push_str(&format!(
                    "    {} [label=\"{}\" shape=ellipse style=\"filled,dashed\" fillcolor=\"{}\"]\n",
                    cond_id, escape_dot_label(&cond_label), fill
                ));
                out.push_str(&format!("    {} -> {} [label=\"requires\"]\n", head_id, cond_id));
                chains_to_edges.push((cond_id.clone(), target));
                cond_ids.push((cond_id, cf.to_string(), ca, cond_label));
            } else {
                // Leaf condition: not produced by any rule, no direct head match.
                out.push_str(&format!(
                    "    {} [label=\"{}\" shape=ellipse style=\"filled,dashed\" fillcolor=\"{}\"]\n",
                    cond_id, escape_dot_label(&cond_label), fill
                ));
                out.push_str(&format!("    {} -> {} [label=\"requires\"]\n", head_id, cond_id));
                cond_ids.push((cond_id, cf.to_string(), ca, cond_label));
            }
        }
        all_cond_ids.push(cond_ids);
    }
    out.push('\n');

    // ── Assert-bridge edges ───────────────────────────────────────────────────
    if !assert_bridge_edges.is_empty() {
        for (from, to) in &assert_bridge_edges {
            out.push_str(&format!("    {} -> {} [color=\"#1a73e8\"]\n", from, to));
        }
        out.push('\n');
    }

    // ── Chains-to edges ───────────────────────────────────────────────────────
    if !chains_to_edges.is_empty() {
        for (cond_id, target) in &chains_to_edges {
            out.push_str(&format!(
                "    {} -> {} [label=\"chains-to\" style=dashed color=\"#1a73e8\"]\n",
                cond_id, target
            ));
        }
        out.push('\n');
    }

    // ── Satisfies edges ───────────────────────────────────────────────────────
    let mut emitted_satisfies = false;
    for (fact_node_id, fact_f, fact_a) in &fact_ids {
        for cond_ids in &all_cond_ids {
            for (cond_id, cond_f, cond_a, _) in cond_ids {
                if fact_f == cond_f && fact_a == cond_a {
                    out.push_str(&format!(
                        "    {} -> {} [label=\"satisfies\" style=dashed color=\"#555555\"]\n",
                        fact_node_id, cond_id
                    ));
                    emitted_satisfies = true;
                }
            }
        }
    }
    if emitted_satisfies {
        out.push('\n');
    }

    // ── Shared condition links ────────────────────────────────────────────────
    if opts.link_shared_conditions {
        let mut by_label: HashMap<String, Vec<String>> = HashMap::new();
        for cond_ids in &all_cond_ids {
            for (cond_id, _, _, label) in cond_ids {
                by_label.entry(label.clone()).or_default().push(cond_id.clone());
            }
        }
        let mut emitted_shared = false;
        for ids in by_label.values() {
            if ids.len() > 1 {
                for i in 0..ids.len() - 1 {
                    out.push_str(&format!(
                        "    {} -> {} [style=dashed color=\"#aaaaaa\" dir=none]\n",
                        ids[i], ids[i + 1]
                    ));
                    emitted_shared = true;
                }
            }
        }
        if emitted_shared {
            out.push('\n');
        }
    }

    out.push_str("}\n");
    out
}

/// Extract functor name and arity from a term.
fn term_functor_arity(t: &Term) -> (&str, usize) {
    match t {
        Term::Atom(s) => (s.as_str(), 0),
        Term::Compound { functor, args } => (functor.as_str(), args.len()),
        Term::Variable(s) => (s.as_str(), 0),
        _ => ("_", 0),
    }
}

/// Sanitize a string for use as a DOT node identifier (alphanumeric + underscores).
fn dot_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

/// Escape a string for use inside a DOT double-quoted label.
fn escape_dot_label(s: &str) -> String {
    s.replace('\\', "\\\\")
     .replace('"', "\\\"")
}

/// Resolve the chains-to target node id for a condition with functor `f` / arity `a`.
///
/// Returns the synth umbrella node id when 2+ rules share that head functor/arity,
/// or the single concrete rule head node id when only one rule matches.
/// Returns `None` if no rule head matches.
fn resolve_chain_target(
    f: &str,
    a: usize,
    head_by_fa: &HashMap<(String, usize), Vec<usize>>,
    rule_head_ids: &[String],
    synth_ids: &HashMap<(String, usize), String>,
) -> Option<String> {
    head_by_fa.get(&(f.to_string(), a)).map(|indices| {
        if indices.len() == 1 {
            rule_head_ids[indices[0]].clone()
        } else {
            synth_ids.get(&(f.to_string(), a))
                .cloned()
                .unwrap_or_else(|| rule_head_ids[indices[0]].clone())
        }
    })
}

/// Extract the inner asserted term from `assert(X)`, `assertz(X)`, or `asserta(X)`.
fn extract_asserted_term(goal: &BodyGoal) -> Option<&Term> {
    if let BodyGoal::Positive(Term::Compound { functor, args }) = goal {
        if (functor == "assert" || functor == "assertz" || functor == "asserta") && args.len() == 1 {
            return Some(&args[0]);
        }
    }
    None
}

/// Map a `TruthValue` to its DOT fill color hex string.
fn truth_fill_color(tv: &TruthValue) -> &'static str {
    match tv {
        TruthValue::KnownTrue       => "#28a745",
        TruthValue::KnownFalse      => "#dc3545",
        TruthValue::KnownUnresolved => "#ffc107",
        TruthValue::Unknown         => "#adb5bd",
    }
}

/// Return the fill color for a node: truth-value override if present, else `default`.
fn node_fill_color(functor: &str, coloring: Option<&NodeColoring>, default: &'static str) -> &'static str {
    coloring
        .and_then(|c| c.values.get(functor))
        .map(truth_fill_color)
        .unwrap_or(default)
}

fn format_rule_comment(rule: &PrologRule) -> String {
    let head = render_prolog_term(&rule.head);
    if rule.body.is_empty() {
        format!("{}.", head)
    } else {
        let body: Vec<_> = rule.body.iter().map(|g| match g {
            BodyGoal::Positive(t) => render_prolog_term(t),
            BodyGoal::Negative(t) => format!("\\+ {}", render_prolog_term(t)),
        }).collect();
        format!("{} :- {}.", head, body.join(", "))
    }
}

// ── Rule parser ───────────────────────────────────────────────────────────────

/// Stateful parser for Prolog source files containing multiple clauses.
struct RuleParser {
    input: Vec<u8>,
    pos: usize,
}

impl RuleParser {
    fn new(s: &str) -> Self {
        Self { input: s.as_bytes().to_vec(), pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let b = self.input.get(self.pos).copied();
        if b.is_some() {
            self.pos += 1;
        }
        b
    }

    fn skip_ws(&mut self) {
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn skip_line_comment(&mut self) {
        while self.pos < self.input.len() && self.input[self.pos] != b'\n' {
            self.pos += 1;
        }
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            self.skip_ws();
            if self.peek() == Some(b'%') {
                self.skip_line_comment();
            } else {
                break;
            }
        }
    }

    fn peek_two(&self) -> Option<[u8; 2]> {
        if self.pos + 1 < self.input.len() {
            Some([self.input[self.pos], self.input[self.pos + 1]])
        } else {
            None
        }
    }

    fn skip_n(&mut self, n: usize) {
        self.pos = (self.pos + n).min(self.input.len());
    }

    /// Advance past the next `.` (end-of-clause marker), or to EOF.
    fn skip_to_period(&mut self) {
        while let Some(b) = self.next() {
            if b == b'.' {
                break;
            }
        }
    }

    // ── Clause parsing ────────────────────────────────────────────────────────

    /// Parse one clause. Returns `Ok(None)` at EOF.
    fn parse_clause(&mut self) -> Result<Option<PrologRule>, String> {
        self.skip_ws_and_comments();
        if self.pos >= self.input.len() {
            return Ok(None);
        }

        let head = self.parse_term()?;
        self.skip_ws();

        if self.peek_two() == Some([b':', b'-']) {
            self.skip_n(2);
            self.skip_ws();
            let body = self.parse_body()?;
            self.skip_ws();
            if self.peek() == Some(b'.') {
                self.next();
            }
            Ok(Some(PrologRule { head, body }))
        } else if self.peek() == Some(b'.') {
            self.next();
            Ok(Some(PrologRule { head, body: vec![] }))
        } else {
            Err(format!("expected :- or . after head at pos {}", self.pos))
        }
    }

    /// Parse a comma/semicolon-separated list of goals (rule body).
    fn parse_body(&mut self) -> Result<Vec<BodyGoal>, String> {
        let mut goals = Vec::new();
        loop {
            self.skip_ws_and_comments();
            goals.push(self.parse_goal()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') | Some(b';') => {
                    self.next();
                }
                _ => break,
            }
        }
        Ok(goals)
    }

    /// Parse a single goal, handling `\+` and `not/1` negation.
    fn parse_goal(&mut self) -> Result<BodyGoal, String> {
        self.skip_ws();
        if self.peek_two() == Some([b'\\', b'+']) {
            self.skip_n(2);
            self.skip_ws();
            Ok(BodyGoal::Negative(self.parse_goal_atom()?))
        } else {
            let t = self.parse_goal_atom()?;
            // Treat not/1 as negation-as-failure, same as \+.
            match t {
                Term::Compound { ref functor, ref args } if functor == "not" && args.len() == 1 => {
                    Ok(BodyGoal::Negative(args[0].clone()))
                }
                _ => Ok(BodyGoal::Positive(t)),
            }
        }
    }

    /// Parse a goal atom (atom or compound), handling parenthesized groups.
    fn parse_goal_atom(&mut self) -> Result<Term, String> {
        self.skip_ws();
        if self.peek() == Some(b'(') {
            self.next();
            let t = self.parse_term()?;
            self.skip_ws();
            if self.peek() == Some(b')') {
                self.next();
            }
            Ok(t)
        } else {
            self.parse_term()
        }
    }

    // ── Term parsing ──────────────────────────────────────────────────────────

    fn parse_term(&mut self) -> Result<Term, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'\'') => self.parse_quoted_atom(),
            Some(b'"') => self.parse_double_quoted(),
            Some(b'[') => self.parse_list(),
            Some(b) if b == b'-' || b.is_ascii_digit() => self.parse_number(),
            Some(b) if b.is_ascii_uppercase() || b == b'_' => self.parse_variable(),
            Some(b) if b.is_ascii_lowercase() => self.parse_atom_or_compound(),
            Some(b) => Err(format!("unexpected char '{}' at pos {}", b as char, self.pos)),
            None => Err("unexpected end of input".into()),
        }
    }

    fn parse_quoted_atom(&mut self) -> Result<Term, String> {
        self.next(); // consume '
        let mut s = String::new();
        loop {
            match self.next() {
                None => return Err("unterminated quoted atom".into()),
                Some(b'\'') => {
                    if self.peek() == Some(b'\'') {
                        self.next();
                        s.push('\'');
                    } else {
                        break;
                    }
                }
                Some(b'\\') => match self.next() {
                    Some(b'n') => s.push('\n'),
                    Some(b't') => s.push('\t'),
                    Some(b'\\') => s.push('\\'),
                    Some(b'\'') => s.push('\''),
                    Some(c) => {
                        s.push('\\');
                        s.push(c as char);
                    }
                    None => return Err("unterminated escape in quoted atom".into()),
                },
                Some(c) => s.push(c as char),
            }
        }
        self.maybe_compound(s)
    }

    fn parse_double_quoted(&mut self) -> Result<Term, String> {
        self.next(); // consume "
        let mut s = String::new();
        loop {
            match self.next() {
                None => return Err("unterminated string".into()),
                Some(b'"') => break,
                Some(b'\\') => match self.next() {
                    Some(b'n') => s.push('\n'),
                    Some(b't') => s.push('\t'),
                    Some(b'"') => s.push('"'),
                    Some(b'\\') => s.push('\\'),
                    Some(c) => s.push(c as char),
                    None => return Err("unterminated escape in string".into()),
                },
                Some(c) => s.push(c as char),
            }
        }
        Ok(Term::Str(s))
    }

    fn parse_list(&mut self) -> Result<Term, String> {
        self.next(); // consume [
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.next();
            return Ok(Term::Atom("[]".into()));
        }
        Err("non-empty list terms are not supported in the rule parser".into())
    }

    fn parse_number(&mut self) -> Result<Term, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.next();
        }
        while self.peek().map_or(false, |b| b.is_ascii_digit()) {
            self.next();
        }
        let is_float = self.peek() == Some(b'.');
        if is_float {
            self.next();
            while self.peek().map_or(false, |b| b.is_ascii_digit()) {
                self.next();
            }
        }
        let s = std::str::from_utf8(&self.input[start..self.pos]).map_err(|e| e.to_string())?;
        if is_float {
            s.parse::<f64>().map(Term::Float).map_err(|e| e.to_string())
        } else {
            s.parse::<i64>().map(Term::Integer).map_err(|e| e.to_string())
        }
    }

    fn parse_variable(&mut self) -> Result<Term, String> {
        let start = self.pos;
        while self.peek().map_or(false, |b| b.is_ascii_alphanumeric() || b == b'_') {
            self.next();
        }
        let name = std::str::from_utf8(&self.input[start..self.pos])
            .map_err(|e| e.to_string())?
            .to_string();
        Ok(Term::Variable(name))
    }

    fn parse_atom_or_compound(&mut self) -> Result<Term, String> {
        let start = self.pos;
        while self.peek().map_or(false, |b| b.is_ascii_alphanumeric() || b == b'_') {
            self.next();
        }
        let name = std::str::from_utf8(&self.input[start..self.pos])
            .map_err(|e| e.to_string())?
            .to_string();
        self.maybe_compound(name)
    }

    fn maybe_compound(&mut self, functor: String) -> Result<Term, String> {
        self.skip_ws();
        if self.peek() == Some(b'(') {
            self.next();
            let args = self.parse_args()?;
            Ok(Term::Compound { functor, args })
        } else {
            Ok(Term::Atom(functor))
        }
    }

    fn parse_args(&mut self) -> Result<Vec<Term>, String> {
        let mut args = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b')') {
            self.next();
            return Ok(args);
        }
        loop {
            args.push(self.parse_term()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.next();
                }
                Some(b')') => {
                    self.next();
                    break;
                }
                Some(b) => {
                    return Err(format!(
                        "expected ',' or ')' in args, got '{}'",
                        b as char
                    ))
                }
                None => return Err("unexpected end of args".into()),
            }
        }
        Ok(args)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_rule() {
        let rules = parse_prolog_rules("fire(Where) :- smoke(Where).");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].body.len(), 1);
        assert!(matches!(&rules[0].body[0], BodyGoal::Positive(_)));
    }

    #[test]
    fn parse_conjunction_rule() {
        let rules = parse_prolog_rules("lemonade(Drink) :- sour(Drink), sweet(Drink).");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].body.len(), 2);
    }

    #[test]
    fn parse_negation_in_body() {
        let rules = parse_prolog_rules("ok(X) :- good(X), \\+ bad(X).");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].body.len(), 2);
        assert!(matches!(&rules[0].body[0], BodyGoal::Positive(_)));
        assert!(matches!(&rules[0].body[1], BodyGoal::Negative(_)));
    }

    #[test]
    fn parse_not1_treated_as_negative() {
        // not/1 is negation-as-failure, same as \+
        let rules = parse_prolog_rules("nonanimal(X) :- not(animal(X)).");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].body.len(), 1);
        assert!(matches!(&rules[0].body[0], BodyGoal::Negative(_)));
    }

    #[test]
    fn transduce_not1_generates_not_pattern() {
        let clp = transduce_source("nonanimal(X) :- not(animal(X)).");
        assert!(clp.contains("transduced-nonanimal-on-not_animal-0"));
        assert!(clp.contains("(not_animal ?X)"));
        assert_eq!(clp.matches("(defrule").count(), 1);
    }

    #[test]
    fn transduce_mixed_not1_and_positive() {
        // mammal(X) :- vertebrata(X), has(X,warm_blooded), not(has(X,feather)).
        // Should generate 3 defrules: vertebrata, has(warm_blooded), not_has(feather).
        let clp = transduce_source(
            "mammal(X) :- vertebrata(X), has(X,warm_blooded), not(has(X,feather))."
        );
        assert!(clp.contains("transduced-mammal-on-vertebrata-0"));
        assert!(clp.contains("transduced-mammal-on-has-1"));
        assert!(clp.contains("transduced-mammal-on-not_has-2"));
        assert!(clp.contains("(not_has ?X feather)"));
        assert_eq!(clp.matches("(defrule").count(), 3);
    }

    #[test]
    fn transduce_not_with_multi_arg() {
        // not(has(X, backbone)) binds X from the trigger
        let clp = transduce_source("nonvertebrata(X) :- animal(X), not(has(X,backbone)).");
        assert!(clp.contains("transduced-nonvertebrata-on-not_has-1"));
        assert!(clp.contains("(not_has ?X backbone)"));
        // X is bound by not_has trigger → str-cat for nonvertebrata head
        assert!(clp.contains("(str-cat \"nonvertebrata(\" ?X \")\""));
    }

    #[test]
    fn parse_fact_yields_empty_body() {
        let rules = parse_prolog_rules("mortal(stan).");
        assert_eq!(rules.len(), 1);
        assert!(rules[0].body.is_empty());
    }

    #[test]
    fn parse_skips_comments_and_blank_lines() {
        let src = "% This is a comment\n\nfire(W) :- smoke(W).";
        let rules = parse_prolog_rules(src);
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn parse_multiple_rules() {
        let src = "fire(W) :- smoke(W).\nalarm(W) :- smoke(W).";
        let rules = parse_prolog_rules(src);
        assert_eq!(rules.len(), 2);
    }

    #[test]
    fn transduce_fact_produces_no_output() {
        let clp = transduce_source("mortal(stan).");
        assert!(clp.is_empty());
    }

    #[test]
    fn transduce_fire_smoke() {
        let clp = transduce_source("fire(Where) :- smoke(Where).");
        assert!(clp.contains("transduced-fire-on-smoke-0"));
        assert!(clp.contains("(smoke ?Where)"));
        assert!(clp.contains("(str-cat \"fire(\" ?Where \")\")"));
    }

    #[test]
    fn transduce_lemonade_two_triggers() {
        let clp = transduce_source("lemonade(Drink) :- sour(Drink), sweet(Drink).");
        assert!(clp.contains("transduced-lemonade-on-sour-0"));
        assert!(clp.contains("transduced-lemonade-on-sweet-1"));
        assert!(clp.contains("(sour ?Drink)"));
        assert!(clp.contains("(sweet ?Drink)"));
    }

    #[test]
    fn transduce_unbound_head_variable_skips_rule() {
        // B is never bound by the trigger cond(A) — the generated goal would
        // contain a free Prolog variable, causing instantiation errors at
        // runtime.  The rule must be skipped entirely.
        let clp = transduce_source("head(A, B) :- cond(A).");
        assert!(clp.is_empty(), "expected no defrule when a head variable is unbound, got:\n{clp}");
    }

    #[test]
    fn transduce_negative_condition_generates_not_pattern() {
        let clp = transduce_source("ok(X) :- good(X), \\+ bad(X).");
        assert!(clp.contains("transduced-ok-on-good-0"));
        assert!(clp.contains("transduced-ok-on-not_bad-1"));
        assert!(clp.contains("(good ?X)"));
        assert!(clp.contains("(not_bad ?X)"));
        assert_eq!(clp.matches("(defrule").count(), 2);
    }

    #[test]
    fn transduce_multiple_rules_counter_increments() {
        let clp = transduce_source("fire(W) :- smoke(W).\nalarm(W) :- smoke(W).");
        assert!(clp.contains("transduced-fire-on-smoke-0"));
        assert!(clp.contains("transduced-alarm-on-smoke-1"));
    }

    #[test]
    fn transduce_nullary_head() {
        // Head with no args — emitted as plain string literal
        let clp = transduce_source("alert :- smoke(X).");
        assert!(clp.contains("\"alert\""));
    }

    // ── decorate_source ───────────────────────────────────────────────────────

    #[test]
    fn decorate_adds_listen_and_updated_rule() {
        let src = ":- dynamic(murder/1).\nmurder(mittens).\n";
        let pl = decorate_source(src);
        assert!(pl.contains(":- prolog_listen(murder/1, updated(murder/1))."));
        assert!(pl.contains("updated(Pred, Action, Context) :-"));
        assert!(pl.contains("coire_publish_assert(Head)"));
    }

    #[test]
    fn decorate_original_source_preserved_verbatim() {
        let src = ":- dynamic(murder/1).\nmurder(mittens).\naccuse(X) :- murder(V), suspect(X).\n";
        let pl = decorate_source(src);
        assert!(pl.contains(":- dynamic(murder/1)."));
        assert!(pl.contains("murder(mittens)."));
        assert!(pl.contains("accuse(X) :- murder(V), suspect(X)."));
        // Rules must NOT be rewritten (no coire_publish_assert in rule bodies)
        assert!(!pl.contains("accuse(X) :- murder(V), suspect(X), coire_publish_assert"));
    }

    #[test]
    fn decorate_multiple_dynamic_predicates() {
        let src = ":- dynamic(murder/1).\n:- dynamic(suspect/1).\n:- dynamic(dislikes/2).\n";
        let pl = decorate_source(src);
        assert!(pl.contains(":- prolog_listen(murder/1, updated(murder/1))."));
        assert!(pl.contains(":- prolog_listen(suspect/1, updated(suspect/1))."));
        assert!(pl.contains(":- prolog_listen(dislikes/2, updated(dislikes/2))."));
    }

    #[test]
    fn decorate_no_dynamic_still_adds_updated_rule() {
        let src = "fire(Where) :- smoke(Where).\n";
        let pl = decorate_source(src);
        assert!(pl.contains("updated(Pred, Action, Context) :-"));
        assert!(!pl.contains("prolog_listen"));
    }

    #[test]
    fn decorate_comment_delimiters_present() {
        let pl = decorate_source(":- dynamic(foo/1).\n");
        assert!(pl.contains("% ── Clara integration"));
        assert!(pl.contains("% ── End Clara integration"));
    }

    // ── effective_trigger / meta-predicate handling ───────────────────────────

    #[test]
    fn transduce_assertz_is_skipped() {
        // All head variables (Who, Group) are bound by the member_of trigger.
        // assertz is a meta-predicate and must not generate its own defrule.
        let clp = transduce_source(
            "prejudiced(Who,Group) :- member_of(Who,Group), assertz(criminal(Who)).",
        );
        // assertz is a meta-predicate — skip it entirely, do NOT generate a rule for it
        assert!(!clp.contains("on-assertz-"));
        assert!(!clp.contains("(assertz"));
        // No CLIPS fact pattern for criminal (it may appear in the comment line)
        assert!(!clp.contains("\n    (criminal"));
        // Only member_of triggers a defrule
        assert!(clp.contains("transduced-prejudiced-on-member_of-0"));
        assert_eq!(clp.matches("(defrule").count(), 1);
    }

    #[test]
    fn transduce_skips_retract() {
        let clp = transduce_source("clean(X) :- dirty(X), retract(dirty(X)).");
        assert!(clp.contains("transduced-clean-on-dirty-0"));
        // retract generates no defrule (check CLIPS pattern, not the comment)
        assert!(!clp.contains("on-retract-"));
        assert!(!clp.contains("(retract"));
        assert_eq!(clp.matches("(defrule").count(), 1);
    }

    #[test]
    fn transduce_skips_io_predicates() {
        // writeln/1 is a meta-predicate — no defrule should be generated for it
        let clp = transduce_source("logged(X) :- event(X), writeln(X).");
        assert_eq!(clp.matches("(defrule").count(), 1);
        assert!(!clp.contains("on-writeln-"));
        assert!(!clp.contains("(writeln"));
    }

    #[test]
    fn transduce_quoted_atom_arg_is_single_quoted_in_goal() {
        // suggestion(Visitor,'Greet the visitor.') :- visitor(Visitor), \+ greeted(Visitor).
        // The atom 'Greet the visitor.' needs single-quote wrapping in the Prolog goal
        // string — otherwise Prolog sees a bare multi-word term and raises a syntax/
        // instantiation error.
        let src = "suggestion(Visitor,'Greet the visitor.') :- visitor(Visitor), \\+ greeted(Visitor).";
        let clp = transduce_source(src);
        // Both triggers should produce defrules.
        assert_eq!(clp.matches("(defrule").count(), 2);
        // The atom must appear with single quotes inside the goal string.
        assert!(
            clp.contains("'Greet the visitor.'"),
            "expected single-quoted atom in goal string, got:\n{clp}"
        );
        // Must NOT appear as a bare unquoted string.
        assert!(
            !clp.contains(",Greet the visitor.)"),
            "atom was emitted bare (unquoted) in goal string:\n{clp}"
        );
    }

    // ── decorate_source module imports ────────────────────────────────────────

    #[test]
    fn decorate_injects_missing_module_imports() {
        let pl = decorate_source("fire(W) :- smoke(W).\n");
        assert!(pl.contains(":- use_module(library(the_rabbit))."));
        assert!(pl.contains(":- use_module(library(the_rat))."));
        assert!(pl.contains(":- use_module(library(the_coire))."));
    }

    #[test]
    fn decorate_does_not_duplicate_existing_imports() {
        let src = ":- use_module(library(the_rat)).\nfire(W) :- smoke(W).\n";
        let pl = decorate_source(src);
        assert_eq!(pl.matches("use_module(library(the_rat))").count(), 1);
        assert!(pl.contains(":- use_module(library(the_rabbit))."));
    }

    // ── transduce_graph (edge transduction) ───────────────────────────────────

    fn two_node_graph(edge_extra: &str) -> String {
        format!(
            r#"{{
              "version": 1,
              "nodes": [
                {{"id": "n1", "type": "daemon", "evaluatorName": "clara_mind_splinter", "label": "Clara"}},
                {{"id": "n2", "type": "daemon", "evaluatorName": "groq_evaluator", "label": "Clara/Groq"}}
              ],
              "edges": [
                {{"id": "e1", "source": "n1", "target": "n2", "flowKind": "unicast"{}}}
              ]
            }}"#,
            edge_extra
        )
    }

    #[test]
    fn graph_offering_edge_generates_consult_helper() {
        let result = transduce_graph(&two_node_graph("")).unwrap();
        let source = &result.per_node["n1"];
        assert!(
            source.prolog.contains("consult_clara_groq(Payload, Result) :-"),
            "helper named from target label, got:\n{}", source.prolog
        );
        assert!(source.prolog.contains("caws_consult('n2', 'consults/e1', Payload, Result)"));
        // Reply hook on the source's CLIPS side
        assert!(source.clips.contains("(defrule edge-e1-on-reply"));
        assert!(source.clips.contains("(coire-event (origin \"ritual/hohi\") (topic \"consults/e1\"))"));
        assert!(source.clips.contains("edge_replied('e1')"));
        // No target snippet without an assertion qualifier
        assert!(!result.per_node.contains_key("n2"));
    }

    #[test]
    fn graph_boolean_nl_qualifier_guards_with_clara_fy() {
        let result = transduce_graph(&two_node_graph(
            r#", "qualifierKind": "boolean", "qualifierValue": "the antimatter core is critical""#,
        ))
        .unwrap();
        let prolog = &result.per_node["n1"].prolog;
        assert!(
            prolog.contains("clara_fy(\"the antimatter core is critical\", true)"),
            "NL guard must classify via clara_fy, got:\n{prolog}"
        );
    }

    #[test]
    fn graph_boolean_literal_qualifier_spliced_as_goal() {
        let result = transduce_graph(&two_node_graph(
            r#", "qualifierKind": "boolean", "qualifierValue": "core_temp(T),T>9000""#,
        ))
        .unwrap();
        let prolog = &result.per_node["n1"].prolog;
        assert!(prolog.contains("core_temp(T),T>9000,\n"), "literal guard spliced:\n{prolog}");
        assert!(!prolog.contains("clara_fy"));
    }

    #[test]
    fn graph_assertion_qualifier_seeds_target_fact() {
        let result = transduce_graph(&two_node_graph(
            r#", "qualifierKind": "assertion", "qualifierValue": "core_temp(9500)""#,
        ))
        .unwrap();
        let target = &result.per_node["n2"];
        assert!(target.prolog.contains("core_temp(9500)."), "fact with period:\n{}", target.prolog);
    }

    #[test]
    fn graph_unknown_msg_type_edge_generates_comment_only() {
        // "event"/"hohi"/"tabu" are recognized message-edge kinds now (see
        // the message-edge tests below); a truly unrecognized msgType still
        // falls back to comment-only, no generated code.
        let result = transduce_graph(&two_node_graph(r#", "msgType": "mystery""#)).unwrap();
        let source = &result.per_node["n1"];
        assert!(source.prolog.contains("carries 'mystery' messages"));
        assert!(!source.prolog.contains("caws_consult"));
        assert!(!source.prolog.contains("caws_emit"));
        assert!(source.clips.is_empty());
    }

    // ── message edges (event/hohi/tabu) ───────────────────────────────────────

    #[test]
    fn graph_manual_event_edge_generates_emit_helper_and_target_dispatch_no_tee() {
        let result = transduce_graph(&two_node_graph(r#", "msgType": "event""#)).unwrap();
        let source = &result.per_node["n1"];
        assert!(
            source.prolog.contains("emit_clara_groq_event(Payload) :-"),
            "helper named from target label + kind, got:\n{}", source.prolog
        );
        assert!(source.prolog.contains("caws_emit('n2', 'event/e1', event, Payload)"));
        assert!(!source.prolog.contains("caws_tee"), "manual mode must not generate a tee");
        assert!(!source.clips.contains("auto-tee"));

        let target = &result.per_node["n2"];
        assert!(target.clips.contains("(defrule edge-e1-on-message"));
        assert!(target.clips.contains(
            "(coire-event (origin \"ritual/event\") (topic \"event/e1\") (correlation ?cid&~\"\"))"
        ));
        assert!(target.clips.contains(
            "(coire-publish-goal (str-cat \"caws_edge_message('e1', event, '\" ?cid \"')\"))"
        ));
    }

    #[test]
    fn graph_auto_event_edge_generates_tee_wrapper_and_trigger() {
        let result = transduce_graph(&two_node_graph(
            r#", "msgType": "event", "pipeMode": "auto""#,
        ))
        .unwrap();
        let source = &result.per_node["n1"];
        assert!(
            source.prolog.contains("caws_auto_tee_e1(Cid) :-"),
            "tee wrapper generated:\n{}", source.prolog
        );
        assert!(source.prolog.contains("caws_tee('e1', 'n2', 'event/e1', event, Cid)"));
        assert!(source.prolog.contains("caws_auto_tee_e1(_)."), "catch-all clause");
        assert!(source.clips.contains("(defrule edge-e1-auto-tee-event"));
        assert!(source.clips.contains(
            "(coire-event (origin \"ritual/event\") (correlation ?cid&~\"\"))"
        ));
        assert!(source.clips.contains(
            "(coire-publish-goal (str-cat \"caws_auto_tee_e1('\" ?cid \"')\"))"
        ));
        // Manual emit helper still generated alongside the tee.
        assert!(source.prolog.contains("emit_clara_groq_event(Payload) :-"));
    }

    #[test]
    fn graph_auto_tabu_edge_generates_exactly_two_tee_triggers() {
        let result = transduce_graph(&two_node_graph(
            r#", "msgType": "tabu", "pipeMode": "auto""#,
        ))
        .unwrap();
        let source = &result.per_node["n1"];
        assert!(source.clips.contains("(defrule edge-e1-auto-tee-tabu\n"));
        assert!(source.clips.contains(
            "(coire-event (origin \"ritual/tabu\") (correlation ?cid&~\"\"))"
        ));
        assert!(source.clips.contains("(defrule edge-e1-auto-tee-tabu-timeout"));
        assert!(source.clips.contains(
            "(coire-event (origin \"ritual/tabu-timeout\") (correlation ?cid&~\"\"))"
        ));
        // Both triggers invoke the same wrapper — no separate timeout wrapper.
        assert_eq!(source.prolog.matches("caws_auto_tee_e1(Cid) :-").count(), 1);
        assert_eq!(source.clips.matches("caws_auto_tee_e1('\" ?cid \"')").count(), 2);
        // Wrapper always tees with Kind=tabu (no wire-level tabu-timeout label).
        assert!(source.prolog.contains("caws_tee('e1', 'n2', 'tabu/e1', tabu, Cid)"));
    }

    #[test]
    fn graph_auto_hohi_edge_generates_exactly_one_tee_trigger() {
        let result = transduce_graph(&two_node_graph(
            r#", "msgType": "hohi", "pipeMode": "auto""#,
        ))
        .unwrap();
        let source = &result.per_node["n1"];
        assert!(source.clips.contains("(defrule edge-e1-auto-tee-hohi"));
        assert!(!source.clips.contains("auto-tee-tabu"));
        assert!(source.prolog.contains("caws_tee('e1', 'n2', 'hohi/e1', hohi, Cid)"));
    }

    #[test]
    fn graph_message_edge_boolean_qualifier_guards_helper_and_wrapper() {
        let result = transduce_graph(&two_node_graph(
            r#", "msgType": "event", "pipeMode": "auto", "qualifierKind": "boolean", "qualifierValue": "core_temp(T),T>9000""#,
        ))
        .unwrap();
        let prolog = &result.per_node["n1"].prolog;
        assert!(prolog.contains(
            "emit_clara_groq_event(Payload) :-\n    core_temp(T),T>9000,"
        ), "{prolog}");
        assert!(prolog.contains(
            "caws_auto_tee_e1(Cid) :-\n    core_temp(T),T>9000,"
        ), "{prolog}");
    }

    #[test]
    fn graph_message_edge_assertion_qualifier_seeds_target_fact() {
        let result = transduce_graph(&two_node_graph(
            r#", "msgType": "hohi", "qualifierKind": "assertion", "qualifierValue": "core_temp(9500)""#,
        ))
        .unwrap();
        let target = &result.per_node["n2"];
        assert!(target.prolog.contains("core_temp(9500)."), "fact with period:\n{}", target.prolog);
    }

    #[test]
    fn graph_message_edge_topic_suffix_overrides_edge_id_channel() {
        let result = transduce_graph(&two_node_graph(
            r#", "msgType": "event", "topicSuffix": "psych evals""#,
        ))
        .unwrap();
        let prolog = &result.per_node["n1"].prolog;
        assert!(prolog.contains("'event/psych-evals'"), "sanitized suffix:\n{prolog}");
    }

    #[test]
    fn graph_auto_message_edge_sanitizes_ugly_edge_ids() {
        let graph = r#"{
          "nodes": [
            {"id": "n1", "type": "daemon", "evaluatorName": "clara_mind_splinter", "label": "Clara"},
            {"id": "n2", "type": "daemon", "evaluatorName": "groq_evaluator", "label": "Groq"}
          ],
          "edges": [
            {"id": "edge 9!", "source": "n1", "target": "n2", "msgType": "event", "pipeMode": "auto"}
          ]
        }"#;
        let result = transduce_graph(graph).unwrap();
        let source = &result.per_node["n1"];
        assert!(source.prolog.contains("caws_auto_tee_edge_9_(Cid) :-"), "{}", source.prolog);
        assert!(source.prolog.contains("caws_tee('edge 9!', 'n2', 'event/edge-9-', event, Cid)"));
        assert!(source.clips.contains("(defrule edge-edge_9_-auto-tee-event"));
        let target = &result.per_node["n2"];
        assert!(target.clips.contains("caws_edge_message('edge 9!', event, '"));
    }

    #[test]
    fn graph_topic_suffix_overrides_edge_id_channel() {
        let result = transduce_graph(&two_node_graph(
            r#", "topicSuffix": "psych evals""#,
        ))
        .unwrap();
        let prolog = &result.per_node["n1"].prolog;
        assert!(prolog.contains("'consults/psych-evals'"), "sanitized suffix:\n{prolog}");
    }

    #[test]
    fn graph_manual_edge_generates_reply_dispatch_but_no_pipe() {
        let result = transduce_graph(&two_node_graph("")).unwrap();
        let source = &result.per_node["n1"];
        // Typed reply dispatch is generated in BOTH modes…
        assert!(source.clips.contains("(defrule edge-e1-on-hohi-result"));
        assert!(source.clips.contains(
            "(coire-publish-goal (str-cat \"caws_edge_reply('e1', hohi, '\" ?cid \"')\"))"
        ));
        assert!(source.clips.contains("(defrule edge-e1-on-tabu-result"));
        assert!(source.clips.contains(
            "(coire-publish-goal (str-cat \"caws_edge_reply('e1', tabu, '\" ?cid \"')\"))"
        ));
        assert!(source.clips.contains("(defrule edge-e1-on-timeout-result"));
        assert!(source.clips.contains(
            "(coire-publish-goal (str-cat \"caws_edge_reply('e1', tabu_timeout, '\" ?cid \"')\"))"
        ));
        // …the legacy reply hook survives…
        assert!(source.clips.contains("edge_replied('e1')"));
        // …but nothing pipes without pipeMode auto.
        assert!(!source.prolog.contains("caws_auto_pipe"), "{}", source.prolog);
        assert!(!source.clips.contains("auto-pipe"), "{}", source.clips);
    }

    #[test]
    fn graph_auto_edge_generates_pipe_wrapper_and_rule() {
        let result = transduce_graph(&two_node_graph(r#", "pipeMode": "auto""#)).unwrap();
        let source = &result.per_node["n1"];
        assert!(
            source.prolog.contains("caws_auto_pipe_e1(Cid) :-"),
            "pipe wrapper generated:\n{}", source.prolog
        );
        assert!(source.prolog.contains("caws_pipe('e1', 'n2', 'consults/e1', Cid)"));
        assert!(source.prolog.contains("caws_auto_pipe_e1(_)."), "catch-all clause");
        assert!(source.clips.contains("(defrule edge-e1-auto-pipe"));
        assert!(source.clips.contains(
            "(coire-event (origin \"ritual/offering\") (correlation ?cid&~\"\"))"
        ));
        assert!(source.clips.contains(
            "(coire-publish-goal (str-cat \"caws_auto_pipe_e1('\" ?cid \"')\"))"
        ));
        // Synchronous consult helper is unchanged alongside the pipe.
        assert!(source.prolog.contains("consult_clara_groq(Payload, Result) :-"));
        assert!(source.prolog.contains("caws_consult('n2', 'consults/e1', Payload, Result)"));
    }

    #[test]
    fn graph_auto_edge_boolean_qualifier_guards_pipe() {
        let result = transduce_graph(&two_node_graph(
            r#", "pipeMode": "auto", "qualifierKind": "boolean", "qualifierValue": "core_temp(T),T>9000""#,
        ))
        .unwrap();
        let prolog = &result.per_node["n1"].prolog;
        // Guard appears in both the consult helper and the pipe wrapper.
        assert!(prolog.contains(
            "consult_clara_groq(Payload, Result) :-\n    core_temp(T),T>9000,"
        ), "{prolog}");
        assert!(prolog.contains(
            "caws_auto_pipe_e1(Cid) :-\n    core_temp(T),T>9000,"
        ), "{prolog}");
    }

    #[test]
    fn graph_auto_edge_sanitizes_ugly_edge_ids() {
        let graph = r#"{
          "nodes": [
            {"id": "n1", "type": "daemon", "evaluatorName": "clara_mind_splinter", "label": "Clara"},
            {"id": "n2", "type": "daemon", "evaluatorName": "groq_evaluator", "label": "Groq"}
          ],
          "edges": [
            {"id": "edge 9!", "source": "n1", "target": "n2", "pipeMode": "auto"}
          ]
        }"#;
        let result = transduce_graph(graph).unwrap();
        let source = &result.per_node["n1"];
        // Functor and rule names sanitized; raw id survives single-quoted.
        assert!(source.prolog.contains("caws_auto_pipe_edge_9_(Cid) :-"), "{}", source.prolog);
        assert!(source.prolog.contains("caws_pipe('edge 9!', 'n2', 'consults/edge-9-', Cid)"));
        assert!(source.clips.contains("(defrule edge-edge_9_-auto-pipe"));
        assert!(source.clips.contains("caws_edge_reply('edge 9!', hohi, '"));
    }

    #[test]
    fn graph_edges_to_unknown_or_self_are_ignored() {
        let graph = r#"{
          "nodes": [{"id": "n1", "type": "daemon", "evaluatorName": "clara_mind_splinter"}],
          "edges": [
            {"id": "e1", "source": "n1", "target": "ghost"},
            {"id": "e2", "source": "n1", "target": "n1"}
          ]
        }"#;
        let result = transduce_graph(graph).unwrap();
        assert!(result.per_node.is_empty());
    }

    #[test]
    fn graph_invalid_json_errors() {
        assert!(transduce_graph("{nope").is_err());
    }
}
