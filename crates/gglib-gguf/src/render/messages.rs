//! The `messages` a template is given, shaped as llama-server shapes a text
//! message (`common_chat_msg::to_json_oaicompat`, `common/chat.cpp:156-233`).

use minijinja::Value;

use super::ChatMessage;

/// The marker of the templates whose message text llama-server trims before
/// rendering (`common/chat.cpp:1296-1302`).
const TRIMMED_CONTENT_MARKER: &str =
    "You have access to the following functions in JSONSchema format";

/// `messages` for the template whose source is `source`.
///
/// Each is `role` and `content`, with `reasoning_content` only when it is
/// not empty, in that order. `developer` becomes `system` for every
/// template but those with `<|channel|>` turns (`common/chat.cpp:1334-1337`).
pub(super) fn to_value(source: &str, messages: &[ChatMessage]) -> Value {
    let trim = source.contains(TRIMMED_CONTENT_MARKER);
    let keep_developer = source.contains("<|channel|>");
    Value::from(
        messages
            .iter()
            .map(|message| one(message, trim, keep_developer))
            .collect::<Vec<_>>(),
    )
}

fn one(message: &ChatMessage, trim: bool, keep_developer: bool) -> Value {
    let text = |s: &str| {
        if trim {
            trim_whitespace(s).to_owned()
        } else {
            s.to_owned()
        }
    };
    let role = if message.role == "developer" && !keep_developer {
        "system"
    } else {
        message.role.as_str()
    };
    let mut shaped = vec![
        ("role", Value::from(role)),
        ("content", Value::from(text(&message.content))),
    ];
    if let Some(reasoning) = message.reasoning_content.as_deref().map(text)
        && !reasoning.is_empty()
    {
        shaped.push(("reasoning_content", Value::from(reasoning)));
    }
    shaped.into_iter().collect()
}

/// `s` without the bytes C's `isspace` names at either end, as llama's
/// `trim_whitespace` (`common/chat-auto-parser-helpers.cpp:12-28`).
fn trim_whitespace(s: &str) -> &str {
    s.trim_matches([' ', '\t', '\n', '\u{b}', '\u{c}', '\r'])
}
