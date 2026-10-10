//! An `OpenAI` chat request's `messages`, read as the agent loop's history.
//!
//! A chat a paired phone keeps itself sends its whole history with each
//! request, in `OpenAI`'s shape. When such a chat runs with gglib's builtins
//! (so that it can draw), the loop needs that history as [`AgentMessage`]s:
//! [`parse_openai_messages`] reads the body's `messages` into
//! [`OpenAiTurn`]s, and [`into_agent_messages`] turns those into the loop's
//! messages once each inline image has been stored and has an id.
//!
//! What is read: `system` and `developer` messages; `user` messages whose
//! content is a string or parts (`text`, and `image_url` holding a base64
//! `data:` URL or a stored image's id); `assistant` messages with text,
//! `tool_calls` or both; and `tool` rows. Anything else is refused by its
//! index, never quoted: a message's text does not belong in an error.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::Value;
use thiserror::Error;

use super::{AgentMessage, AssistantContent, ToolCall};
use crate::domain::AttachmentId;
use crate::request_pipeline::image_urls;

/// One image of a user's message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenAiImage {
    /// Sent inline as a base64 `data:` URL: these are its bytes, to store.
    Inline(Vec<u8>),
    /// Named by the id of an image already stored here.
    Stored(AttachmentId),
}

/// One message of an `OpenAI` history.
#[derive(Debug, Clone)]
pub enum OpenAiTurn {
    /// A `system` or `developer` message.
    System {
        /// Its text.
        content: String,
    },
    /// A `user` message: its text parts joined, and its images in order.
    User {
        /// Its text.
        content: String,
        /// Its images.
        images: Vec<OpenAiImage>,
    },
    /// An `assistant` message: text, tool calls, or both.
    Assistant {
        /// Its text, when it has any.
        text: Option<String>,
        /// The tools it called.
        tool_calls: Vec<ToolCall>,
    },
    /// A `tool` row: what a call answered.
    Tool {
        /// The call it answers.
        tool_call_id: String,
        /// The answer.
        content: String,
    },
}

/// Why a history cannot be read. Names a message by its index, never by
/// its text.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HistoryError {
    /// `messages` is absent, not a list, or empty.
    #[error("`messages` must be a list with at least one message")]
    NoMessages,
    /// A message has a role the loop has no message for.
    #[error("message {index} has a role that is not system, developer, user, assistant or tool")]
    UnknownRole {
        /// Its place in the list, from 0.
        index: usize,
    },
    /// A message lacks what its role needs, or carries it in a shape that
    /// is not `OpenAI`'s.
    #[error("message {index} is not a readable {role} message: {what}")]
    Unreadable {
        /// Its place in the list, from 0.
        index: usize,
        /// Its role.
        role: &'static str,
        /// What is wrong, in fixed words.
        what: &'static str,
    },
    /// An image is neither a base64 `data:` URL nor a stored image's id.
    #[error(
        "message {index} carries an image that is neither a base64 data URL nor a stored \
         image's id; send the image's bytes inline"
    )]
    UnreadableImage {
        /// Its place in the list, from 0.
        index: usize,
    },
    /// Fewer ids were given than the history has inline images.
    #[error("an inline image of the history was not stored")]
    ImageNotStored,
}

/// Read an `OpenAI` request's `messages`.
///
/// # Errors
///
/// [`HistoryError`] for a list that is missing or empty, a role the loop
/// has no message for, a message that lacks what its role needs, or an
/// image that is not inline and not a stored id.
pub fn parse_openai_messages(messages: &Value) -> Result<Vec<OpenAiTurn>, HistoryError> {
    let list = messages
        .as_array()
        .filter(|list| !list.is_empty())
        .ok_or(HistoryError::NoMessages)?;
    list.iter()
        .enumerate()
        .map(|(index, message)| turn(index, message))
        .collect()
}

fn turn(index: usize, message: &Value) -> Result<OpenAiTurn, HistoryError> {
    let role = message.get("role").and_then(Value::as_str);
    let content = message.get("content").unwrap_or(&Value::Null);
    match role {
        Some("system" | "developer") => Ok(OpenAiTurn::System {
            content: text(content).ok_or(HistoryError::Unreadable {
                index,
                role: "system",
                what: "its content is neither text nor text parts",
            })?,
        }),
        Some("user") => Ok(OpenAiTurn::User {
            content: text(content).ok_or(HistoryError::Unreadable {
                index,
                role: "user",
                what: "its content is neither text nor parts",
            })?,
            images: image_urls(content)
                .map(|url| image(url).ok_or(HistoryError::UnreadableImage { index }))
                .collect::<Result<_, _>>()?,
        }),
        Some("assistant") => assistant(index, message, content),
        Some("tool") => Ok(OpenAiTurn::Tool {
            tool_call_id: message
                .get("tool_call_id")
                .and_then(Value::as_str)
                .ok_or(HistoryError::Unreadable {
                    index,
                    role: "tool",
                    what: "it names no tool_call_id",
                })?
                .to_owned(),
            content: text(content).ok_or(HistoryError::Unreadable {
                index,
                role: "tool",
                what: "its content is neither text nor text parts",
            })?,
        }),
        _ => Err(HistoryError::UnknownRole { index }),
    }
}

fn assistant(index: usize, message: &Value, content: &Value) -> Result<OpenAiTurn, HistoryError> {
    let unreadable = |what| HistoryError::Unreadable {
        index,
        role: "assistant",
        what,
    };
    let said = match content {
        Value::Null => None,
        other => Some(text(other).ok_or_else(|| unreadable("its content is not text"))?),
    };
    let calls = match message.get("tool_calls") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(calls)) => calls
            .iter()
            .map(|call| tool_call(call).ok_or_else(|| unreadable("a tool call has no id or name")))
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(unreadable("its tool_calls is not a list")),
    };
    let text = said.filter(|text| !text.is_empty() || calls.is_empty());
    if text.is_none() && calls.is_empty() {
        return Err(unreadable("it has neither content nor tool calls"));
    }
    Ok(OpenAiTurn::Assistant {
        text,
        tool_calls: calls,
    })
}

/// `{"id", "function": {"name", "arguments"}}`, the arguments a JSON string
/// as `OpenAI` writes them, or an object as some clients do. Arguments that
/// do not read are kept as the string they came as.
fn tool_call(call: &Value) -> Option<ToolCall> {
    let function = call.get("function")?;
    let arguments = match function.get("arguments") {
        Some(Value::String(written)) => {
            serde_json::from_str(written).unwrap_or_else(|_| Value::String(written.clone()))
        }
        Some(other) => other.clone(),
        None => Value::Object(serde_json::Map::new()),
    };
    Some(ToolCall {
        id: call.get("id")?.as_str()?.to_owned(),
        name: function.get("name")?.as_str()?.to_owned(),
        arguments,
    })
}

/// The text `content` carries: a string as it is, an array's `text` parts
/// joined by a newline; `None` for any other shape.
fn text(content: &Value) -> Option<String> {
    match content {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => Some(
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        _ => None,
    }
}

/// An image part's URL as the image it is: a base64 `data:` URL's bytes, or
/// a stored image's id.
fn image(url: &str) -> Option<OpenAiImage> {
    if let Some(rest) = url.strip_prefix("data:") {
        let (head, data) = rest.split_once(',')?;
        if !head.ends_with(";base64") {
            return None;
        }
        return BASE64.decode(data.trim()).ok().map(OpenAiImage::Inline);
    }
    AttachmentId::parse(url).ok().map(OpenAiImage::Stored)
}

/// Every inline image of `turns`, in the order [`into_agent_messages`]
/// takes their ids.
pub fn inline_images(turns: &[OpenAiTurn]) -> impl Iterator<Item = &[u8]> {
    turns
        .iter()
        .filter_map(|turn| match turn {
            OpenAiTurn::User { images, .. } => Some(images),
            _ => None,
        })
        .flatten()
        .filter_map(|image| match image {
            OpenAiImage::Inline(bytes) => Some(bytes.as_slice()),
            OpenAiImage::Stored(_) => None,
        })
}

/// `turns` as the agent loop's messages, each inline image named by the
/// next of `ids`: the ids its bytes were stored under, in
/// [`inline_images`]' order.
///
/// # Errors
///
/// [`HistoryError::ImageNotStored`] when `ids` runs out.
pub fn into_agent_messages(
    turns: Vec<OpenAiTurn>,
    ids: impl IntoIterator<Item = AttachmentId>,
) -> Result<Vec<AgentMessage>, HistoryError> {
    let mut ids = ids.into_iter();
    turns
        .into_iter()
        .map(|turn| {
            Ok(match turn {
                OpenAiTurn::System { content } => AgentMessage::System { content },
                OpenAiTurn::User { content, images } => AgentMessage::User {
                    content,
                    images: images
                        .into_iter()
                        .map(|image| match image {
                            OpenAiImage::Stored(id) => Ok(id),
                            OpenAiImage::Inline(_) => {
                                ids.next().ok_or(HistoryError::ImageNotStored)
                            }
                        })
                        .collect::<Result<_, _>>()?,
                },
                OpenAiTurn::Assistant { text, tool_calls } => AgentMessage::Assistant {
                    content: AssistantContent { text, tool_calls },
                },
                OpenAiTurn::Tool {
                    tool_call_id,
                    content,
                } => AgentMessage::Tool {
                    tool_call_id,
                    content,
                },
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "openai_history_tests.rs"]
mod tests;
