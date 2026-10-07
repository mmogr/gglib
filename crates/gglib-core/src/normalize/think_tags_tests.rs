//! A stray think tag leaves a reply's text the same way whether the reply
//! is streamed or sent whole: one table of texts, read through both paths.

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use anyhow::Result;
use futures_core::Stream;
use serde_json::{Value, json};

use super::strip_think_tags;
use crate::domain::agent::LlmStreamEvent;
use crate::domain::dialect::DialectSpec;
use crate::normalize::registry::dialect_for_tags;
use crate::normalize::tags::FORMAT_QWEN_XML;
use crate::normalize::{NormalizingStream, get_parser, normalize_chat_completion_body};

/// A reply's text as the model sent it, and as a client is to read it.
const REPLIES: [(&str, &str); 6] = [
    ("</think>\n\nactual answer", "\n\nactual answer"),
    ("<think>spurious</think>real text", "spuriousreal text"),
    ("one </think> two <think> three", "one  two  three"),
    ("</think>", ""),
    ("<think>", ""),
    ("no tag at all", "no tag at all"),
];

/// Events ready at once, in order.
struct Ready(VecDeque<Result<LlmStreamEvent>>);

impl Stream for Ready {
    type Item = Result<LlmStreamEvent>;
    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.0.pop_front())
    }
}

/// The text a client assembles from `raw` sent as one streamed delta.
fn streamed(raw: &str, dialect: Option<&DialectSpec>) -> String {
    let events = [
        LlmStreamEvent::TextDelta {
            content: raw.to_owned(),
        },
        LlmStreamEvent::Done {
            finish_reason: Some("stop".to_owned()),
        },
    ];
    let inner = Box::pin(Ready(events.into_iter().map(Ok).collect()));
    let mut stream = NormalizingStream::new(inner, get_parser(dialect));
    let mut cx = Context::from_waker(Waker::noop());
    let mut text = String::new();
    while let Poll::Ready(Some(event)) = Pin::new(&mut stream).poll_next(&mut cx) {
        if let LlmStreamEvent::TextDelta { content } = event.expect("no upstream error") {
            text.push_str(&content);
        }
    }
    text
}

/// The content a client reads from `raw` sent as a whole reply.
fn whole(raw: &str, dialect: Option<&DialectSpec>) -> Value {
    let mut body = json!({
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": raw },
            "finish_reason": "stop",
        }],
    });
    let errors = normalize_chat_completion_body(&mut body, dialect);
    assert!(errors.is_empty(), "{errors:?}");
    body["choices"][0]["message"]["content"].take()
}

#[test]
fn a_whole_reply_loses_a_stray_think_tag_exactly_as_a_streamed_one_does() {
    let qwen = dialect_for_tags(&[FORMAT_QWEN_XML.to_owned()]);
    for dialect in [None, qwen.as_ref()] {
        for (raw, clean) in REPLIES {
            assert_eq!(strip_think_tags(raw), clean, "the function, on {raw:?}");
            assert_eq!(streamed(raw, dialect), clean, "streamed, on {raw:?}");
            assert_eq!(whole(raw, dialect), clean, "whole, on {raw:?}");
        }
    }
}

/// A whole reply with no tag in it comes back as it was sent: nothing but
/// the tag is this step's to change.
#[test]
fn a_whole_reply_without_a_tag_is_left_as_it_was() {
    let original = json!({
        "id": "chatcmpl-1",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "a < b, and think> is no tag" },
            "finish_reason": "stop",
        }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1 },
    });
    let mut body = original.clone();
    assert!(normalize_chat_completion_body(&mut body, None).is_empty());
    assert_eq!(body, original);
}

/// The tag goes and the rest of the message stays: its other keys, the
/// finish reason, and the body around it.
#[test]
fn only_the_content_of_a_whole_reply_changes_when_a_tag_is_stripped() {
    let mut body = json!({
        "id": "chatcmpl-1",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "</think>\n\nanswer",
                "reasoning_content": "thought",
            },
            "finish_reason": "stop",
        }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1 },
    });
    assert!(normalize_chat_completion_body(&mut body, None).is_empty());
    assert_eq!(
        body,
        json!({
            "id": "chatcmpl-1",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "\n\nanswer",
                    "reasoning_content": "thought",
                },
                "finish_reason": "stop",
            }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1 },
        })
    );
}
