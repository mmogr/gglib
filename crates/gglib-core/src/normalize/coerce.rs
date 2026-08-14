//! Deterministic local repair of a tool-call body the parsers rejected.
//!
//! The cheapest rung of the escalation ladder, and the only one that costs no
//! model call at all. When a model emits a tool call wrapped in a code fence,
//! trailed by prose, or carrying a trailing comma, the payload is *right* and
//! the packaging is wrong. Today that turn is thrown away and the raw bytes
//! are shown to the person as text. Repairing the packaging is measured in
//! microseconds and cannot cost a generation.
//!
//! # Only ever additive
//!
//! [`coerce_json_object`] returns `Some` only when its output parses. Every
//! failure path returns `None` and the caller behaves exactly as it did
//! before. A turn can therefore be rescued by this module but never made
//! worse by it — the same fail-open rule the repair re-issue follows.
//!
//! # The one repair deliberately not attempted
//!
//! **An unterminated string is never closed.** Given
//! `{"name":"read_file","arguments":{"path":"/etc/ho` the structurally
//! obvious fix is to add `"}}`, which yields valid JSON and a tool call that
//! reads the wrong file. Dispatching a plausible-but-wrong call is worse than
//! dispatching none: the client executes it, and a truncated path or query is
//! a side effect nobody asked for. Truncation mid-string means the model's
//! output was cut off, which is a real failure, and this module declines to
//! paper over it.
//!
//! Structural delimiters are different in kind. A missing `}` at the very end
//! of an otherwise complete object cannot change the meaning of any value
//! already present; it can only fail to terminate them.

use serde_json::Value;

/// The most nesting a tool-call body may legitimately carry.
///
/// Bounds the repair against a pathological input — a model emitting
/// thousands of `[` produces a body this refuses rather than a very long
/// string of `]`.
const MAX_NESTING: usize = 64;

/// Try to make `body` parse as a JSON object, without changing what it says.
///
/// Returns the repaired text only when it parses *and* is an object; `None`
/// otherwise, leaving the caller to fail exactly as it would have.
#[must_use]
pub(crate) fn coerce_json_object(body: &str) -> Option<String> {
    let candidate = strip_packaging(body);
    if candidate.is_empty() {
        return None;
    }

    // Cheap path: the packaging was the whole problem.
    if parses_as_object(&candidate) {
        return Some(candidate);
    }

    let without_commas = drop_trailing_commas(&candidate);
    if parses_as_object(&without_commas) {
        return Some(without_commas);
    }

    let closed = close_delimiters(&without_commas)?;
    parses_as_object(&closed).then_some(closed)
}

fn parses_as_object(text: &str) -> bool {
    serde_json::from_str::<Value>(text).is_ok_and(|v| v.is_object())
}

/// Remove a surrounding code fence and any prose either side of the object.
///
/// Models introduce a tool call conversationally ("Sure — I'll read it:") or
/// wrap it in markdown out of habit. Both leave the JSON itself intact.
fn strip_packaging(body: &str) -> String {
    let mut text = body.trim();

    // ```json … ``` or ``` … ```
    if let Some(rest) = text.strip_prefix("```") {
        let rest = rest.strip_prefix("json").unwrap_or(rest);
        text = rest.trim_start_matches(['\r', '\n']).trim();
        if let Some(stripped) = text.strip_suffix("```") {
            text = stripped.trim();
        }
    }

    // Prose either side. Anchored on the outermost braces rather than the
    // first, so a sentence containing `{` does not truncate the object.
    match (text.find('{'), text.rfind('}')) {
        (Some(start), Some(end)) if end > start => text[start..=end].trim().to_owned(),
        // No closing brace at all: keep everything from the opening one, so
        // `close_delimiters` still gets a chance.
        (Some(start), _) => text[start..].trim().to_owned(),
        _ => String::new(),
    }
}

/// Drop commas that sit immediately before a closing delimiter.
///
/// Skips anything inside a string, so a value like `"a, }"` is untouched.
fn drop_trailing_commas(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_comma: Option<usize> = None;
    let mut scan = StringScan::default();

    for ch in text.chars() {
        if scan.step(ch) {
            // Inside a string (or its escape): copy verbatim.
            if let Some(idx) = pending_comma.take() {
                out.insert(idx, ',');
            }
            out.push(ch);
            continue;
        }
        match ch {
            ',' => {
                if let Some(idx) = pending_comma.take() {
                    out.insert(idx, ',');
                }
                pending_comma = Some(out.len());
            }
            '}' | ']' => {
                pending_comma = None;
                out.push(ch);
            }
            c if c.is_whitespace() => out.push(c),
            c => {
                if let Some(idx) = pending_comma.take() {
                    out.insert(idx, ',');
                }
                out.push(c);
            }
        }
    }
    if let Some(idx) = pending_comma {
        out.insert(idx, ',');
    }
    out
}

/// Append whatever closing delimiters the body is missing.
///
/// Returns `None` when the text ends inside a string, when a delimiter is
/// mismatched, or when nesting exceeds [`MAX_NESTING`] — see the module doc
/// on why an unterminated string is left alone.
fn close_delimiters(text: &str) -> Option<String> {
    let mut stack: Vec<char> = Vec::new();
    let mut scan = StringScan::default();

    for ch in text.chars() {
        if scan.step(ch) {
            continue;
        }
        match ch {
            '{' | '[' => {
                if stack.len() >= MAX_NESTING {
                    return None;
                }
                stack.push(ch);
            }
            '}' if stack.pop() != Some('{') => return None,
            ']' if stack.pop() != Some('[') => return None,
            _ => {}
        }
    }

    // Cut off mid-string: refuse, rather than invent the rest of a value.
    if scan.in_string {
        return None;
    }
    if stack.is_empty() {
        return None; // Nothing to close; the failure is something else.
    }

    let mut out = text.trim_end().to_owned();
    while let Some(open) = stack.pop() {
        out.push(if open == '{' { '}' } else { ']' });
    }
    Some(out)
}

/// Tracks whether the scan is inside a JSON string literal.
#[derive(Default)]
struct StringScan {
    in_string: bool,
    escaped: bool,
}

impl StringScan {
    /// Feed one character; returns `true` if it belongs to a string literal
    /// (and so must not be read as structure).
    const fn step(&mut self, ch: char) -> bool {
        if self.in_string {
            if self.escaped {
                self.escaped = false;
            } else if ch == '\\' {
                self.escaped = true;
            } else if ch == '"' {
                self.in_string = false;
                return true; // the closing quote is still string punctuation
            }
            return true;
        }
        if ch == '"' {
            self.in_string = true;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{"name":"read_file","arguments":{"path":"a"}}"#;

    #[test]
    fn a_code_fence_is_unwrapped() {
        let fenced = format!("```json\n{GOOD}\n```");
        assert_eq!(coerce_json_object(&fenced).as_deref(), Some(GOOD));
    }

    #[test]
    fn prose_either_side_is_dropped() {
        let chatty = format!("Sure, I'll read it:\n{GOOD}\nLet me know!");
        assert_eq!(coerce_json_object(&chatty).as_deref(), Some(GOOD));
    }

    #[test]
    fn a_trailing_comma_is_removed() {
        let sloppy = r#"{"name":"read_file","arguments":{"path":"a"},}"#;
        assert!(coerce_json_object(sloppy).is_some());
    }

    #[test]
    fn missing_closing_braces_are_appended() {
        let cut = r#"{"name":"read_file","arguments":{"path":"a"}"#;
        assert_eq!(coerce_json_object(cut).as_deref(), Some(GOOD));
    }

    /// The safety rule this module exists to respect. Closing the string
    /// would yield valid JSON and a call that reads the wrong file.
    #[test]
    fn an_unterminated_string_is_never_completed() {
        let truncated = r#"{"name":"read_file","arguments":{"path":"/etc/ho"#;
        assert_eq!(coerce_json_object(truncated), None);
    }

    #[test]
    fn a_comma_inside_a_string_survives() {
        let body = r#"{"name":"say","arguments":{"text":"a, b, }"}}"#;
        let out = coerce_json_object(body).expect("already valid");
        assert!(out.contains("a, b, }"), "string content altered: {out}");
    }

    #[test]
    fn mismatched_delimiters_are_refused() {
        assert_eq!(coerce_json_object(r#"{"name":"x","args":[}"#), None);
    }

    #[test]
    fn a_non_object_is_refused() {
        assert_eq!(coerce_json_object("[1, 2, 3]"), None);
        assert_eq!(coerce_json_object("\"just a string\""), None);
    }

    #[test]
    fn pathological_nesting_is_refused() {
        let deep = "[".repeat(MAX_NESTING + 5);
        assert_eq!(coerce_json_object(&deep), None);
    }

    #[test]
    fn text_with_no_object_at_all_is_refused() {
        assert_eq!(coerce_json_object("I could not do that."), None);
        assert_eq!(coerce_json_object(""), None);
    }

    /// Valid input must survive untouched — the repair never rewrites a body
    /// that was already fine.
    #[test]
    fn an_already_valid_body_is_returned_unchanged() {
        assert_eq!(coerce_json_object(GOOD).as_deref(), Some(GOOD));
    }
}
