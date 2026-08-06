//! Stage: strip `tools` from requests to models that cannot call them.
//!
//! A client that always advertises its tools (most agentic harnesses do)
//! will send them to whatever model is selected. For a model without
//! [`ModelCapabilities::SUPPORTS_TOOL_CALLS`], forwarding the array is worse
//! than useless: llama-server either rejects the request outright or renders
//! dozens of schemas into the prompt of a model that will only parrot them
//! back as text. The `WebUI`'s chat path has stripped tools this way since it
//! existed; this stage gives every `apply` caller — the proxy above all —
//! the same behaviour.
//!
//! The check is deliberately conservative: it acts only on a
//! [`catalog_resolved`](super::ModelContext::catalog_resolved) context. A
//! passthrough context has an empty capability bitfield because *nobody
//! knows* what the model supports, and stripping tools from an unknown model
//! would silently break a working agent. Unknown models keep their tools.
//!
//! [`ModelCapabilities::SUPPORTS_TOOL_CALLS`]: crate::domain::ModelCapabilities::SUPPORTS_TOOL_CALLS

use serde_json::Value;
use tracing::debug;

use super::ModelContext;

/// Remove `tools` and `tool_choice` when the resolved model cannot use them.
///
/// No-op for tool-capable models, for unresolved (passthrough) contexts, and
/// for requests that carry no tools.
pub fn strip_unsupported_tools(body: &mut Value, ctx: &ModelContext) {
    if !ctx.catalog_resolved || ctx.capabilities.supports_tool_calls() {
        return;
    }
    let Some(obj) = body.as_object_mut() else {
        return;
    };
    if obj.remove("tools").is_some() {
        obj.remove("tool_choice");
        debug!("stripped tools from request: model does not support tool calls");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ModelCapabilities;
    use serde_json::json;

    fn body_with_tools() -> Value {
        json!({
            "model": "m",
            "messages": [{"role": "user", "content": "hi"}],
            "tools": [{"type": "function", "function": {"name": "f"}}],
            "tool_choice": "auto",
        })
    }

    fn resolved(capabilities: ModelCapabilities) -> ModelContext {
        ModelContext {
            capabilities,
            catalog_resolved: true,
            ..ModelContext::passthrough()
        }
    }

    /// The case this stage exists for: a resolved model without the
    /// capability loses the tools array and its `tool_choice`.
    #[test]
    fn a_resolved_non_tool_model_loses_its_tools() {
        let mut body = body_with_tools();
        strip_unsupported_tools(&mut body, &resolved(ModelCapabilities::empty()));

        assert!(body.get("tools").is_none());
        assert!(body.get("tool_choice").is_none());
        assert_eq!(body["model"], "m", "everything else is untouched");
    }

    #[test]
    fn a_tool_capable_model_keeps_its_tools() {
        let mut body = body_with_tools();
        strip_unsupported_tools(&mut body, &resolved(ModelCapabilities::SUPPORTS_TOOL_CALLS));

        assert!(body.get("tools").is_some());
        assert!(body.get("tool_choice").is_some());
    }

    /// An unknown model must not be second-guessed — empty capabilities on a
    /// passthrough context mean "unknown", not "unsupported".
    #[test]
    fn an_unresolved_model_keeps_its_tools() {
        let mut body = body_with_tools();
        strip_unsupported_tools(&mut body, &ModelContext::passthrough());

        assert!(body.get("tools").is_some());
    }

    /// `tool_choice` alone is left in place: it is inert without `tools`,
    /// and inventing removals the `WebUI` path never did would be a behaviour
    /// change smuggled into a refactor.
    #[test]
    fn tool_choice_without_tools_is_left_alone() {
        let mut body = json!({"model": "m", "tool_choice": "auto"});
        strip_unsupported_tools(&mut body, &resolved(ModelCapabilities::empty()));

        assert!(body.get("tool_choice").is_some());
    }
}
