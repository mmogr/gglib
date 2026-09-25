//! A page on another site can run, load or clear nothing through the proxy
//! (#1118).
//!
//! The proxy asks no key on loopback until one is stored, and its chat and
//! embeddings routes read raw bytes, so a form post from any page reaches
//! them; CORS only hides the answer. The origin guard is what refuses it.
//! Runs the real `gglib_proxy::serve` under the default access config, whose
//! CORS is `LocalOnly`, as the runtime's supervisor starts it; one test serves
//! it under `AllowAll`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use gglib_core::ports::{
    Admission, LaunchOverrides, ModelRuntimeError, ModelRuntimePort, RunningTarget,
};
use gglib_core::{CorsConfig, ProxyAccessConfig};
use reqwest::header::HeaderValue;
use reqwest::{Client, RequestBuilder, StatusCode};
use serde_json::Value;

mod fixtures;
use fixtures::access;
use fixtures::common::spawn_proxy_with_runtime;

const ELSEWHERE: &str = "https://evil.example";

/// Counts every call that would load, run or recycle a model.
#[derive(Debug, Default)]
struct Watched {
    calls: AtomicU64,
}

#[async_trait]
impl ModelRuntimePort for Watched {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let target = RunningTarget::local(0, 1, "m".into(), 4096, false);
        Ok(Admission::detached(target))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

async fn proxy() -> (String, Arc<Watched>, tokio_util::sync::CancellationToken) {
    let runtime = Arc::new(Watched::default());
    let (base, cancel) = spawn_proxy_with_runtime(runtime.clone(), "m", Vec::new()).await;
    (base, runtime, cancel)
}

/// The code a refusal carries, or `None` when the answer is not one.
async fn refusal_code(request: RequestBuilder) -> (StatusCode, Option<String>) {
    let response = request.send().await.unwrap();
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    let code = body["error"]["code"].as_str().map(str::to_owned);
    (status, code)
}

/// Each request as a page can send it without a preflight: a `text/plain`
/// body, or no body and no content type at all.
#[tokio::test]
async fn a_page_on_another_site_can_run_load_or_clear_nothing() {
    let (base, runtime, cancel) = proxy().await;
    let turn = r#"{"model":"m","messages":[{"role":"user","content":"hi"}]}"#;
    let embed = r#"{"model":"m","input":"hi"}"#;
    let client = Client::new();
    let requests = [
        ("/v1/chat/completions", Some(turn)),
        ("/v1/embeddings", Some(embed)),
        ("/v1/models/m/load", None),
        ("/v1/proxy/cache/clear", None),
    ];
    for (path, body) in requests {
        let mut request = client
            .post(format!("{base}{path}"))
            .header("origin", ELSEWHERE);
        if let Some(body) = body {
            request = request.header("content-type", "text/plain").body(body);
        }
        let (status, code) = refusal_code(request).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
        assert_eq!(code.as_deref(), Some("origin_not_allowed"), "{path}");
    }
    assert_eq!(
        runtime.calls.load(Ordering::SeqCst),
        0,
        "a model was touched"
    );
    cancel.cancel();
}

#[tokio::test]
async fn a_page_on_another_site_cannot_reach_the_tool_gateway() {
    let (base, _runtime, cancel) = proxy().await;
    let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
    let response = Client::new()
        .post(format!("{base}/mcp"))
        .header("origin", ELSEWHERE)
        .header("content-type", "text/plain")
        .body(initialize)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(response.headers().get("mcp-session-id").is_none());

    let ending = Client::new()
        .delete(format!("{base}/mcp"))
        .header("origin", ELSEWHERE)
        .header("mcp-session-id", "any");
    let (status, code) = refusal_code(ending).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(code.as_deref(), Some("origin_not_allowed"));
    cancel.cancel();
}

/// The tool gateway keeps its own check, held to local pages and the
/// proxy's own origin whatever the CORS config lets read. Under `AllowAll`
/// the router's guard lets a page on another site clear the cache, and the
/// gateway still opens it no session.
#[tokio::test]
async fn the_tool_gateway_refuses_another_site_even_when_every_page_may_read() {
    let every_page_reads = ProxyAccessConfig {
        cors: CorsConfig::AllowAll,
        ..ProxyAccessConfig::default()
    };
    let (base, _port, cancel) = access::spawn_proxy(every_page_reads).await;
    let client = Client::new();

    let clear = client
        .post(format!("{base}/v1/proxy/cache/clear"))
        .header("origin", ELSEWHERE);
    let (status, code) = refusal_code(clear).await;
    assert_ne!(code.as_deref(), Some("origin_not_allowed"));
    assert_eq!(status, StatusCode::OK);

    let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
    let response = client
        .post(format!("{base}/mcp"))
        .header("origin", ELSEWHERE)
        .header("content-type", "application/json")
        .body(initialize)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(response.headers().get("mcp-session-id").is_none());
    cancel.cancel();
}

#[tokio::test]
async fn a_cross_site_request_without_an_origin_is_refused_by_its_fetch_metadata() {
    let (base, runtime, cancel) = proxy().await;
    let request = Client::new()
        .post(format!("{base}/v1/proxy/cache/clear"))
        .header("sec-fetch-site", "cross-site");
    let (status, code) = refusal_code(request).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(code.as_deref(), Some("origin_not_allowed"));
    assert_eq!(runtime.calls.load(Ordering::SeqCst), 0);
    cancel.cancel();
}

/// No browser sends an `Origin` that is not text, but one that arrives is
/// refused. Read as absent, it would pass as a program's request.
#[tokio::test]
async fn an_origin_that_is_not_text_is_refused_not_read_as_absent() {
    let (base, runtime, cancel) = proxy().await;
    let origin = HeaderValue::from_bytes(b"http://\xffevil.example").unwrap();
    let request = Client::new()
        .post(format!("{base}/v1/proxy/cache/clear"))
        .header("origin", origin);
    let (status, code) = refusal_code(request).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(code.as_deref(), Some("origin_not_allowed"));
    assert_eq!(runtime.calls.load(Ordering::SeqCst), 0);
    cancel.cancel();
}

/// A page the proxy serves names the `Host` it was sent to, and passes on
/// that alone: `127.0.0.2` is loopback, so the Host guard admits it, but it
/// is not an origin `LocalOnly` lets read. Sent to another name, the same
/// origin is another site.
#[tokio::test]
async fn the_proxys_own_origin_passes_only_under_the_host_it_names() {
    let (base, runtime, cancel) = proxy().await;
    let port = base.rsplit(':').next().unwrap();
    let own = format!("http://127.0.0.2:{port}");

    let under_its_name = Client::new()
        .post(format!("{base}/v1/proxy/cache/clear"))
        .header("host", format!("127.0.0.2:{port}"))
        .header("origin", &own);
    let status = under_its_name.send().await.unwrap().status();
    assert_eq!(status, StatusCode::OK);

    let under_another_name = Client::new()
        .post(format!("{base}/v1/proxy/cache/clear"))
        .header("origin", &own);
    let (status, code) = refusal_code(under_another_name).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(code.as_deref(), Some("origin_not_allowed"));
    assert_eq!(runtime.calls.load(Ordering::SeqCst), 1);
    cancel.cancel();
}

/// The desktop app, a page on `localhost`, and a program sending no origin
/// all still clear the cache, which recycles the idle model each time.
#[tokio::test]
async fn the_desktop_app_local_pages_and_programs_still_change_things() {
    let (base, runtime, cancel) = proxy().await;
    let origins = [
        Some("tauri://localhost"),
        Some("http://localhost:5173"),
        None,
    ];
    for origin in origins {
        let mut request = Client::new().post(format!("{base}/v1/proxy/cache/clear"));
        if let Some(origin) = origin {
            request = request
                .header("origin", origin)
                .header("sec-fetch-site", "cross-site");
        }
        let status = request.send().await.unwrap().status();
        assert_eq!(status, StatusCode::OK, "{origin:?}");
    }
    assert_eq!(runtime.calls.load(Ordering::SeqCst), 3);
    cancel.cancel();
}
