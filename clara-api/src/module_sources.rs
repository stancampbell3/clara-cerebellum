//! Prolog module fragments for `/deduce` (ritual_properties_spec.md, P4 Tier 1).
//!
//! A *module* is a registered source of type `prolog-module`. A run loads its modules, in the order given, into the
//! same flat namespace as the node source and ahead of it. Because `consult_string` asserts every clause into one
//! namespace, two sources defining the same `Name/Arity` would silently merge, so before loading we check that no
//! predicate is defined by more than one source and that no module redefines a name the compiled-in overlay libraries
//! export. A missing or wrongly typed module is a hard error, unlike the node source's warn-and-fall-back.

use std::collections::BTreeMap;
use std::fmt;

use clara_coire::CoireStore;
use clara_prolog::PrologEnvironment;
use serde::Serialize;
use uuid::Uuid;

/// `source_type` a registered module carries.
pub const MODULE_SOURCE_TYPE: &str = "prolog-module";
/// Label used for the compiled-in overlay libraries in a conflict report.
pub const OVERLAY_LABEL: &str = "compiled-in overlay library";

#[derive(Debug, Clone)]
pub struct ResolvedModule {
    pub id:      Uuid,
    /// The registered label (lildaemon uses `name@version`), else the id.
    pub label:   String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conflict {
    /// `Name/Arity`.
    pub predicate: String,
    /// Labels of every source defining it (plus the overlay label when it is an overlay export).
    pub sources:   Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleError {
    /// Modules were requested but persistence (the source registry) is not configured.
    NoStore,
    Missing(Uuid),
    WrongType { id: Uuid, found: String },
    Lookup(String),
    /// Reading the sources' predicates failed (e.g. a syntax error in a module).
    Inspect(String),
    Conflicts(Vec<Conflict>),
}

impl fmt::Display for ModuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModuleError::NoStore => write!(f, "module sources need persistence (a source registry) to be configured"),
            ModuleError::Missing(id) => write!(f, "module source {id} not found"),
            ModuleError::WrongType { id, found } => {
                write!(f, "source {id} is type '{found}', not '{MODULE_SOURCE_TYPE}'")
            }
            ModuleError::Lookup(e) => write!(f, "module source lookup failed: {e}"),
            ModuleError::Inspect(e) => write!(f, "could not read module predicates: {e}"),
            ModuleError::Conflicts(cs) => {
                let parts: Vec<String> = cs
                    .iter()
                    .map(|c| format!("{} defined by {}", c.predicate, c.sources.join(", ")))
                    .collect();
                write!(f, "module predicate conflict: {}", parts.join("; "))
            }
        }
    }
}

/// Resolve module ids, keeping their order and dropping repeats. Any missing or wrongly typed id is an error.
pub fn resolve_module_sources(
    ids:   &[Uuid],
    store: Option<&CoireStore>,
) -> Result<Vec<ResolvedModule>, ModuleError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let store = store.ok_or(ModuleError::NoStore)?;
    let mut seen = std::collections::HashSet::new();
    let mut resolved = Vec::new();
    for id in ids {
        if !seen.insert(*id) {
            continue;
        }
        match store.sources.get(*id) {
            Ok(Some(entry)) if entry.source_type == MODULE_SOURCE_TYPE => resolved.push(ResolvedModule {
                id:      *id,
                label:   entry.label.unwrap_or_else(|| id.to_string()),
                content: entry.content,
            }),
            Ok(Some(entry)) => {
                return Err(ModuleError::WrongType { id: *id, found: entry.source_type });
            }
            Ok(None) => return Err(ModuleError::Missing(*id)),
            Err(e) => return Err(ModuleError::Lookup(e.to_string())),
        }
    }
    Ok(resolved)
}

/// One source's defined predicates, as input to [`find_conflicts`].
pub struct SourcePredicates {
    pub label:     String,
    pub is_module: bool,
    pub defined:   Vec<String>,
}

/// Pure conflict detection. A predicate defined by two or more sources conflicts; so does a predicate a *module*
/// defines that the overlay libraries export (the node source is never checked against the overlay: that is today's
/// behavior and stays unchanged). Sorted by predicate, so output is deterministic.
pub fn find_conflicts(sources: &[SourcePredicates], overlay: &[String]) -> Vec<Conflict> {
    let mut by_pred: BTreeMap<&str, Vec<&SourcePredicates>> = BTreeMap::new();
    for src in sources {
        for p in &src.defined {
            by_pred.entry(p.as_str()).or_default().push(src);
        }
    }
    let overlay: std::collections::HashSet<&str> = overlay.iter().map(String::as_str).collect();
    let mut conflicts = Vec::new();
    for (pred, defs) in by_pred {
        let shadows_overlay = overlay.contains(pred) && defs.iter().any(|d| d.is_module);
        if defs.len() > 1 || shadows_overlay {
            let mut labels: Vec<String> = defs.iter().map(|d| d.label.clone()).collect();
            if shadows_overlay {
                labels.push(OVERLAY_LABEL.to_string());
            }
            conflicts.push(Conflict { predicate: pred.to_string(), sources: labels });
        }
    }
    conflicts
}

/// Inspect the node source and every module with `env` (nothing is loaded) and return the conflicts.
/// No modules means no check and no engine work.
pub fn check_modules(
    env:          &PrologEnvironment,
    node_label:   &str,
    node_content: &str,
    modules:      &[ResolvedModule],
) -> Result<Vec<Conflict>, ModuleError> {
    if modules.is_empty() {
        return Ok(Vec::new());
    }
    let inspect = |code: &str| env.source_predicates(code).map_err(|e| ModuleError::Inspect(e.to_string()));
    let mut sources = Vec::with_capacity(modules.len() + 1);
    for m in modules {
        let defined = inspect(&m.content).map_err(|e| match e {
            ModuleError::Inspect(msg) => ModuleError::Inspect(format!("{}: {msg}", m.label)),
            other => other,
        })?;
        sources.push(SourcePredicates { label: m.label.clone(), is_module: true, defined });
    }
    if !node_content.trim().is_empty() {
        sources.push(SourcePredicates {
            label:     node_label.to_string(),
            is_module: false,
            defined:   inspect(node_content)?,
        });
    }
    let overlay = env.overlay_exports().map_err(|e| ModuleError::Inspect(e.to_string()))?;
    Ok(find_conflicts(&sources, &overlay))
}

/// Effective clauses for a run: module contents in order, then the node clauses. One `seed_prolog` call loads all.
pub fn with_modules(modules: &[ResolvedModule], node_clauses: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = modules.iter().map(|m| m.content.clone()).collect();
    out.extend(node_clauses);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(label: &str, is_module: bool, defined: &[&str]) -> SourcePredicates {
        SourcePredicates {
            label: label.to_string(),
            is_module,
            defined: defined.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn disjoint_sources_do_not_conflict() {
        let s = [src("a@1", true, &["f/1", "g/2"]), src("b@1", true, &["h/1"]), src("node", false, &["run/1"])];
        assert!(find_conflicts(&s, &["strip_think/2".into()]).is_empty());
    }

    #[test]
    fn module_vs_module_and_module_vs_node_conflict() {
        let s = [src("a@1", true, &["f/1", "dup/1"]), src("b@1", true, &["dup/1"]), src("node", false, &["f/1"])];
        let c = find_conflicts(&s, &[]);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0], Conflict { predicate: "dup/1".into(), sources: vec!["a@1".into(), "b@1".into()] });
        assert_eq!(c[1], Conflict { predicate: "f/1".into(), sources: vec!["a@1".into(), "node".into()] });
    }

    #[test]
    fn a_module_shadowing_an_overlay_export_conflicts_but_the_node_source_does_not() {
        let overlay = vec!["strip_think/2".to_string()];
        let c = find_conflicts(&[src("m@1", true, &["strip_think/2"])], &overlay);
        assert_eq!(c, vec![Conflict {
            predicate: "strip_think/2".into(),
            sources:   vec!["m@1".into(), OVERLAY_LABEL.into()],
        }]);
        // Node-only redefinition of an overlay name is today's behavior: never flagged here.
        assert!(find_conflicts(&[src("node", false, &["strip_think/2"])], &overlay).is_empty());
    }

    #[test]
    fn a_predicate_in_both_a_module_and_the_overlay_is_reported_once() {
        let overlay = vec!["x/1".to_string()];
        let c = find_conflicts(&[src("a", true, &["x/1"]), src("b", true, &["x/1"])], &overlay);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].sources, vec!["a".to_string(), "b".to_string(), OVERLAY_LABEL.to_string()]);
    }

    #[test]
    fn with_modules_puts_modules_first_in_declared_order() {
        let m = |c: &str| ResolvedModule { id: Uuid::new_v4(), label: c.into(), content: c.into() };
        let out = with_modules(&[m("mod_a."), m("mod_b.")], vec!["node.".into()]);
        assert_eq!(out, vec!["mod_a.", "mod_b.", "node."]);
    }

    #[test]
    fn no_modules_means_no_resolution_and_no_store_needed() {
        assert!(resolve_module_sources(&[], None).unwrap().is_empty());
    }

    #[test]
    fn modules_without_a_store_are_an_error() {
        let err = resolve_module_sources(&[Uuid::new_v4()], None).unwrap_err();
        assert_eq!(err, ModuleError::NoStore);
    }

    fn temp_store() -> (CoireStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = CoireStore::open(dir.path().join("t.duckdb")).unwrap();
        (store, dir)
    }

    #[test]
    fn resolves_registered_modules_in_order_and_drops_repeats() {
        let (store, _d) = temp_store();
        let (a, _) = store.sources.register(MODULE_SOURCE_TYPE, Some("a@1.0.0"), "a_fact(1).", None).unwrap();
        let (b, _) = store.sources.register(MODULE_SOURCE_TYPE, None, "b_fact(2).", None).unwrap();
        let resolved = resolve_module_sources(&[b, a, b], Some(&store)).unwrap();
        assert_eq!(resolved.len(), 2, "the repeated id is dropped");
        assert_eq!(resolved[0].id, b);
        assert_eq!(resolved[0].label, b.to_string(), "no label falls back to the id");
        assert_eq!(resolved[1].label, "a@1.0.0");
        assert_eq!(resolved[1].content, "a_fact(1).");
    }

    #[test]
    fn a_missing_module_is_a_hard_error() {
        let (store, _d) = temp_store();
        let ghost = Uuid::new_v4();
        assert_eq!(resolve_module_sources(&[ghost], Some(&store)).unwrap_err(), ModuleError::Missing(ghost));
    }

    #[test]
    fn a_source_of_another_type_is_rejected() {
        let (store, _d) = temp_store();
        let (plain, _) = store.sources.register("prolog", None, "node_fact(1).", None).unwrap();
        match resolve_module_sources(&[plain], Some(&store)).unwrap_err() {
            ModuleError::WrongType { id, found } => {
                assert_eq!(id, plain);
                assert_eq!(found, "prolog");
            }
            other => panic!("expected WrongType, got {other:?}"),
        }
    }

    #[test]
    fn a_module_registered_without_expiry_is_never_reaped() {
        let (store, _d) = temp_store();
        let (id, _) = store.sources.register(MODULE_SOURCE_TYPE, Some("keep@1"), "k(1).", None).unwrap();
        let entry = store.sources.get(id).unwrap().unwrap();
        assert_eq!(entry.expires_at_ms, None);
        store.sources.sweep_expired(i64::MAX).unwrap();
        assert!(store.sources.get(id).unwrap().is_some());
    }

    #[test]
    fn check_modules_finds_real_conflicts_with_the_embedded_prolog() {
        let env = PrologEnvironment::new().unwrap();
        let m = |label: &str, content: &str| ResolvedModule {
            id: Uuid::new_v4(),
            label: label.into(),
            content: content.into(),
        };
        // disjoint: fine
        let ok = check_modules(&env, "node", "run(X) :- helper(X).", &[m("h@1", "helper(1).")]).unwrap();
        assert!(ok.is_empty(), "{ok:?}");
        // module vs node
        let c = check_modules(&env, "node", "helper(2).", &[m("h@1", "helper(1).")]).unwrap();
        assert_eq!(c[0].predicate, "helper/1");
        // module vs module
        let c = check_modules(&env, "node", "", &[m("a@1", "same(1)."), m("b@1", "same(2).")]).unwrap();
        assert_eq!(c[0].sources, vec!["a@1".to_string(), "b@1".to_string()]);
        // module vs overlay export
        let c = check_modules(&env, "node", "", &[m("s@1", "strip_think(A, A).")]).unwrap();
        assert_eq!(c[0].predicate, "strip_think/2");
        assert!(c[0].sources.contains(&OVERLAY_LABEL.to_string()));
        // node-only: never inspected, never flagged
        assert!(check_modules(&env, "node", "strip_think(A, A).", &[]).unwrap().is_empty());
        // a module with a syntax error reports which module
        match check_modules(&env, "node", "", &[m("bad@1", "broken( .")]) {
            Err(ModuleError::Inspect(msg)) => assert!(msg.starts_with("bad@1:"), "{msg}"),
            other => panic!("expected Inspect error, got {other:?}"),
        }
    }
}
