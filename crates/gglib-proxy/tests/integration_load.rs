//! `POST /v1/models/{name}/load` over real HTTP: the model is resident when
//! the call returns, and nothing is holding it.
//!
//! The route exists for a paired machine that wants a model loaded before a
//! turn needs it (`gglib serve --remote`, ADR 0013). What has to be true of
//! it is exactly what is true of a chat request's admission, minus the
//! request: the same queue and the same refusals, and then the lease
//! dropped at once — so a load never blocks the next swap the way an early
//! lease drop on a *response* would let a swap unload a model mid-stream.
//! [`ResidentSimRuntime`] counts leases in flight, which makes both halves
//! observable.

mod fixtures;

use std::collections::HashMap;
use std::sync::Arc;

use reqwest::Client;
use serde_json::{Value, json};

use gglib_core::ports::ModelRuntimePort;

use fixtures::common::{
    MultiModelCatalog, ResidentSimRuntime, spawn_mock_upstream, spawn_proxy_with_catalog,
};

const MODEL: &str = "chat-model";

/// One non-streaming completion frame, which the proxy re-emits as SSE.
const CHAT_STREAM: &[u8] =
    b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"},\"index\":0}]}\n\ndata: [DONE]\n\n";

#[tokio::test]
async fn loading_a_model_makes_it_resident_and_holds_nothing() {
    let upstream_cancel = tokio_util::sync::CancellationToken::new();
    let port = spawn_mock_upstream(vec![CHAT_STREAM], upstream_cancel.clone()).await;
    let runtime = Arc::new(ResidentSimRuntime::new(HashMap::from([(
        MODEL.to_string(),
        port,
    )])));
    let catalog = Arc::new(MultiModelCatalog(vec![(MODEL.to_string(), vec![])]));
    let (base, proxy_cancel) =
        spawn_proxy_with_catalog(Arc::clone(&runtime) as Arc<dyn ModelRuntimePort>, catalog).await;
    let client = Client::new();

    let first = client
        .post(format!("{base}/v1/models/{MODEL}/load"))
        .json(&json!({}))
        .send()
        .await
        .expect("request");
    assert_eq!(first.status(), 200);
    let body: Value = first.json().await.expect("json");
    assert_eq!(body["model"], MODEL);
    assert!(body["started"].is_boolean(), "{body}");
    assert!(body["context"].is_u64(), "{body}");
    assert_eq!(runtime.swaps(), 1, "the model was loaded");
    assert_eq!(runtime.inflight(), 0, "and nothing is holding it");

    // Loading what is loaded is a no-op the runtime never sees as a swap.
    let again = client
        .post(format!("{base}/v1/models/{MODEL}/load"))
        .send()
        .await
        .expect("request");
    assert_eq!(again.status(), 200);
    assert_eq!(runtime.swaps(), 1);
    assert_eq!(runtime.inflight(), 0);

    // A model that is not on the machine is refused as a chat request would
    // refuse it, and nothing was loaded for it.
    let missing = client
        .post(format!("{base}/v1/models/no-such-model/load"))
        .send()
        .await
        .expect("request");
    assert_eq!(
        missing.status(),
        404,
        "{}",
        missing.text().await.unwrap_or_default()
    );
    assert_eq!(runtime.swaps(), 1);

    proxy_cancel.cancel();
    upstream_cancel.cancel();
}
