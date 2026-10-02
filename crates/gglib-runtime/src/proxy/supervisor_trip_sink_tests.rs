//! The loop guard's trip sink across proxy runs: a request has to resolve to
//! a catalogued model before the guard scans it, so these run against a
//! catalog that holds one.

use super::*;

/// The one model [`OneModelCatalog`] holds.
const MODEL: &str = "catalogued-model";

/// A catalog holding [`MODEL`], by that name, and nothing else.
#[derive(Debug)]
struct OneModelCatalog;

#[async_trait]
impl ModelCatalogPort for OneModelCatalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(vec![])
    }

    async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok((name == MODEL).then(|| ModelSummary {
            id: 1,
            name: MODEL.to_owned(),
            tags: vec![],
            capabilities: gglib_core::domain::ModelCapabilities::empty(),
            dialect: None,
            template_caps: None,
            param_count: String::new(),
            quantization: None,
            architecture: None,
            created_at: 0,
            file_size: 0,
            context_length: None,
            inference_defaults: None,
            defaults_origin: None,
            server_defaults: None,
        }))
    }

    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

/// A sink that counts the decisions it is handed.
#[derive(Default)]
struct CountingSink(std::sync::atomic::AtomicUsize);

impl gglib_core::ports::LoopGuardTripSink for CountingSink {
    fn record_trip(&self, _event: gglib_core::domain::loop_guard_log::LoopGuardTripEvent) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    fn record_scan(&self, _model_name: &str, _mode: gglib_core::LoopGuardMode, _at_secs: u64) {}
}

/// Post a history that trips the loop guard — three identical batches of a
/// mutating tool, each answered the same way — to the proxy at `addr`. The
/// model is catalogued, so the guard runs, and the runtime admits nothing, so
/// admission then refuses it.
async fn post_a_loop(addr: std::net::SocketAddr) {
    let call = serde_json::json!({ "role": "assistant", "content": null, "tool_calls": [{
        "id": "c1", "type": "function",
        "function": { "name": "write_file", "arguments": "{\"path\":\"a\"}" }
    }] });
    let answer = serde_json::json!({ "role": "tool", "tool_call_id": "c1", "content": "done" });
    let mut messages = vec![serde_json::json!({ "role": "user", "content": "go" })];
    for _ in 0..3 {
        messages.push(call.clone());
        messages.push(answer.clone());
    }
    messages.push(serde_json::json!({ "role": "user", "content": "continue" }));
    gglib_proxy::loopback::client_builder()
        .build()
        .unwrap()
        .post(format!("http://{addr}/v1/chat/completions"))
        .json(&serde_json::json!({ "model": MODEL, "messages": messages }))
        .send()
        .await
        .expect("the proxy answers");
}

/// The sink a supervisor is built with reaches every proxy it starts, and a
/// stop and a start keep counting into it.
#[tokio::test]
async fn the_trip_sink_reaches_every_proxy_run() {
    let sink = Arc::new(CountingSink::default());
    let supervisor = ProxySupervisor::with_trip_sink(
        Arc::clone(&sink) as Arc<dyn gglib_core::ports::LoopGuardTripSink>
    );
    let config = ProxyConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        ..ProxyConfig::default()
    };
    for expected in 1..=2 {
        let (runtime, catalog) = (Arc::new(MockRuntimePort), Arc::new(OneModelCatalog));
        let bind = supervisor
            .start(
                config.clone(),
                runtime,
                catalog,
                make_mcp(),
                make_settings_repo(),
            )
            .await
            .unwrap();
        post_a_loop(bind.addr).await;
        supervisor.stop().await.unwrap();
        assert_eq!(
            sink.0.load(std::sync::atomic::Ordering::SeqCst),
            expected,
            "run {expected} records into the same sink"
        );
    }
}
