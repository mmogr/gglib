//! The proxy's thinking policy, read off the body the upstream is handed.
//!
//! A request that carries tools, sent through a whole proxy to a model that
//! can call them: what `reasoning_budget_tokens` the upstream receives, by who
//! spoke last and by what the settings store.

mod fixtures;

use serde_json::{Value, json};

use fixtures::agentic::{sent_upstream, tool_request};
use fixtures::loop_guard::{assistant_call, looping_history};
use gglib_core::{AgenticThinking, Settings};

/// [`sent_upstream`], for a turn that still carries its tools when it gets
/// there: without them no policy applies, and an absent budget would prove
/// nothing.
async fn forwarded(settings: Settings, request: Value) -> (Value, Value) {
    let (body, status) = sent_upstream(settings, request).await;
    assert!(
        body["tools"]
            .as_array()
            .is_some_and(|tools| !tools.is_empty()),
        "{body}"
    );
    (body, status)
}

fn storing(policy: Option<AgenticThinking>) -> Settings {
    Settings {
        agentic_thinking: policy,
        ..Settings::with_defaults()
    }
}

/// The default, with nothing stored: a turn that follows a tool result is
/// sent a budget of 0, and the same conversation with the user's message
/// after it is sent none.
#[tokio::test]
async fn an_unset_install_stops_thinking_after_a_tool_result_and_not_after_the_user() {
    for stored in [None, Some(AgenticThinking::WhenAsked)] {
        let (after_tool, status) = forwarded(storing(stored), tool_request(false)).await;
        assert_eq!(
            after_tool["reasoning_budget_tokens"],
            json!(0),
            "{stored:?}"
        );
        assert!(after_tool.get("reasoning_budget_message").is_none());
        // The proxy's own record of the turn names the policy, not a rung.
        let recorded = &status["sampling_audit"]["reasoning"]["latest"]["budget"];
        assert_eq!(recorded["tokens"], json!(0), "{status}");
        assert_eq!(recorded["source"], json!("agentic turn"), "{status}");

        let (after_user, status) = forwarded(storing(stored), tool_request(true)).await;
        assert!(
            after_user.get("reasoning_budget_tokens").is_none(),
            "{stored:?}: {after_user}"
        );
        let latest = &status["sampling_audit"]["reasoning"]["latest"];
        assert!(latest.is_object(), "the turn was recorded: {status}");
        assert!(latest["budget"].is_null(), "{status}");
    }
}

#[tokio::test]
async fn each_stored_policy_reaches_the_upstream_as_its_own_budget() {
    let (off, _) = forwarded(storing(Some(AgenticThinking::Off)), tool_request(true)).await;
    assert_eq!(off["reasoning_budget_tokens"], json!(0));
    assert!(off.get("reasoning_budget_message").is_none());

    let (brief, _) = forwarded(storing(Some(AgenticThinking::Brief)), tool_request(false)).await;
    assert_eq!(brief["reasoning_budget_tokens"], json!(512));
    assert_eq!(
        brief["reasoning_budget_message"],
        json!("Enough thinking. Act now.")
    );

    let (unchanged, _) =
        forwarded(storing(Some(AgenticThinking::Default)), tool_request(false)).await;
    assert!(
        unchanged.get("reasoning_budget_tokens").is_none(),
        "{unchanged}"
    );
}

/// The way out for a client: its own budget, `-1` for the model's default.
#[tokio::test]
async fn a_clients_own_budget_reaches_the_upstream_as_sent() {
    for sent in [-1, 0, 2048] {
        let mut request = tool_request(false);
        request["reasoning_budget_tokens"] = json!(sent);
        let (body, _) = forwarded(storing(None), request).await;

        assert_eq!(body["reasoning_budget_tokens"], json!(sent));
    }
}

/// `agentic_sampling` stored off switches the policy off with the ceiling.
#[tokio::test]
async fn the_agentic_sampling_switch_turns_the_policy_off() {
    let settings = Settings {
        agentic_sampling: Some(false),
        agentic_thinking: Some(AgenticThinking::Off),
        ..Settings::with_defaults()
    };
    let (body, _) = forwarded(settings, tool_request(false)).await;

    assert!(body.get("reasoning_budget_tokens").is_none(), "{body}");
}

/// The loop guard's note goes on as a new `user` message when the request's
/// last message has no text to add it to. The person did not write it, so
/// `when-asked` reads who spoke last in what the client sent, and the turn is
/// still sent a budget of 0.
#[tokio::test]
async fn the_loop_guards_own_user_message_is_not_the_person_speaking() {
    let mut request = tool_request(false);
    let mut messages = vec![json!({ "role": "user", "content": "fix it" })];
    messages.extend(looping_history(3));
    // The fourth call of the batch, with no text and no result yet.
    messages.push(assistant_call("write_file", r#"{"path":"src/main.rs"}"#));
    request["messages"] = json!(messages);

    let (body, status) = forwarded(storing(None), request).await;

    let last = body["messages"].as_array().unwrap().last().unwrap();
    assert_eq!(last["role"], json!("user"), "{body}");
    let note = last["content"].as_str().expect("the note is text");
    assert!(note.contains("[gglib loop guard]"), "{note}");
    assert_eq!(
        status["recent_requests"][0]["loop_guard_trip"],
        json!("loop")
    );
    assert_eq!(body["reasoning_budget_tokens"], json!(0), "{body}");
}
