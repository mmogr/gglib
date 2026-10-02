//! The two passes `error_codes_scan` makes over a file's tokens before it
//! reads them: every item under a test gate dropped, and every token named by
//! the `fn` that holds it.

use std::rc::Rc;

use super::error_codes_tokens::{Tok, Token, args, closes, opens, render};

/// Whether `toks[i..]` opens `#[cfg(test)]` or `#[cfg(all(test, …`.
fn test_gate(toks: &[Token], i: usize) -> bool {
    let spelled: String = render(toks.get(i..(i + 9).min(toks.len())).unwrap_or(&[]));
    spelled.starts_with("#[cfg(test)]") || spelled.starts_with("#[cfg(all(test,")
}

/// Keywords that open an item, whose generics may hold a comma.
const ITEMS: [&str; 10] = [
    "fn", "mod", "use", "impl", "struct", "enum", "const", "static", "type", "let",
];

/// The tokens less every item under a test gate: its attributes, then the
/// item to its `;` or the close of its first top-level brace. A gated field,
/// arm or argument, which no item keyword opens, ends at its `,` or at the
/// close of what holds it.
pub(super) fn strip_tests(toks: &[Token]) -> Vec<Token> {
    let mut kept = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        if !test_gate(toks, i) {
            kept.push(toks[i].clone());
            i += 1;
            continue;
        }
        while toks.get(i).is_some_and(|t| t.tok == Tok::Punct('#')) {
            i = args(toks, i + 1).1;
        }
        let mut item = false;
        while let Some(t) = toks.get(i) {
            item |= matches!(&t.tok, Tok::Ident(k) if ITEMS.contains(&k.as_str()));
            if closes(t) {
                break;
            }
            if t.tok == Tok::Punct(';') || (!item && t.tok == Tok::Punct(',')) {
                i += 1;
                break;
            }
            if opens(t) {
                let (_, end) = args(toks, i);
                let braced = t.tok == Tok::Punct('{');
                i = end;
                if braced {
                    break;
                }
            } else {
                i += 1;
            }
        }
    }
    kept
}

/// Name each token by the innermost `fn` whose body holds it. A body is the
/// first `{` after the name that no `(…)` or `[…]` of the signature holds; a
/// `fn` that reaches `;` first has none.
pub(super) fn name_fns(toks: &mut [Token]) {
    for i in 0..toks.len() {
        let name = match (&toks[i].tok, toks.get(i + 1).map(|t| &t.tok)) {
            (Tok::Ident(kw), Some(Tok::Ident(name))) if kw == "fn" => {
                Rc::<str>::from(name.as_str())
            }
            _ => continue,
        };
        let mut at = i + 2;
        while let Some(t) = toks.get(at) {
            match t.tok {
                Tok::Punct('(' | '[') => at = args(toks, at).1,
                Tok::Punct('{' | ';') => break,
                _ => at += 1,
            }
        }
        if toks.get(at).is_some_and(|t| t.tok == Tok::Punct('{')) {
            let end = args(toks, at).1;
            // An inner `fn` comes later, so it overwrites the outer's name.
            for t in &mut toks[at..end] {
                t.func = Rc::clone(&name);
            }
        }
    }
}
