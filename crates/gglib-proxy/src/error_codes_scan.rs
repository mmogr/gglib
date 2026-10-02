//! The reader behind `error_codes_tests`: which codes one source file writes,
//! read as tokens, so a call laid out over several lines reads the same as one
//! on a single line. Items under `#[cfg(test)]` or `#[cfg(all(test, …))]` are
//! not read.
//!
//! A code is written where it is a string literal, or the literal an
//! `unwrap_or` falls back on, in one of these places:
//!
//! - the code argument of `with_code(message, type, code)`,
//!   `coded(status, code, message)`, `error(status, type, code, message)`,
//!   `run_error(code, message)` and `upstream_error_frame(message, type,
//!   code)`;
//! - the `code` field of a `RunError`, `TurnRefused` or `ErrorDetail`, or of
//!   a `…::Coded` or `…::UpstreamError`, being built;
//! - the value of a `"code"` key, as `json!` writes one;
//! - a `fn code(&self)`'s body, as `RunsError::code` answers;
//! - the value of a `let code = …`, and the `code` place of every arm of a
//!   `let (…, code, …) = match …`: the arm's tuple, or the whole arm when it
//!   is not one.
//!
//! Outside a `fn code`, a code that is neither a literal nor falls back on
//! one is a relay, which `error_codes_tests` must name by its file, its `fn`
//! and its spelling, one row a site. When the code is that `fn`'s parameter,
//! naming it is the time to add the `fn` to `CALLS`: the codes its callers
//! pass are read only then. What this cannot see at all is a code that
//! reaches the wire through none of these places, such as a field of another
//! name.

use std::ops::Range;

use super::error_codes_items::{name_fns, strip_tests};
use super::error_codes_tokens::{Tok, Token, args, lex, literal, render};

/// A code a file writes, or the spelling of one it relays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Write {
    /// A code, with the type written beside it when the writer names one.
    Code(String, Option<String>),
    /// The `fn` it sits in, and the code's expression, which is not a literal.
    Relay(String, String),
}

/// Calls whose arguments include a code: (name, arity, code index, type index).
const CALLS: [(&str, usize, usize, Option<usize>); 5] = [
    ("with_code", 3, 2, Some(1)),
    ("coded", 3, 1, None),
    ("error", 4, 2, Some(1)),
    ("run_error", 2, 0, None),
    ("upstream_error_frame", 3, 2, Some(1)),
];

/// Structs whose `code` field is a code on the wire.
const STRUCTS: [&str; 3] = ["RunError", "TurnRefused", "ErrorDetail"];

/// Enum variants whose `code` field is one, read only under a path
/// (`HttpError::Coded`, `Self::Coded`) so the enum's own definition is not.
const VARIANTS: [&str; 2] = ["Coded", "UpstreamError"];

/// Every write in one file's production code, with its line.
pub(super) fn scan(text: &str) -> Vec<(Write, usize)> {
    let mut toks = strip_tests(&lex(text));
    name_fns(&mut toks);
    let mut out = Vec::new();
    for i in 0..toks.len() {
        calls(&toks, i, &mut out);
        structs(&toks, i, &mut out);
        json_key(&toks, i, &mut out);
        code_fn(&toks, i, &mut out);
        lets(&toks, i, &mut out);
    }
    out
}

fn is(toks: &[Token], i: usize, want: &Tok) -> bool {
    toks.get(i).is_some_and(|t| &t.tok == want)
}

fn ident(toks: &[Token], i: usize, name: &str) -> bool {
    matches!(toks.get(i), Some(Token { tok: Tok::Ident(n), .. }) if n == name)
}

fn punct(toks: &[Token], i: usize, c: char) -> bool {
    is(toks, i, &Tok::Punct(c))
}

/// The literal fallbacks of `unwrap_or("…")` and `unwrap_or_else(|| "…"…)`.
fn fallbacks(toks: &[Token]) -> Vec<String> {
    let mut out = Vec::new();
    for i in 0..toks.len() {
        let at = if ident(toks, i, "unwrap_or") && punct(toks, i + 1, '(') {
            i + 2
        } else if ident(toks, i, "unwrap_or_else") && punct(toks, i + 2, '|') {
            i + 4
        } else {
            continue;
        };
        if let Some(Tok::Str(s)) = toks.get(at).map(|t| &t.tok) {
            out.push(s.clone());
        }
    }
    out
}

/// What an expression in a code's place writes: its literal, else the
/// literals it falls back on, else a relay of its spelling.
fn code_at(toks: &[Token], kind: Option<String>, out: &mut Vec<(Write, usize)>) {
    let Some(first) = toks.first() else {
        return;
    };
    if toks.len() == 1 && ident(toks, 0, "None") {
        return; // no code at all
    }
    if let Some(code) = literal(toks) {
        out.push((Write::Code(code, kind), first.line));
        return;
    }
    let falls = fallbacks(toks);
    if falls.is_empty() {
        let relay = Write::Relay(first.func.to_string(), render(toks));
        out.push((relay, first.line));
    }
    for code in falls {
        out.push((Write::Code(code, None), first.line));
    }
}

fn calls(toks: &[Token], i: usize, out: &mut Vec<(Write, usize)>) {
    let Some(&(_, arity, code, kind)) = CALLS.iter().find(|c| ident(toks, i, c.0)) else {
        return;
    };
    let defined_or_method = i > 0 && (ident(toks, i - 1, "fn") || punct(toks, i - 1, '.'));
    if defined_or_method || !punct(toks, i + 1, '(') {
        return;
    }
    let (list, _) = args(toks, i + 1);
    if list.len() == arity {
        let kind = kind.and_then(|k| literal(&toks[list[k].clone()]));
        code_at(&toks[list[code].clone()], kind, out);
    }
}

/// A struct being built (not defined, and not a pattern) with a `code` field.
fn structs(toks: &[Token], i: usize, out: &mut Vec<(Write, usize)>) {
    let Some(Tok::Ident(name)) = toks.get(i).map(|t| &t.tok) else {
        return;
    };
    let pathed = i > 1 && punct(toks, i - 1, ':') && punct(toks, i - 2, ':');
    let named = STRUCTS.contains(&name.as_str()) || (pathed && VARIANTS.contains(&name.as_str()));
    if !named || !punct(toks, i + 1, '{') || (i > 0 && ident(toks, i - 1, "struct")) {
        return;
    }
    let (fields, end) = args(toks, i + 1);
    if punct(toks, end, '=') {
        return; // a pattern: `… } =>` or `let … { … } =`
    }
    for field in fields {
        let field = &toks[field];
        if ident(field, 0, "code") && field.len() == 1 {
            code_at(field, None, out);
        } else if ident(field, 0, "code") && punct(field, 1, ':') {
            code_at(&field[2..], None, out);
        }
    }
}

/// A `"code": …` key, with the literal of a `"type": …` key beside it.
fn json_key(toks: &[Token], i: usize, out: &mut Vec<(Write, usize)>) {
    let key =
        |j: usize, name: &str| is(toks, j, &Tok::Str(name.to_owned())) && punct(toks, j + 1, ':');
    if !key(i, "code") {
        return;
    }
    let mut depth = 0usize;
    let open = (0..i).rev().find(|&j| match toks[j].tok {
        Tok::Punct(')' | ']' | '}') => {
            depth += 1;
            false
        }
        Tok::Punct('(' | '[' | '{') if depth > 0 => {
            depth -= 1;
            false
        }
        Tok::Punct('(' | '[' | '{') => true,
        _ => false,
    });
    let Some(open) = open else {
        return;
    };
    let (entries, _) = args(toks, open);
    let value = |entry: &Range<usize>| &toks[entry.start + 2..entry.end];
    let kind = entries
        .iter()
        .find(|e| key(e.start, "type"))
        .and_then(|e| literal(value(e)));
    if let Some(entry) = entries.iter().find(|e| e.start == i) {
        code_at(value(entry), kind, out);
    }
}

fn code_fn(toks: &[Token], i: usize, out: &mut Vec<(Write, usize)>) {
    if !(ident(toks, i, "fn") && ident(toks, i + 1, "code") && punct(toks, i + 2, '(')) {
        return;
    }
    let Some(open) = (i + 2..toks.len()).find(|&j| punct(toks, j, '{')) else {
        return;
    };
    let (_, end) = args(toks, open);
    for t in &toks[open..end] {
        if let Tok::Str(s) = &t.tok {
            out.push((Write::Code(s.clone(), None), t.line));
        }
    }
}

/// `let code = …;` and `let (…, code, …) = match … { … }`.
fn lets(toks: &[Token], i: usize, out: &mut Vec<(Write, usize)>) {
    if !ident(toks, i, "let") {
        return;
    }
    if ident(toks, i + 1, "code") && punct(toks, i + 2, '=') {
        let end = (i + 3..toks.len())
            .find(|&j| punct(toks, j, ';'))
            .unwrap_or(toks.len());
        code_at(&toks[i + 3..end], None, out);
        return;
    }
    if !punct(toks, i + 1, '(') {
        return;
    }
    let (names, after) = args(toks, i + 1);
    let Some(place) = names
        .iter()
        .position(|n| n.len() == 1 && ident(toks, n.start, "code"))
    else {
        return;
    };
    if !(punct(toks, after, '=') && ident(toks, after + 1, "match")) {
        return;
    }
    let Some(open) = (after..toks.len()).find(|&j| punct(toks, j, '{')) else {
        return;
    };
    // Every arm: a `=>` at the block's own depth.
    let (_, end) = args(toks, open);
    let mut depth = 0usize;
    for j in open..end {
        match &toks[j].tok {
            Tok::Punct('(' | '[' | '{') => depth += 1,
            Tok::Punct(')' | ']' | '}') => depth -= 1,
            Tok::Punct('=') if depth == 1 && punct(toks, j + 1, '>') => {
                arm(toks, j + 2, place, out);
            }
            _ => {}
        }
    }
}

/// The `code` place of the arm whose value starts at `at`: that element of a
/// tuple, else the whole value, a block or up to its `,`.
fn arm(toks: &[Token], at: usize, place: usize, out: &mut Vec<(Write, usize)>) {
    if punct(toks, at, '(')
        && let Some(element) = args(toks, at).0.get(place)
    {
        code_at(&toks[element.clone()], None, out);
        return;
    }
    let mut end = at;
    while let Some(t) = toks.get(end) {
        match t.tok {
            Tok::Punct('{') if end == at => {
                end = args(toks, end).1;
                break;
            }
            Tok::Punct('(' | '[' | '{') => end = args(toks, end).1,
            Tok::Punct(',' | ')' | ']' | '}') => break,
            _ => end += 1,
        }
    }
    code_at(&toks[at..end], None, out);
}
