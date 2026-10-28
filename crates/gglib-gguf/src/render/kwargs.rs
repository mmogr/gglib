//! What a chat request asks of its template beyond the messages, read the
//! way llama-server reads it (`oaicompat_chat_params_parse`,
//! `tools/server/server-common.cpp:1395-1428`), and the one mapping that is
//! gglib's own: a reasoning budget of zero renders with thinking off.

use std::collections::BTreeMap;

use minijinja::Value as JinjaValue;
use serde_json::{Map, Value};

use super::RenderError;

/// A chat request's template kwargs, and the `enable_thinking` they and the
/// request's reasoning fields come to.
///
/// [`ChatKwargs::default`] is what llama-server holds for a request that
/// asks nothing, as gglib launches it (neither `--reasoning` nor
/// `--[no-]reasoning-preserve`): the kwarg `preserve_reasoning: true`
/// (`common/arg.cpp:964-966`) and thinking on.
///
/// With no `enable_thinking` in the request llama-server takes whether its
/// own analysis of the template finds a reasoning block
/// (`tools/server/server-context.cpp:1612-1613`), an analysis gglib does
/// not reproduce, so `true` stands in. Of the templates llama.cpp bundles
/// at the pin that read the flag and that minijinja compiles, that gives
/// llama-server's prompt for all but Apertus-8B-Instruct, whose flag
/// changes its system turn rather than a reasoning block. A request that
/// says `enable_thinking` renders as llama-server renders it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatKwargs {
    enable_thinking: bool,
    kwargs: Map<String, Value>,
}

impl Default for ChatKwargs {
    fn default() -> Self {
        let mut kwargs = Map::new();
        kwargs.insert("preserve_reasoning".to_owned(), Value::Bool(true));
        Self {
            enable_thinking: true,
            kwargs,
        }
    }
}

impl ChatKwargs {
    /// What the chat completion `body` asks of its template.
    ///
    /// `chat_template_kwargs` are laid over the defaults, and a boolean
    /// `enable_thinking` among them sets the flag; a string one is refused,
    /// as llama-server refuses it. `reasoning_effort: "none"` turns thinking
    /// off, and another effort becomes the `reasoning_effort` kwarg.
    ///
    /// Then gglib's own rule, which llama-server does not have: a
    /// `reasoning_budget_tokens` (or `thinking_budget_tokens`) of 0 renders
    /// with thinking off, over anything else the request says. llama-server
    /// keeps the thinking prompt and ends the thinking at its sampler
    /// (`tools/server/server-common.cpp:1439-1454`); gglib's in-process
    /// engine has no such sampler, so the budget is honoured here.
    pub fn from_request(body: &Value) -> Result<Self, RenderError> {
        let mut read = Self::default();
        if let Some(asked) = body.get("chat_template_kwargs").and_then(Value::as_object) {
            for (key, value) in asked {
                read.kwargs.insert(key.clone(), value.clone());
            }
        }
        match read.kwargs.get("enable_thinking") {
            Some(Value::Bool(on)) => read.enable_thinking = *on,
            Some(Value::String(_)) => {
                return Err(RenderError::Request(
                    "invalid type for \"enable_thinking\" (expected boolean, got string)"
                        .to_owned(),
                ));
            }
            _ => {}
        }
        match body.get("reasoning_effort").and_then(Value::as_str) {
            Some("none") => {
                read.enable_thinking = false;
                read.kwargs.remove("reasoning_effort");
            }
            Some(effort) if !effort.is_empty() => {
                read.kwargs
                    .insert("reasoning_effort".to_owned(), Value::from(effort));
            }
            _ => {}
        }
        if reasoning_budget(body) == Some(0) {
            read.enable_thinking = false;
            read.kwargs
                .insert("enable_thinking".to_owned(), Value::Bool(false));
        }
        Ok(read)
    }

    /// Whether the template is asked to think, before the kwargs are laid
    /// over it.
    pub fn enable_thinking(&self) -> bool {
        self.enable_thinking
    }

    /// The kwargs, each given to the template as a variable of its name.
    pub fn kwargs(&self) -> &Map<String, Value> {
        &self.kwargs
    }
}

/// The request's reasoning budget: `reasoning_budget_tokens`, else
/// `thinking_budget_tokens` (`tools/server/server-common.cpp:1441-1442`).
///
/// A field that is null or not a number reads as absent, as llama-server's
/// `json_value` reads it (`tools/server/server-common.h:43-55`). Only an
/// integer is a budget here: a float or a boolean, which llama-server casts
/// to an int, reads as absent too.
fn reasoning_budget(body: &Value) -> Option<i64> {
    let budget = |key| body.get(key).and_then(Value::as_i64);
    budget("reasoning_budget_tokens").or_else(|| budget("thinking_budget_tokens"))
}

/// The variables a `preserve_reasoning` boolean and a `reasoning_effort`
/// string set before the kwargs are laid over them
/// (`common/jinja/caps.cpp:22-33`, `common/chat.cpp:912-919`).
pub(super) fn reasoning_variables(kwargs: &ChatKwargs) -> BTreeMap<String, JinjaValue> {
    let mut set = BTreeMap::new();
    if let Some(on) = kwargs
        .kwargs
        .get("preserve_reasoning")
        .and_then(Value::as_bool)
    {
        set.insert("preserve_thinking".to_owned(), JinjaValue::from(on));
        for off in [
            "clear_thinking",
            "truncate_history_thinking",
            "drop_thinking",
        ] {
            set.insert(off.to_owned(), JinjaValue::from(!on));
        }
    }
    if let Some(effort) = kwargs
        .kwargs
        .get("reasoning_effort")
        .and_then(Value::as_str)
        && !effort.is_empty()
    {
        for name in ["reasoning_effort", "reasoning_strength"] {
            set.insert(name.to_owned(), JinjaValue::from(effort));
        }
    }
    set
}
