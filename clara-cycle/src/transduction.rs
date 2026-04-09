//! Prolog → CLIPS transduction: speculative forward-chaining rule generation.
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
//! # Example
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

use std::collections::{HashMap, HashSet};

use crate::transpile::{render_clips_fact, render_clips_field, render_prolog_term, Term};
use clara_dagda::TruthValue;

// ── Public types ──────────────────────────────────────────────────────────────

/// A single goal in a Prolog rule body.
#[derive(Debug, Clone, PartialEq)]
pub enum BodyGoal {
    /// A positive condition — can trigger a CLIPS defrule.
    Positive(Term),
    /// A negation-as-failure condition (`\+` / `not/1`).
    /// Generates a CLIPS defrule triggered by a `not_<functor>` fact —
    /// a positive assertion of a constructively-determined negation.
    Negative(Term),
}

/// A parsed Prolog rule: `head :- body.` or a bare fact `head.`
#[derive(Debug, Clone)]
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
    let preamble = generate_listen_preamble(&indicators);
    format!("{}\n{}", preamble, prolog_source)
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

/// Build the Clara integration preamble for a set of dynamic predicate indicators.
fn generate_listen_preamble(indicators: &[String]) -> String {
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

/// Options controlling DOT graph generation.
pub struct DotOptions {
    /// When true, emit a dashed gray undirected edge between condition nodes
    /// that share the same functor, arity, and arguments across different rules
    /// (e.g. two `turn(left)` nodes in separate rule bodies).
    pub link_shared_conditions: bool,
}

impl Default for DotOptions {
    fn default() -> Self {
        Self { link_shared_conditions: false }
    }
}

/// Per-node truth-value override for colored deduction snapshots.
///
/// Keys are predicate functor names (e.g. `"unlocked"`, `"tumbler_1"`).
/// When a node's functor is present, its fill color is overridden by the
/// truth-value palette instead of the structural default.
pub struct NodeColoring {
    pub values: HashMap<String, TruthValue>,
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

    // Direct head lookup: (functor, arity) → cluster index
    let head_by_fa: HashMap<(String, usize), usize> = rule_list.iter()
        .enumerate()
        .map(|(ci, (_orig_i, rule))| {
            let (f, a) = term_functor_arity(&rule.head);
            ((f.to_string(), a), ci)
        })
        .collect();

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
            let chain_target = head_by_fa.get(&(cf.to_string(), ca)).copied();

            if let Some(producers) = assert_producers {
                // Assert-bridge takes precedence: emit rule-head → rule-head edges.
                // Direction: this head requires what the producing rule asserts.
                // No condition node emitted for this goal.
                // TODO: consider per-edge term labels or tooltips in a future sprint.
                for &prod_ci in producers {
                    assert_bridge_edges.push((head_id.clone(), rule_head_ids[prod_ci].clone()));
                }
                // Secondary chains-to if condition also directly matches a rule head.
                if let Some(chain_ci) = chain_target {
                    out.push_str(&format!(
                        "    {} [label=\"{}\" shape=ellipse style=\"filled,dashed\" fillcolor=\"{}\"]\n",
                        cond_id, escape_dot_label(&cond_label), fill
                    ));
                    out.push_str(&format!("    {} -> {} [label=\"requires\"]\n", head_id, cond_id));
                    chains_to_edges.push((cond_id.clone(), rule_head_ids[chain_ci].clone()));
                    cond_ids.push((cond_id, cf.to_string(), ca, cond_label));
                }
            } else if let Some(chain_ci) = chain_target {
                // Chain-bridged: condition visible, chains-to edge deferred.
                out.push_str(&format!(
                    "    {} [label=\"{}\" shape=ellipse style=\"filled,dashed\" fillcolor=\"{}\"]\n",
                    cond_id, escape_dot_label(&cond_label), fill
                ));
                out.push_str(&format!("    {} -> {} [label=\"requires\"]\n", head_id, cond_id));
                chains_to_edges.push((cond_id.clone(), rule_head_ids[chain_ci].clone()));
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
}
