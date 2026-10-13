//! The proxy's batching note, read off the body the upstream is handed.
//!
//! A request sent through a whole proxy to a model that can call tools:
//! whether its system prompt reaches the upstream with
//! [`AGENTIC_NOTE`] on it, by what the settings store and by whether the
//! request carries tools.

mod fixtures;

use serde_json::{Value, json};

use fixtures::agentic::{sent_upstream, tool_request};
use gglib_core::Settings;
use gglib_core::request_pipeline::AGENTIC_NOTE;

fn storing(note: Option<bool>) -> Settings {
    Settings {
        agentic_prompt_note: note,
        ..Settings::with_defaults()
    }
}

/// Whether any text of the forwarded body holds the note.
fn holds_the_note(body: &Value) -> bool {
    let escaped = Value::from(AGENTIC_NOTE).to_string();
    body.to_string().contains(escaped.trim_matches('"'))
}

/// Stored on: the client's system prompt reaches the upstream with the note
/// after it, on a turn that follows a tool result and on one that follows
/// the user, and it is the same text on both.
#[tokio::test]
async fn a_stored_true_adds_the_note_to_a_request_that_carries_tools() {
    let mut prompts = Vec::new();
    for the_user_spoke_last in [false, true] {
        let request = tool_request(the_user_spoke_last);
        let (body, _) = sent_upstream(storing(Some(true)), request.clone()).await;

        assert_eq!(
            body["messages"][0],
            json!({ "role": "system", "content": format!("be helpful\n\n{AGENTIC_NOTE}") })
        );
        let sent = request["messages"].as_array().unwrap();
        let received = body["messages"].as_array().unwrap();
        assert_eq!(received[1..], sent[1..], "the rest is the client's");
        assert_eq!(body["tools"], request["tools"]);
        prompts.push(body["messages"][0].clone());
    }
    assert_eq!(prompts[0], prompts[1]);
}

/// The default: with nothing stored, or `false` stored, the conversation
/// reaches the upstream as the client sent it.
#[tokio::test]
async fn an_unset_or_false_setting_adds_no_note() {
    for stored in [None, Some(false)] {
        let request = tool_request(false);
        let (body, _) = sent_upstream(storing(stored), request.clone()).await;

        assert!(!holds_the_note(&body), "{stored:?}: {body}");
        assert_eq!(body["messages"], request["messages"], "{stored:?}");
        assert!(body["tools"].is_array(), "{stored:?}: {body}");
    }
}

/// A request without tools is never given the note, whatever is stored.
#[tokio::test]
async fn a_request_without_tools_is_never_given_the_note() {
    for stored in [None, Some(false), Some(true)] {
        let mut request = tool_request(true);
        request.as_object_mut().unwrap().remove("tools");
        let (body, _) = sent_upstream(storing(stored), request.clone()).await;

        assert!(!holds_the_note(&body), "{stored:?}: {body}");
        assert_eq!(body["messages"], request["messages"], "{stored:?}");
    }
}

/// The agentic-sampling switch stored off turns off the sampling changes
/// and leaves the note, which is not one.
#[tokio::test]
async fn the_agentic_sampling_switch_does_not_turn_the_note_off() {
    let settings = Settings {
        agentic_sampling: Some(false),
        ..storing(Some(true))
    };
    let (body, _) = sent_upstream(settings, tool_request(false)).await;

    assert!(holds_the_note(&body), "{body}");
    assert!(body.get("reasoning_budget_tokens").is_none(), "{body}");
}
