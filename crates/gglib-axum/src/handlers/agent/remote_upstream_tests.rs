//! The paired machine's model as a turn drives it: looked up there, sent by
//! its id, and counted and made under that machine's name for it.

use super::*;
use crate::handlers::remote::fake_far::{FINGERPRINT, carries_key, far as fake_far, only};

/// A model of the paired machine, by its id there.
fn far_ref(id: i64) -> ModelRef {
    ModelRef {
        machine: Machine::Paired {
            fingerprint: FINGERPRINT.to_owned(),
        },
        id,
    }
}

/// The far detail route's answer for model 3, as a gglib proxy writes it.
const LOOKUP: &str = r#"{"detail":{"id":3,"name":"org/qwen3","paramCountB":8.0,
    "quantization":"Q4_K_M","addedAt":"2026-10-01 09:00:00","isServing":false,"metadata":{}}}"#;

/// A far chat's saved profile goes on the id the turn sends, which that
/// machine routes to a profile of its own; the lookup asks for the id alone.
#[tokio::test]
async fn a_far_chats_profile_rides_on_the_id_it_sends() {
    let (fake, far) = fake_far(200, LOOKUP).await;
    let saved = ChatSampling {
        profile: Some("coding".to_owned()),
        ..ChatSampling::default()
    };

    let upstream = remote(&far, &far_ref(3), &saved).await.expect("looked up");

    assert_eq!(upstream.model.as_deref(), Some("3:coding"));
    assert_eq!(only(&fake).uri, "/v1/gglib/models/3/detail");
}

/// A turn reached through [`far`] carries what its chat saved: its profile
/// rides on the id it sends.
#[tokio::test]
async fn a_far_turn_carries_its_chats_saved_profile() {
    let (_fake, proxy) = fake_far(200, LOOKUP).await;
    let saved = ChatSampling {
        profile: Some("coding".to_owned()),
        ..ChatSampling::default()
    };
    let reach = async |machine: &Machine| {
        assert_eq!(
            *machine,
            far_ref(3).machine,
            "the chat's machine is reached"
        );
        Ok(proxy)
    };

    let upstream = far(reach, &far_ref(3), &saved).await.expect("reached");

    assert_eq!(upstream.model.as_deref(), Some("3:coding"));
}

/// The far model is looked up by its id, sent by its id, and counted and
/// made under the name and quantisation that machine has for it.
#[tokio::test]
async fn a_far_run_sends_the_id_and_is_made_by_the_far_name() {
    let (fake, far) = fake_far(200, LOOKUP).await;

    let upstream = remote(&far, &far_ref(3), &ChatSampling::default())
        .await
        .expect("looked up");

    assert_eq!(
        upstream.model.as_deref(),
        Some("3"),
        "the wire carries the id"
    );
    assert_eq!(upstream.counted_as, "org/qwen3");
    assert_eq!(upstream.made_by.model, "org/qwen3");
    assert_eq!(upstream.made_by.quantization.as_deref(), Some("Q4_K_M"));
    assert_eq!(upstream.far_model, Some(far_ref(3)));
    assert_eq!(upstream.local_model, None);
    assert_eq!(upstream.base_url, far.server_root());
    // The far proxy shapes the turn and folds its own layers.
    assert_eq!(upstream.model_context, ModelContext::passthrough());
    let seen = only(&fake);
    assert_eq!(
        (seen.method.as_str(), seen.uri.as_str()),
        ("GET", "/v1/gglib/models/3/detail")
    );
    assert!(carries_key(&seen), "{seen:?}");
}

/// A far run is handed nothing this machine stores: no global default
/// reaches its request, and its turn with tools is sent no temperature. The
/// far proxy folds its own layers.
#[tokio::test]
async fn a_far_turn_with_tools_is_sent_nothing_this_machine_stores() {
    use super::super::compose::{Prepared, prepare_over};
    use super::super::turn_fixture::{REPLY, TOOL, global, page, temperature};

    let (_dir, state) = super::super::run_fixture::state().await;
    global(&state, temperature(0.42)).await;
    let (fake, far) = fake_far(200, LOOKUP).await;
    let upstream = remote(&far, &far_ref(3), &ChatSampling::default())
        .await
        .expect("looked up");
    *fake.body.lock().unwrap() = REPLY.to_owned();
    *fake.content_type.lock().unwrap() = "text/event-stream";

    let chat = page(serde_json::json!({ "tool_filter": [TOOL] }));
    let prepared = prepare_over(&state, chat, upstream, None).await;
    let Prepared {
        agent_loop,
        messages,
        config,
        tx,
        rx: _rx,
        ..
    } = prepared;
    let ended = agent_loop.run(messages, config, tx).await;
    assert!(ended.is_ok(), "{:?}", ended.err());

    let turn = fake.seen.lock().unwrap().last().expect("a request").clone();
    assert_eq!(turn.uri, "/v1/chat/completions");
    let sent: serde_json::Value = serde_json::from_str(&turn.body).unwrap();
    assert!(
        sent["tools"]
            .as_array()
            .is_some_and(|tools| !tools.is_empty())
    );
    assert_eq!(sent.get("temperature"), None);
}

/// An id the far machine does not have is its `404`, with its words, and
/// no turn starts.
#[tokio::test]
async fn a_far_id_that_machine_does_not_have_is_its_404() {
    let refusal = r#"{"error":{"message":"No model with that id or name is in the catalog: 9","code":"model_not_found"}}"#;
    let (_, far) = fake_far(404, refusal).await;

    let Err(err) = remote(&far, &far_ref(9), &ChatSampling::default()).await else {
        panic!("a missing model was driven");
    };

    let HttpError::NotFound(message) = err else {
        panic!("got {err:?}");
    };
    assert!(message.ends_with("in the catalog: 9"), "{message}");
}

/// A key that machine no longer admits is a `409` that says to pair again,
/// never a `401`, which would read as this daemon wanting a key.
#[tokio::test]
async fn a_far_key_refused_at_lookup_is_a_conflict() {
    let (_, far) = fake_far(401, r#"{"error":{"message":"Invalid API key"}}"#).await;

    let Err(err) = remote(&far, &far_ref(3), &ChatSampling::default()).await else {
        panic!("a refused key was driven");
    };

    assert!(
        matches!(&err, HttpError::Conflict(m) if m.contains("pair again")),
        "got {err:?}"
    );
}
