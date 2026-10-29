//! A chat model too big for the device, refused at admission, over real HTTP:
//! the status, type and code `docs/error-codes.json` publishes for
//! `model_does_not_fit`, the runtime's words, and no `Retry-After`, streamed
//! or not.

mod fixtures;

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::ports::{
    Admission, LaunchOverrides, ModelRuntimeError, ModelRuntimePort, RunningTarget,
};
use reqwest::Client;
use serde_json::{Value, json};

use fixtures::common::spawn_proxy_with_runtime;

const MODEL: &str = "too-big";
const GIB: u64 = 1024 * 1024 * 1024;

/// A runtime on a device that holds no model it is asked for.
#[derive(Debug)]
struct TooSmall;

#[async_trait]
impl ModelRuntimePort for TooSmall {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        Err(ModelRuntimeError::ChatModelDoesNotFit {
            needed_bytes: 3 * GIB,
            budget_bytes: 2 * GIB,
        })
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }

    async fn stop_model(&self, _model_id: u32) -> Result<bool, ModelRuntimeError> {
        Ok(false)
    }
}

/// The row `docs/error-codes.json` publishes for `code`.
fn published(code: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/error-codes.json");
    let file: Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("the published file"))
            .expect("the published file is JSON");
    file["codes"]
        .as_array()
        .expect("a list of codes")
        .iter()
        .find(|row| row["code"] == code)
        .unwrap_or_else(|| panic!("{code} is published"))
        .clone()
}

#[tokio::test]
async fn a_chat_model_too_big_for_the_device_is_refused_as_published() {
    let row = published("model_does_not_fit");
    let (base, cancel) = spawn_proxy_with_runtime(
        Arc::new(TooSmall) as Arc<dyn ModelRuntimePort>,
        MODEL,
        vec![],
    )
    .await;

    for stream in [false, true] {
        let response = Client::new()
            .post(format!("{base}/v1/chat/completions"))
            .json(&json!({
                "model": MODEL,
                "messages": [{ "role": "user", "content": "hello" }],
                "stream": stream
            }))
            .send()
            .await
            .expect("proxy request");

        assert_eq!(
            Some(u64::from(response.status().as_u16())),
            row["status"].as_u64(),
            "stream {stream}"
        );
        assert!(
            !response.headers().contains_key("retry-after"),
            "a model too big for the device does not fit by waiting"
        );
        let body: Value = response.json().await.expect("a JSON error body");
        let error = &body["error"];
        assert_eq!(error["code"], "model_does_not_fit", "{body}");
        assert_eq!(error["type"], row["type"], "{body}");
        let message = error["message"].as_str().expect("a message");
        for named in ["3.00 GiB", "2.00 GiB", "smaller model"] {
            assert!(message.contains(named), "{named} missing from {message}");
        }
    }

    cancel.cancel();
}
