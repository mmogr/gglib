//! What kind of turn a request body represents.
//!
//! The pipeline's shaping stages are otherwise built entirely from facts about
//! the *model* ([`ModelContext`](super::ModelContext)). This module holds the
//! one thing they need to know about the *request*: whether it is asking the
//! model to emit a tool call.
//!
//! It lives here, rather than inside the stage that first needed it, because
//! two stages read the same signal for different purposes —
//! [`sampling`](super::sampling) caps the temperature from it, and
//! [`constrain`](super::constrain) decides whether to originate a grammar —
//! and a second copy of "does this request carry tools" would eventually
//! answer differently from the first.

use serde_json::Value;

/// Whether the request carries a non-empty `tools` array.
///
/// # This identifies an agentic turn, not a tool-emission turn
///
/// The distinction matters and was learned the hard way. This answers *"could
/// this turn produce a tool call?"*, never *"will it?"*. VS Code Copilot in
/// agent mode sends the `tools` array on essentially every request —
/// including prose, planning, summarising and thinking turns — so in the
/// client this was built for, the answer is permanently yes.
///
/// Anything keyed on this therefore applies to a whole agentic session, not to
/// the moment a call is emitted. Adjustments hanging off it must be safe for
/// prose, because they will spend most of their time there.
///
/// # Why there is no better signal
///
/// Every candidate fails. `tool_choice` is `"auto"` on every turn. The last
/// message's role does not predict what comes next. A history containing prior
/// `tool_calls` only says "this is an agentic session", which is the thing
/// already known. This is the same wall [`super::constrain`] documents for
/// grammars: you cannot know before decoding whether the model will emit a
/// call. The only true discriminator is mid-stream marker detection, which is
/// a different and much larger piece of machinery.
///
/// # Deliberately more lenient than the grammar path
///
/// [`constrain`](super::constrain) needs the complete list of tool *names* to
/// build a GBNF alternation, so it rejects a `tools` array in which any entry
/// is missing `function.name`. That strictness is right for originating a
/// grammar and wrong here: a request with one malformed tool entry is still an
/// agentic turn.
///
/// A `tools` key that is present but not a non-empty array — `null`, `[]`, an
/// object — reads as no tools. `tool_choice` is not consulted at all: it can
/// appear without `tools` (nothing strips it), and on its own it does not make
/// a turn agentic.
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
    fn a_non_empty_tools_array_is_an_agentic_turn() {
        let body = json!({"tools": [{"function": {"name": "read_file"}}]});
        assert!(carries_tools(&body));
    }

    /// The leniency that separates this from `constrain`'s `tool_names`: a
    /// malformed entry still means tools are in scope for this turn.
    #[test]
    fn a_tool_entry_without_a_name_is_still_an_agentic_turn() {
        let body = json!({"tools": [{"function": {}}]});
        assert!(carries_tools(&body));
    }

    #[test]
    fn tool_choice_alone_is_not_an_agentic_turn() {
        // `strip_unsupported_tools` removes `tools` but leaves a `tool_choice`
        // behind when there were no tools to begin with, so this shape reaches
        // the pipeline in practice.
        let body = json!({"tool_choice": "required"});
        assert!(!carries_tools(&body));
    }

    #[test]
    fn an_empty_or_malformed_tools_key_is_not_an_agentic_turn() {
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
