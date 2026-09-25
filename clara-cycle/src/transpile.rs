//! Bidirectional transpiler: Prolog term text ↔ CLIPS ordered-fact text.
//!
//! Used by the relay to translate Coire event payloads as they cross engine
//! boundaries so that each engine always sees its own native syntax.
//!
//! # Mapping
//!
//! | Prolog                  | CLIPS fact              |
//! |-------------------------|-------------------------|
//! | `foo`                   | `(foo)`                 |
//! | `man_with_plan(stan)`   | `(man_with_plan stan)`  |
//! | `parent(tom, bob)`      | `(parent tom bob)`      |
//! | `count(42)`             | `(count 42)`            |
//! | `X` (variable)          | `?X`                    |
//! | `"hello"` (string)      | `"hello"`               |
//!
//! Nested compound terms (e.g. `foo(bar(a), b)`) are serialised as CLIPS
//! strings when they appear as *arguments*; they are not valid as top-level
//! CLIPS ordered-fact names.
//!
//! # Retract
//!
//! Prolog "retract" events → CLIPS `(do-for-all-facts ...)` expressions, which
//! the relay re-emits as "goal" events so `consume_coire_events` evals them.
//!
//! Retract matching is ground-only: if any argument is a Prolog variable the
//! condition collapses to `TRUE` (retract all facts of that functor).

// ── Shared AST ───────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Term {
    Atom(String),
    Variable(String),
    Integer(i64),
    Float(f64),
    Str(String),
    Compound { functor: String, args: Vec<Term> },
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Convert a Prolog term string to a CLIPS ordered-fact string.
///
/// ```text
/// "man_with_plan(stan)"  →  "(man_with_plan stan)"
/// "foo"                  →  "(foo)"
/// "parent(tom, bob)"     →  "(parent tom bob)"
/// ```
pub fn prolog_to_clips_fact(s: &str) -> Result<String, String> {
    let term = PrologParser::new(s.trim()).parse_complete()?;
    Ok(render_clips_fact(&term))
}

/// Convert a CLIPS ordered-fact string to a Prolog term string.
///
/// ```text
/// "(man_with_plan stan)"  →  "man_with_plan(stan)"
/// "(foo)"                 →  "foo"
/// "(parent tom bob)"      →  "parent(tom,bob)"
/// ```
///
/// Returns `Err` if the input does not start with `(`, so callers can skip
/// translation for data that is already in Prolog syntax.
pub fn clips_fact_to_prolog(s: &str) -> Result<String, String> {
    let s = s.trim();
    if !s.starts_with('(') {
        return Err(format!("not a CLIPS fact (doesn't start with '('): {}", s));
    }
    let term = ClipsParser::new(s).parse_fact()?;
    Ok(render_prolog_term(&term))
}

/// Parse a Prolog term string into the shared Term AST.
pub fn parse_prolog_term(s: &str) -> Result<Term, String> {
    PrologParser::new(s.trim()).parse_complete()
}

/// Generate a CLIPS expression that retracts all facts matching the Prolog term.
///
/// ```text
/// "man_with_plan(stan)" →
///   "(do-for-all-facts ((?f man_with_plan)) (eq ?f:implied (create$ stan)) (retract ?f))"
/// "foo" →
///   "(do-for-all-facts ((?f foo)) TRUE (retract ?f))"
/// ```
pub fn prolog_to_clips_retract(s: &str) -> Result<String, String> {
    let term = PrologParser::new(s.trim()).parse_complete()?;
    Ok(render_clips_retract(&term))
}

// ── UTF-8 helpers ─────────────────────────────────────────────────────────────
//
// Both parsers walk the input as bytes (their delimiters are all ASCII), so any multi-byte character has to be reassembled whole:
// pushing `byte as char` turned every UTF-8 byte into its own Latin-1 character (an em dash became three), and byte-class checks like
// `is_ascii_alphanumeric` stopped a name at its first accented letter and let the rest of the input vanish.

/// The character starting at `pos` (which must be a character boundary) and its length in bytes.
fn char_at(input: &[u8], pos: usize) -> Option<(char, usize)> {
    let first = *input.get(pos)?;
    let len = if first < 0x80 {
        1
    } else if first >= 0xF0 {
        4
    } else if first >= 0xE0 {
        3
    } else {
        2
    };
    let end = (pos + len).min(input.len());
    let ch = std::str::from_utf8(&input[pos..end]).ok().and_then(|s| s.chars().next()).unwrap_or('\u{FFFD}');
    Some((ch, end - pos))
}

// ── Prolog parser ─────────────────────────────────────────────────────────────

struct PrologParser {
    input: Vec<u8>,
    pos: usize,
}

impl PrologParser {
    fn new(s: &str) -> Self {
        Self { input: s.as_bytes().to_vec(), pos: 0 }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
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

    /// Finish a character whose first byte `next()` has just returned (reads its continuation bytes).
    fn take_char(&mut self, first: u8) -> char {
        if first < 0x80 {
            return first as char;
        }
        let start = self.pos - 1;
        let (ch, len) = char_at(&self.input, start).unwrap_or(('\u{FFFD}', 1));
        self.pos = start + len;
        ch
    }

    fn peek_char(&self) -> Option<char> {
        char_at(&self.input, self.pos).map(|(c, _)| c)
    }

    fn advance_char(&mut self) {
        if let Some((_, len)) = char_at(&self.input, self.pos) {
            self.pos += len;
        }
    }

    /// Parse one term and require that nothing but an optional closing full stop follows it, so trailing text is an error
    /// instead of being silently dropped.
    fn parse_complete(&mut self) -> Result<Term, String> {
        let term = self.parse_term()?;
        self.skip_ws();
        if self.peek() == Some(b'.') {
            self.next();
            self.skip_ws();
        }
        if self.pos < self.input.len() {
            let rest = String::from_utf8_lossy(&self.input[self.pos..]).into_owned();
            return Err(format!("unexpected text after the Prolog term: {:?}", rest));
        }
        Ok(term)
    }

    fn parse_term(&mut self) -> Result<Term, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'\'') => self.parse_quoted_atom(),
            Some(b'"') => self.parse_double_quoted(),
            Some(b'[') => self.parse_list(),
            Some(b) if b == b'-' || b.is_ascii_digit() => self.parse_number(),
            Some(b) if b.is_ascii_uppercase() || b == b'_' => self.parse_variable(),
            Some(b) if b.is_ascii_lowercase() => self.parse_atom_or_compound(),
            Some(b) if b >= 0x80 => match self.peek_char() {
                // Non-ASCII: a capital starts a variable; any other letter (including caseless ones such as CJK) starts an atom.
                Some(c) if c.is_uppercase() => self.parse_variable(),
                Some(c) if c.is_alphabetic() => self.parse_atom_or_compound(),
                other => Err(format!("unexpected character '{}' in Prolog term", other.unwrap_or('\u{FFFD}'))),
            },
            Some(b) => Err(format!("unexpected character '{}' in Prolog term", b as char)),
            None => Err("unexpected end of Prolog term".into()),
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
                        s.push(self.take_char(c));
                    }
                    None => return Err("unterminated escape in quoted atom".into()),
                },
                Some(c) => s.push(self.take_char(c)),
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
                    Some(c) => s.push(self.take_char(c)),
                    None => return Err("unterminated escape in string".into()),
                },
                Some(c) => s.push(self.take_char(c)),
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
        Err("non-empty list terms are not supported in the transpiler".into())
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
        let s = std::str::from_utf8(&self.input[start..self.pos])
            .map_err(|e| e.to_string())?;
        if is_float {
            s.parse::<f64>().map(Term::Float).map_err(|e| e.to_string())
        } else {
            s.parse::<i64>().map(Term::Integer).map_err(|e| e.to_string())
        }
    }

    fn parse_variable(&mut self) -> Result<Term, String> {
        let start = self.pos;
        while self.peek_char().map_or(false, |c| c.is_alphanumeric() || c == '_') {
            self.advance_char();
        }
        let name = std::str::from_utf8(&self.input[start..self.pos])
            .map_err(|e| e.to_string())?
            .to_string();
        Ok(Term::Variable(name))
    }

    fn parse_atom_or_compound(&mut self) -> Result<Term, String> {
        let start = self.pos;
        while self.peek_char().map_or(false, |c| c.is_alphanumeric() || c == '_') {
            self.advance_char();
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
                    return Err(format!("expected ',' or ')' in args, got '{}'", b as char))
                }
                None => return Err("unexpected end of args".into()),
            }
        }
        Ok(args)
    }
}

// ── CLIPS fact parser ─────────────────────────────────────────────────────────

struct ClipsParser {
    input: Vec<u8>,
    pos: usize,
}

impl ClipsParser {
    fn new(s: &str) -> Self {
        Self { input: s.as_bytes().to_vec(), pos: 0 }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
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

    /// Finish a character whose first byte `next()` has just returned (reads its continuation bytes).
    fn take_char(&mut self, first: u8) -> char {
        if first < 0x80 {
            return first as char;
        }
        let start = self.pos - 1;
        let (ch, len) = char_at(&self.input, start).unwrap_or(('\u{FFFD}', 1));
        self.pos = start + len;
        ch
    }

    fn parse_fact(&mut self) -> Result<Term, String> {
        self.skip_ws();
        if self.peek() != Some(b'(') {
            return Err(format!(
                "expected '(' at start of CLIPS fact, got {:?}",
                self.peek().map(|b| b as char)
            ));
        }
        self.next(); // consume (
        self.skip_ws();
        let functor = self.parse_symbol()?;
        let mut args = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b')') => {
                    self.next();
                    break;
                }
                None => return Err("unexpected end in CLIPS fact".into()),
                _ => args.push(self.parse_field()?),
            }
        }
        if args.is_empty() {
            Ok(Term::Atom(functor))
        } else {
            Ok(Term::Compound { functor, args })
        }
    }

    fn parse_field(&mut self) -> Result<Term, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'"') => self.parse_string(),
            Some(b'?') => self.parse_variable(),
            Some(b) if b.is_ascii_digit() || b == b'-' => self.parse_number(),
            _ => {
                let sym = self.parse_symbol()?;
                Ok(Term::Atom(sym))
            }
        }
    }

    /// Parse a CLIPS symbol: non-whitespace chars, not `(`, `)`, `"`, `;`.
    fn parse_symbol(&mut self) -> Result<String, String> {
        let start = self.pos;
        while self.peek().map_or(false, |b| {
            !b.is_ascii_whitespace() && b != b'(' && b != b')' && b != b'"' && b != b';'
        }) {
            self.next();
        }
        if self.pos == start {
            return Err("empty symbol in CLIPS fact".into());
        }
        std::str::from_utf8(&self.input[start..self.pos])
            .map(|s| s.to_string())
            .map_err(|e| e.to_string())
    }

    fn parse_variable(&mut self) -> Result<Term, String> {
        self.next(); // consume ?
        let start = self.pos;
        while self.peek().map_or(false, |b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b >= 0x80) {
            self.next();
        }
        let raw = std::str::from_utf8(&self.input[start..self.pos])
            .map_err(|e| e.to_string())?;
        // Ensure the Prolog variable name starts with an uppercase letter.
        let name = capitalize_first(raw);
        Ok(Term::Variable(name))
    }

    fn parse_string(&mut self) -> Result<Term, String> {
        self.next(); // consume "
        let mut s = String::new();
        loop {
            match self.next() {
                None => return Err("unterminated string in CLIPS fact".into()),
                Some(b'"') => break,
                Some(b'\\') => match self.next() {
                    Some(b'n') => s.push('\n'),
                    Some(b't') => s.push('\t'),
                    Some(b'"') => s.push('"'),
                    Some(b'\\') => s.push('\\'),
                    Some(c) => s.push(self.take_char(c)),
                    None => return Err("unterminated escape in CLIPS string".into()),
                },
                Some(c) => s.push(self.take_char(c)),
            }
        }
        Ok(Term::Str(s))
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
        let s = std::str::from_utf8(&self.input[start..self.pos])
            .map_err(|e| e.to_string())?;
        if is_float {
            s.parse::<f64>().map(Term::Float).map_err(|e| e.to_string())
        } else {
            s.parse::<i64>().map(Term::Integer).map_err(|e| e.to_string())
        }
    }
}

// ── Renderers ─────────────────────────────────────────────────────────────────

/// Render a term as a CLIPS ordered-fact string, e.g. `(man_with_plan stan)`.
pub(crate) fn render_clips_fact(term: &Term) -> String {
    match term {
        Term::Atom(s) => format!("({})", clips_symbol_for_functor(s)),
        Term::Compound { functor, args } => {
            let fields: Vec<_> = args.iter().map(render_clips_field).collect();
            format!("({} {})", clips_symbol_for_functor(functor), fields.join(" "))
        }
        Term::Integer(n) => n.to_string(),
        Term::Float(f) => format!("{f}"),
        Term::Str(s) => format!("\"{}\"", escape_clips_string(s)),
        Term::Variable(s) => format!("?{s}"),
    }
}

/// Render a single CLIPS fact field value (argument position).
pub(crate) fn render_clips_field(term: &Term) -> String {
    match term {
        Term::Atom(s) => {
            if is_simple_clips_symbol(s) {
                s.clone()
            } else {
                format!("\"{}\"", escape_clips_string(s))
            }
        }
        // Prolog anonymous variables (_  or _Name) → CLIPS single-field wildcard ?
        Term::Variable(s) if s.starts_with('_') => "?".to_string(),
        Term::Variable(s) => format!("?{s}"),
        Term::Integer(n) => n.to_string(),
        Term::Float(f) => format!("{f}"),
        Term::Str(s) => format!("\"{}\"", escape_clips_string(s)),
        Term::Compound { functor, args } => {
            // Nested compound: serialise as a CLIPS string — CLIPS ordered facts
            // don't support nested structure natively.
            let inner: Vec<_> = args.iter().map(render_clips_field).collect();
            let repr = format!("{}({})", functor, inner.join(","));
            format!("\"{}\"", escape_clips_string(&repr))
        }
    }
}

/// Generate a CLIPS `do-for-all-facts` expression that retracts matching facts.
fn render_clips_retract(term: &Term) -> String {
    let (functor, args): (&str, &[Term]) = match term {
        Term::Atom(s) => (s.as_str(), &[]),
        Term::Compound { functor, args } => (functor.as_str(), args.as_slice()),
        _ => return "(printout t \"retract: unsupported top-level term\" crlf)".into(),
    };

    let sym = clips_symbol_for_functor(functor);

    let all_ground = !args.is_empty() && args.iter().all(term_is_ground);

    if all_ground {
        let fields: Vec<_> = args.iter().map(render_clips_field).collect();
        format!(
            "(do-for-all-facts ((?f {sym})) (eq ?f:implied (create$ {})) (retract ?f))",
            fields.join(" ")
        )
    } else {
        format!("(do-for-all-facts ((?f {sym})) TRUE (retract ?f))")
    }
}

/// Render a term as a Prolog term string, e.g. `man_with_plan(stan)`.
pub(crate) fn render_prolog_term(term: &Term) -> String {
    match term {
        Term::Atom(s) => prolog_atom_render(s),
        Term::Variable(s) => ensure_prolog_var(s),
        Term::Integer(n) => n.to_string(),
        Term::Float(f) => format!("{f}"),
        Term::Str(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        Term::Compound { functor, args } => {
            let args_str: Vec<_> = args.iter().map(render_prolog_term).collect();
            format!("{}({})", prolog_atom_render(functor), args_str.join(","))
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn clips_symbol_for_functor(s: &str) -> String {
    // Replace spaces with underscores for functor/predicate names.
    s.replace(' ', "_")
}

fn is_simple_clips_symbol(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
}

fn escape_clips_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn term_is_ground(t: &Term) -> bool {
    match t {
        Term::Variable(_) => false,
        Term::Compound { args, .. } => args.iter().all(term_is_ground),
        _ => true,
    }
}

fn prolog_atom_needs_quoting(s: &str) -> bool {
    s.is_empty()
        || s.starts_with(|c: char| c.is_ascii_uppercase() || c.is_ascii_digit())
        || s.chars().any(|c| !c.is_alphanumeric() && c != '_')
}

fn prolog_atom_render(s: &str) -> String {
    if prolog_atom_needs_quoting(s) {
        format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
    } else {
        s.to_string()
    }
}

fn ensure_prolog_var(s: &str) -> String {
    // Prolog variables must start with uppercase or _
    match s.chars().next() {
        Some(c) if c.is_ascii_uppercase() || c == '_' => s.to_string(),
        Some(c) => format!("{}{}", c.to_ascii_uppercase(), &s[c.len_utf8()..]),
        None => "_".to_string(),
    }
}

fn capitalize_first(s: &str) -> String {
    match s.chars().next() {
        Some(c) if c.is_ascii_lowercase() => {
            format!("{}{}", c.to_ascii_uppercase(), &s[c.len_utf8()..])
        }
        _ => s.to_string(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Prolog → CLIPS fact

    #[test]
    fn prolog_atom_to_clips_fact() {
        assert_eq!(prolog_to_clips_fact("foo").unwrap(), "(foo)");
    }

    #[test]
    fn prolog_unary_to_clips_fact() {
        assert_eq!(
            prolog_to_clips_fact("man_with_plan(stan)").unwrap(),
            "(man_with_plan stan)"
        );
    }

    #[test]
    fn prolog_binary_to_clips_fact() {
        assert_eq!(
            prolog_to_clips_fact("parent(tom, bob)").unwrap(),
            "(parent tom bob)"
        );
    }

    #[test]
    fn prolog_number_arg_to_clips_fact() {
        assert_eq!(prolog_to_clips_fact("count(42)").unwrap(), "(count 42)");
    }

    #[test]
    fn prolog_string_arg_to_clips_fact() {
        assert_eq!(
            prolog_to_clips_fact(r#"greet("hello")"#).unwrap(),
            r#"(greet "hello")"#
        );
    }

    #[test]
    fn prolog_variable_arg_to_clips_fact() {
        assert_eq!(
            prolog_to_clips_fact("parent(tom, X)").unwrap(),
            "(parent tom ?X)"
        );
    }

    // CLIPS fact → Prolog

    #[test]
    fn clips_nullary_to_prolog() {
        assert_eq!(clips_fact_to_prolog("(foo)").unwrap(), "foo");
    }

    #[test]
    fn clips_unary_to_prolog() {
        assert_eq!(
            clips_fact_to_prolog("(man_with_plan stan)").unwrap(),
            "man_with_plan(stan)"
        );
    }

    #[test]
    fn clips_binary_to_prolog() {
        assert_eq!(
            clips_fact_to_prolog("(parent tom bob)").unwrap(),
            "parent(tom,bob)"
        );
    }

    #[test]
    fn clips_number_field_to_prolog() {
        assert_eq!(clips_fact_to_prolog("(count 42)").unwrap(), "count(42)");
    }

    #[test]
    fn clips_variable_field_to_prolog() {
        assert_eq!(
            clips_fact_to_prolog("(parent tom ?X)").unwrap(),
            "parent(tom,X)"
        );
    }

    #[test]
    fn clips_non_fact_skipped() {
        assert!(clips_fact_to_prolog("user_authenticated(alice)").is_err());
    }

    // Prolog → CLIPS retract

    #[test]
    fn retract_nullary() {
        assert_eq!(
            prolog_to_clips_retract("foo").unwrap(),
            "(do-for-all-facts ((?f foo)) TRUE (retract ?f))"
        );
    }

    #[test]
    fn retract_ground_unary() {
        assert_eq!(
            prolog_to_clips_retract("man_with_plan(stan)").unwrap(),
            "(do-for-all-facts ((?f man_with_plan)) (eq ?f:implied (create$ stan)) (retract ?f))"
        );
    }

    #[test]
    fn retract_ground_binary() {
        assert_eq!(
            prolog_to_clips_retract("parent(tom, bob)").unwrap(),
            "(do-for-all-facts ((?f parent)) (eq ?f:implied (create$ tom bob)) (retract ?f))"
        );
    }

    #[test]
    fn retract_with_variable_falls_back_to_true() {
        assert_eq!(
            prolog_to_clips_retract("parent(tom, X)").unwrap(),
            "(do-for-all-facts ((?f parent)) TRUE (retract ?f))"
        );
    }

    // Round-trip

    #[test]
    fn round_trip_prolog_to_clips_to_prolog() {
        let original = "parent(tom,bob)";
        let clips = prolog_to_clips_fact(original).unwrap();
        let back = clips_fact_to_prolog(&clips).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn round_trip_clips_to_prolog_to_clips() {
        let original = "(parent tom bob)";
        let prolog = clips_fact_to_prolog(original).unwrap();
        let back = prolog_to_clips_fact(&prolog).unwrap();
        assert_eq!(back, original);
    }
}
