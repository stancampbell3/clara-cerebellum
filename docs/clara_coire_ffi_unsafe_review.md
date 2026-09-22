# clara-coire CLIPS FFI bridge — missing `unsafe` on pointer-dereferencing public functions

_For team review. Surfaced 2026-08-11 while wiring up GitLab CI (`clara-cerebellum/.gitlab-ci.yml`) on the new on-site GitLab instance — this is the first time `cargo clippy --workspace --all-targets` has ever actually run against this codebase. The old GitHub Actions `ci.yml` was an empty stub since the initial scaffold commit, so this is pre-existing lint debt, not a regression from anything done that day._

**RESOLVED 2026-09-22 — see "Resolution" section below.**

## Summary

`clara-coire/src/clips_bridge.rs` — the C-callable bridge linked into CLIPS via `userfunctions.c` — has 7 functions that are `pub` (or `pub extern "C"`) and dereference raw pointers internally, but are not themselves marked `unsafe fn`. `clippy::not_unsafe_ptr_arg_deref` is deny-by-default in the clippy version this workspace pins to (1.97.0) and fails the build:

```
error: this public function might dereference a raw pointer but is not marked `unsafe`
```

Current state: `cargo-clippy` is set to `allow_failure: true` in `.gitlab-ci.yml` specifically because of this, so it doesn't block the pipeline. `cargo test --workspace` is unaffected and green.

## Exact locations

| Function | File:line | What it does |
|---|---|---|
| `rust_coire_free_string` | `clara-coire/src/clips_bridge.rs:33` (deref at `:36`) | Frees a C string previously handed to CLIPS |
| `rust_coire_emit` | `clara-coire/src/clips_bridge.rs:44` (derefs at `:50,52,54`) | Emits an event to the Coire from CLIPS |
| `rust_coire_poll` | `clara-coire/src/clips_bridge.rs:80` (deref at `:82`) | Polls pending events for a session |
| `rust_coire_mark` | `clara-coire/src/clips_bridge.rs:104` (deref at `:106`) | Marks a single event processed |
| `rust_coire_count` | `clara-coire/src/clips_bridge.rs:126` (deref at `:128`) | Counts pending events for a session |

All 5 route through the private helper `unsafe fn cstr_to_str` (`:19`), which is already correctly marked `unsafe` — the lint is entirely about the *public* wrapper functions above it, not the helper itself.

## Why clippy flags this

These are `#[no_mangle] pub extern "C" fn` — real FFI entry points, meant to be called from CLIPS's C runtime, which is exactly where raw-pointer arguments are expected and unavoidable. That's not in question.

The issue is narrower: because the *Rust function signature* isn't marked `unsafe`, nothing stops other Rust code (anywhere in the crate, or anywhere downstream since these are `pub`) from calling them directly without an `unsafe` block — even though doing so can dereference an invalid/dangling pointer and cause undefined behavior if the caller doesn't uphold the same invariants CLIPS's C side is trusted to uphold (non-null, valid UTF-8, live for the duration of the call). Rust's own safety story asks that any function whose misuse can cause UB be marked `unsafe fn`, so the compiler forces every call site to opt in explicitly.

## Options

1. **Mark all 5 as `pub unsafe extern "C" fn`** (idiomatic fix). `extern "C"` linkage is unaffected — CLIPS's C side has no concept of Rust's `unsafe` keyword, so this is purely a Rust-side API contract change. Any in-crate Rust callers (tests, etc.) would need an `unsafe { }` block added at the call site. Standard practice for this kind of FFI boundary.
2. **`#[allow(clippy::not_unsafe_ptr_arg_deref)]` per function**, with a `# Safety` doc comment explaining the invariant CLIPS is trusted to uphold. Keeps the public signature safe-looking (no caller-side `unsafe` needed) at the cost of the lint no longer protecting against misuse from other Rust code.
3. **Status quo** — leave `cargo-clippy` as `allow_failure: true` indefinitely. Not a real fix, just defers it.

**Recommendation:** Option 1. It's the standard idiom, the CLIPS-facing C ABI is untouched by it, and the mechanical change is small — but it does mean auditing every in-repo Rust call site (if any exist outside CLIPS) to add `unsafe {}`, which is why this is being routed for review rather than applied unilaterally.

## Aside worth a quick look

`clara-toolbox/src/ffi.rs:236` (`rust_clara_evaluate`) has a structurally similar pattern — a `pub extern "C" fn` that dereferences a raw pointer via `CStr::from_ptr` inside an `unsafe` block, without being marked `unsafe fn` itself — but clippy did **not** flag it in this run. Worth a manual look when this gets triaged, since it may be a difference in how the null-check is structured rather than a genuine pass on safety.

## Resolution (2026-09-22)

Option 1 applied, team-approved. All 5 flagged `clara-coire` functions are now `pub unsafe extern "C" fn` with a `# Safety` doc comment each (`rust_coire_free_string`'s redundant inner `unsafe {}` block was unwrapped since it's now covered by the outer signature; the other 4 keep their internal `unsafe { cstr_to_str(...) }` since those calls sit inside nested IIFE closures that don't inherit the enclosing function's unsafe context).

The "Aside" above was confirmed correct and extended to match:

- **`clara-ritual/src/clips_bridge.rs`** — same unmarked pattern, not flagged by clippy in isolation but genuinely compiled into the same `--workspace` run (`clara-clips` depends on both crates' `ffi` feature). 6 of 7 functions marked `unsafe fn` the same way. `rust_ritual_topic_list()` was deliberately left as a plain `pub extern "C" fn` — it takes no pointer arguments, so there's no real safety contract to mark unsafe for; doing so anyway "for consistency" would be misleading rather than correct.
- **`clara-toolbox/src/ffi.rs`** — `rust_clara_evaluate` and `rust_free_string` (both already had `# Safety` docs, just needed the signature change) got the same treatment; their now-fully-redundant enclosing `unsafe {}` blocks were removed.

**A third case surfaced mid-fix, structurally different from the rest**: `clara-toolbox/src/ffi.rs`'s `free_c_string` — a plain `pub fn` (not `extern "C"`), explicitly documented as *"a safe wrapper... callable from Rust code"* — has the same unmarked raw-pointer deref, but unlike everything else above it has real Rust call sites (6, across `clara-prolog`/`clara-clips` test code, plus ~14 more inside `ffi.rs`'s own test module). After discussion it was marked `unsafe fn` too, for consistency with its siblings, and every call site updated to wrap in `unsafe {}`.

**Open question, not fully settled**: marking `free_c_string` unsafe pushes the FFI safety burden back onto every Rust caller, which may cut against its original design intent as *the* safe half of the alloc/free pair (`evaluate_json_string` produces the raw pointer; `free_c_string` was meant to let ordinary Rust code free it without needing its own `unsafe` block). Flagged for a possible follow-up: reverting just this one function to a safe `pub fn` with an internal `unsafe {}` wrapping only the `CString::from_raw` call, distinct from the 7+ `extern "C"` functions where `unsafe fn` is unambiguously correct.

Verified clean: `cargo clippy --workspace --all-targets` — zero `not_unsafe_ptr_arg_deref` errors, zero new `unused_unsafe` warnings. `cargo test --workspace` — 0 failed across every crate, including all 5 touched (`clara-coire` 53 passed, `clara-ritual` 71 passed, `clara-toolbox` 55 passed, `clara-prolog` 9+26 passed, `clara-clips` 15+4 passed). `cargo-clippy`'s `allow_failure: true` removed from `.gitlab-ci.yml` — it's blocking again.
