//! What kind of turn a request body represents.
//!
//! The pipeline's shaping stages are otherwise built entirely from facts about
//! the *model* ([`ModelContext`](super::ModelContext)). This module holds the
//! one thing they need to know about the *request*: whether it is asking the
//! model to emit a tool call.
//!
//! It lives here, rather than inside the stage that first needed it, because
//! two stages read the same signal for different purposes —
//! [`sampling`](super::sampling) selects a floor from it, and
//! [`constrain`](super::constrain) decides whether to originate a grammar —
//! and a second copy of "does this request carry tools" would eventually
//! answer differently from the first.

use serde_json::Value;

/// Whether the request carries a non-empty `tools` array.
///
/// # Why presence, and not `tool_choice: "required"`
///
/// Agentic clients overwhelmingly send `tool_choice: "auto"` — Cline, Roo
/// Code and Copilot BYOK all do, and the in-process agent writes `"auto"`
/// itself whenever it has tools to offer. A signal keyed on `"required"`
/// would therefore describe almost no real traffic. Presence of tools is what
/// actually distinguishes an agentic turn from a chat turn.
///
/// # Deliberately more lenient than the grammar path
///
/// [`constrain`](super::constrain) needs the complete list of tool *names* to
/// build a GBNF alternation, so it rejects a `tools` array in which any entry
/// is missing `function.name`. That strictness is right for originating a
/// grammar and wrong here: a request with one malformed tool entry is still a
/// tool-emission turn, and should still be sampled like one.
///
/// A `tools` key that is present but not a non-empty array — `null`, `[]`, an
/// object — reads as no tools. `tool_choice` is not consulted at all: it can
/// appear without `tools` (nothing strips it), and on its own it does not
/// make a turn a tool turn.
#[must_use]
pub fn carries_tools(body: &Value) -> bool {
    body.get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| !tools.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_non_empty_tools_array_is_a_tool_turn() {
        let body = json!({"tools": [{"function": {"name": "read_file"}}]});
        assert!(carries_tools(&body));
    }

    /// The leniency that separates this from `constrain`'s `tool_names`: a
    /// malformed entry still means the model is being asked for a call.
    #[test]
    fn a_tool_entry_without_a_name_is_still_a_tool_turn() {
        let body = json!({"tools": [{"function": {}}]});
        assert!(carries_tools(&body));
    }

    #[test]
    fn tool_choice_alone_is_not_a_tool_turn() {
        // `strip_unsupported_tools` removes `tools` but leaves a `tool_choice`
        // behind when there were no tools to begin with, so this shape reaches
        // the pipeline in practice.
        let body = json!({"tool_choice": "required"});
        assert!(!carries_tools(&body));
    }

    #[test]
    fn an_empty_or_malformed_tools_key_is_not_a_tool_turn() {
        for body in [
            json!({"tools": []}),
            json!({"tools": null}),
            json!({"tools": {"nested": true}}),
            json!({}),
            json!([1, 2, 3]),
        ] {
            assert!(!carries_tools(&body), "{body} should not read as tools");
        }
    }
}
