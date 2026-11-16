//! Stage 4b: a request whose thinking budget is `0` tells the template that
//! thinking is off, beside the budget.
//!
//! llama-server enforces `reasoning_budget_tokens: 0` at its sampler and
//! renders the prompt as if thinking were on. A hybrid model such as Qwen3
//! then has its thinking block closed at once and reasons on in plain text,
//! writes its own `</think>` and answers, and all of it is reply text
//! (recorded at b12110: `gglib-runtime`'s `tests/thinking_off_recordings.rs`).
//! Told `enable_thinking: false`, the template opens and closes an empty
//! block itself and the reply is the answer alone. The budget stays: it
//! still ends the thinking of a template that never reads the flag
//! ([ADR 0007]'s 2026-11-16 amendment).
//!
//! The rule is the in-process engine's too: `gglib-gguf`'s `ChatKwargs`
//! renders a request [`budget_turns_thinking_off`] says yes to with
//! `enable_thinking` false, so both engines read one request alike.
//!
//! [ADR 0007]: https://github.com/mmogr/gglib/blob/main/docs/adr/0007-ask-the-server-for-template-capabilities.md

use serde_json::{Map, Value};

use crate::domain::inference::{REASONING_BUDGET_TOKENS_KEY, THINKING_BUDGET_TOKENS_KEY};

/// The request's kwargs for its chat template.
const CHAT_TEMPLATE_KWARGS_KEY: &str = "chat_template_kwargs";

/// The template variable that switches a hybrid model's thinking.
const ENABLE_THINKING_KEY: &str = "enable_thinking";

/// Whether `body` asks for no thinking: its reasoning budget is `0`.
///
/// The budget is `reasoning_budget_tokens`, else `thinking_budget_tokens`,
/// the two names llama-server reads (`tools/server/server-common.cpp:1441-1442`).
/// A field that is null or not a number reads as absent, as llama-server's
/// `json_value` reads it (`tools/server/server-common.h:43-55`). Only an
/// integer is a budget here: a float or a boolean, which llama-server casts
/// to an int, reads as absent too.
///
/// After stage 4 the body's budget is the one the ladder resolved, written
/// over whatever the client sent, and its alias is gone.
#[must_use]
pub fn budget_turns_thinking_off(body: &Value) -> bool {
    let budget = |key| body.get(key).and_then(Value::as_i64);
    budget(REASONING_BUDGET_TOKENS_KEY).or_else(|| budget(THINKING_BUDGET_TOKENS_KEY)) == Some(0)
}

/// Tell the template that thinking is off when `body`'s budget is `0`:
/// `chat_template_kwargs.enable_thinking` becomes `false`. Returns whether it
/// did.
///
/// The client's own kwargs are kept. Its own `enable_thinking: true` beside a
/// budget of `0` gives way: the budget is the field upstream governs and the
/// one gglib's own surfaces send for Thinking off, that pair is exactly the
/// request that leaks, and the in-process engine renders it with thinking off
/// too. A `null` is no kwargs; kwargs that are not an object are the client's
/// mistake and are left as they came, for llama-server to answer. Any other
/// request is not touched: thinking
/// on, or unsaid, is never sent `enable_thinking: true`, so a template's own
/// default stands.
pub fn tell_template_thinking_is_off(body: &mut Value) -> bool {
    if !budget_turns_thinking_off(body) {
        return false;
    }
    let Some(obj) = body.as_object_mut() else {
        return false;
    };
    let kwargs = obj.entry(CHAT_TEMPLATE_KWARGS_KEY).or_insert(Value::Null);
    if kwargs.is_null() {
        *kwargs = Value::Object(Map::new());
    }
    let Some(kwargs) = kwargs.as_object_mut() else {
        return false;
    };
    kwargs.insert(ENABLE_THINKING_KEY.to_owned(), Value::Bool(false));
    true
}

#[cfg(test)]
#[path = "thinking_off_tests.rs"]
mod thinking_off_tests;
