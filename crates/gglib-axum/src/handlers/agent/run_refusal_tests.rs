//! A run whose request the model's endpoint refused ends with the
//! endpoint's code when it is one of gglib's own, and `agent_error` when it
//! is not. On a paired machine's model the `error` event says the
//! refusal's code and names that machine (`LLM stream error: <machine>
//! answered …`), so its advice reads as advice for that machine.

use std::time::Duration;

use futures_util::StreamExt as _;
use gglib_core::domain::runs::{RunInfo, RunStatus};
use gglib_core::domain::{Machine, ModelRef};
use gglib_core::ports::{RunEvent, RunsPort as _};
use serde_json::{Value, json};

use super::chat_sampling::ChatSampling;
use super::compose::prepare_over;
use super::remote_upstream::remote;
use super::run_fixture::{LOCAL, settled, start, state};
use super::turn_fixture::page;
use crate::handlers::remote::fake_far::{FINGERPRINT, far as fake_far};

/// The far detail route's answer for model 3, as a gglib proxy writes it.
const LOOKUP: &str = r#"{"detail":{"id":3,"name":"org/qwen3","paramCountB":8.0,
    "quantization":"Q4_K_M","addedAt":"2026-10-01 09:00:00","isServing":false,"metadata":{}}}"#;

/// The far proxy's refusal of an image for a model with no projector.
const CANNOT_READ: &str = r#"{"error":{"message":"Model '3' cannot read images: it has no projector linked. Link one with `gglib model update 3 --projector <path>`, or name a model that has one.","type":"invalid_request_error","code":"model_cannot_read_images"}}"#;

/// The tunnel edge's refusal when the far model server is wedged: a code,
/// but not one of gglib's.
const BAD_GATEWAY: &str =
    r#"{"error":{"message":"the backend answered something unreadable","code":"bad_gateway"}}"#;

/// Run a turn on the paired machine's model 3, whose completion request it
/// refuses with `status` and `body`: how the run ended, its last frame, and
/// the name the machine is shown by.
async fn refused_far_run(status: u16, body: &str) -> (RunInfo, Value, String) {
    let (_dir, state) = state().await;
    let (fake, far) = fake_far(200, LOOKUP).await;
    let model = ModelRef {
        machine: Machine::Paired {
            fingerprint: FINGERPRINT.to_owned(),
        },
        id: 3,
    };
    let upstream = remote(&far, &model, &ChatSampling::default())
        .await
        .expect("looked up");
    *fake.status.lock().unwrap() = status;
    *fake.body.lock().unwrap() = body.to_owned();

    let prepared = prepare_over(&state, page(json!({})), upstream, None).await;
    start(&state, "a1", None, prepared).await;
    settled(&state).await;

    let info = state.runs.get(&LOCAL, "a1").unwrap();
    let mut events = state.runs.events(&LOCAL, "a1", 0).unwrap();
    let mut last = None;
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(item) = events.next().await {
            if let RunEvent::Frame { data, .. } = item {
                last = Some(data);
            }
        }
    })
    .await
    .expect("the log is read to its end");
    let last = serde_json::from_str(&last.expect("a frame")).unwrap();
    (info, last, far.shown_name().to_owned())
}

#[tokio::test]
async fn a_far_refusal_with_gglibs_code_ends_the_run_with_it() {
    let (info, last, name) = refused_far_run(400, CANNOT_READ).await;

    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(
        info.error.map(|e| e.code).as_deref(),
        Some("model_cannot_read_images")
    );
    assert_eq!(last["type"], "error");
    assert_eq!(last["code"], "model_cannot_read_images");
    let message = last["message"].as_str().unwrap();
    assert!(
        message.starts_with(&format!(
            "LLM stream error: {name} answered 400 Bad Request"
        )),
        "{message}"
    );
    assert!(
        message.contains("gglib model update 3 --projector"),
        "{message}"
    );
}

#[tokio::test]
async fn a_far_refusal_with_a_code_not_gglibs_ends_the_run_as_agent_error() {
    let (info, last, name) = refused_far_run(502, BAD_GATEWAY).await;

    assert_eq!(info.error.map(|e| e.code).as_deref(), Some("agent_error"));
    assert_eq!(
        last["code"], "bad_gateway",
        "the event says what was written"
    );
    assert!(last["message"].as_str().unwrap().contains(&name), "{last}");
}
