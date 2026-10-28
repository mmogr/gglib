//! A chat request rendered into the prompt the model reads, through
//! `gglib_gguf::render_chat`, as llama-server renders it at the pin.
//!
//! The request is read as llama-server reads it
//! (`oaicompat_chat_params_parse`, `tools/server/server-common.cpp`): each
//! message's role and text, an array of text parts joined with newlines,
//! `add_generation_prompt` defaulting to true, and the template kwargs,
//! `reasoning_effort` and the reasoning budget through
//! [`ChatKwargs::from_request`], where a budget of 0 renders with thinking
//! off. What the engine cannot render is refused with the message a client
//! is answered: tools, a tool call or tool result in the history, and any
//! content part that is not text.
//!
//! A positive reasoning budget is read and not enforced: thinking is
//! bounded only by `n_predict` and the context. llama-server enforces one in
//! its sampler (`common/reasoning-budget.cpp`), forcing the closing tag into
//! the model's context once the budget is spent; that belongs with a
//! decoder that samples, not with the text it hands out.

use gglib_gguf::{ChatKwargs, ChatMessage, ChatTemplate, RenderError, render_chat};
use serde_json::Value;

/// A chat request, read and ready to render.
#[derive(Debug, Clone)]
pub(crate) struct Chat {
    messages: Vec<ChatMessage>,
    kwargs: ChatKwargs,
    add_generation_prompt: bool,
}

impl Chat {
    /// The chat `body` asks to have rendered, or why it cannot be.
    pub(crate) fn read(body: &Value) -> Result<Self, String> {
        if body
            .get("tools")
            .and_then(Value::as_array)
            .is_some_and(|tools| !tools.is_empty())
        {
            return Err("tools are not supported by gglib's in-process engine".to_owned());
        }
        let messages = body
            .get("messages")
            .ok_or_else(|| "'messages' is required".to_owned())?
            .as_array()
            .ok_or_else(|| "Expected 'messages' to be an array".to_owned())?
            .iter()
            .map(message)
            .collect::<Result<Vec<_>, _>>()?;
        let kwargs = ChatKwargs::from_request(body).map_err(|e| why(&e))?;
        let add_generation_prompt = body
            .get("add_generation_prompt")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        Ok(Self {
            messages,
            kwargs,
            add_generation_prompt,
        })
    }

    /// The prompt `template` renders for this chat.
    pub(crate) fn render(&self, template: &ChatTemplate) -> Result<String, String> {
        self.render_as(template, self.add_generation_prompt)
    }

    /// The text `add_generation_prompt` adds to this chat's render, which a
    /// reply is parsed after, as llama-server finds it
    /// (`common_chat_template_generation_prompt`): the render with it, past
    /// what the render without it shares.
    pub(crate) fn generation_prompt(&self, template: &ChatTemplate) -> Result<String, String> {
        let with = self.render_as(template, true)?;
        let without = self.render_as(template, false)?;
        let shared: usize = with
            .chars()
            .zip(without.chars())
            .take_while(|(a, b)| a == b)
            .map(|(c, _)| c.len_utf8())
            .sum();
        Ok(with[shared..].to_owned())
    }

    fn render_as(
        &self,
        template: &ChatTemplate,
        add_generation_prompt: bool,
    ) -> Result<String, String> {
        render_chat(
            template,
            &self.messages,
            add_generation_prompt,
            &self.kwargs,
        )
        .map_err(|e| why(&e))
    }
}

/// A render error as the client is told it.
fn why(err: &RenderError) -> String {
    match err {
        RenderError::Request(why) => why.clone(),
        RenderError::Template(why) => format!("Failed to format input: {why}"),
    }
}

/// One message of the request, as text.
fn message(msg: &Value) -> Result<ChatMessage, String> {
    let role = msg.get("role").and_then(Value::as_str).unwrap_or_default();
    if role == "tool" || msg.get("tool_calls").is_some_and(|c| !c.is_null()) {
        return Err(
            "tool calls and tool results are not supported by gglib's in-process engine".to_owned(),
        );
    }
    let content = match msg.get("content") {
        None if role == "assistant" => {
            return Err(
                "Assistant message must contain either 'content' or 'tool_calls'!".to_owned(),
            );
        }
        None => return Err("All non-assistant messages must contain 'content'".to_owned()),
        Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => text_of(parts)?,
        Some(_) => return Err("Expected 'content' to be a string or an array".to_owned()),
    };
    let mut message = ChatMessage::new(role, content);
    message.reasoning_content = msg
        .get("reasoning_content")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Ok(message)
}

/// Text parts joined with a newline, as llama-server joins them for a
/// template that reads text (`concat_content_parts`, `common/chat.cpp`).
fn text_of(parts: &[Value]) -> Result<String, String> {
    let mut text = String::new();
    for part in parts {
        if part.get("type").and_then(Value::as_str) != Some("text") {
            return Err("gglib's in-process engine reads text only".to_owned());
        }
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(part.get("text").and_then(Value::as_str).unwrap_or_default());
    }
    Ok(text)
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod render_tests;
