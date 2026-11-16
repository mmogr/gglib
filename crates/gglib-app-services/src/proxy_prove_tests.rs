//! The proof request against a stand-in endpoint: who answered is read from
//! the answer, the key travels as the bearer and nowhere else, a refusal
//! carries the endpoint's words, and a listener that never comes is said.
//!
//! The stand-in reads each request to its end before answering, as a peer
//! must ([`gglib_test_http`]).

use std::io::Write as _;
use std::time::Duration;

use tokio::net::TcpListener;

use super::*;

/// A stand-in endpoint that answers `/health` and then one completion with
/// `status` and `body`; hands back the completion request it read.
async fn endpoint(
    status: &'static str,
    body: &'static str,
) -> (String, tokio::task::JoinHandle<gglib_test_http::Request>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port");
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let served = tokio::spawn(async move {
        loop {
            let (request, stream) = gglib_test_http::accept(&listener).await.expect("a request");
            let mut stream = stream.into_std().expect("a blocking stream");
            stream.set_nonblocking(false).ok();
            if request.line().starts_with("GET /health") {
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok"
                );
                continue;
            }
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            return request;
        }
    });
    (base, served)
}

#[tokio::test]
async fn the_answer_names_who_answered_and_the_key_travels_as_the_bearer() {
    let (base, served) = endpoint("200 OK", r#"{"model":"qwen3-8b","choices":[]}"#).await;
    let outcome = prove(
        &gglib_proxy::loopback::client(),
        &base,
        "Qwen3 8B",
        Some("k3y-5f3a"),
        &[],
        Duration::from_secs(5),
        Duration::from_secs(5),
    )
    .await
    .expect("answered");
    // A pinned endpoint answers with its pin whatever was asked for, so the
    // answer's model is the one reported, and the request's is kept beside it.
    assert_eq!(outcome.answered_by, "qwen3-8b");
    assert_eq!(outcome.asked_for, "Qwen3 8B");

    let request = served.await.unwrap();
    assert!(
        request.line().starts_with("POST /v1/chat/completions"),
        "{}",
        request.line()
    );
    assert_eq!(request.header("authorization"), Some("Bearer k3y-5f3a"));
    // The key travels as the bearer and nowhere else: not in the line, not
    // in the body.
    assert!(!request.line().contains("k3y-5f3a"), "{}", request.line());
    assert!(
        !request.body_text().contains("k3y-5f3a"),
        "{}",
        request.body_text()
    );
    let sent: serde_json::Value = serde_json::from_str(&request.body_text()).unwrap();
    assert_eq!(sent["model"], "Qwen3 8B");
    assert_eq!(sent["max_tokens"], 1);
    assert_eq!(sent["stream"], false);
}

#[tokio::test]
async fn without_a_key_none_is_sent_and_an_answer_without_a_model_names_the_one_asked_for() {
    let (base, served) = endpoint("200 OK", r#"{"choices":[]}"#).await;
    let outcome = prove(
        &gglib_proxy::loopback::client(),
        &base,
        "m",
        None,
        &[],
        Duration::from_secs(5),
        Duration::from_secs(5),
    )
    .await
    .expect("answered");
    assert_eq!(outcome.answered_by, "m");
    assert_eq!(served.await.unwrap().header("authorization"), None);
}

#[tokio::test]
async fn a_refusal_carries_the_endpoints_status_and_words() {
    let (base, _served) = endpoint("404 Not Found", r#"{"error":"model not found: m"}"#).await;
    let refusal = prove(
        &gglib_proxy::loopback::client(),
        &base,
        "m",
        None,
        &[],
        Duration::from_secs(5),
        Duration::from_secs(5),
    )
    .await
    .expect_err("refused");
    assert_eq!(
        refusal.reason,
        r#"404 Not Found: {"error":"model not found: m"}"#
    );
    assert!(!refusal.load_failed);
}

/// A model that did not load is refused in the endpoint's sentence alone,
/// marked as a failed load, so `gglib up` prints the sentence and not the
/// body it came in (#1959 C5).
#[tokio::test]
async fn a_model_that_did_not_load_is_refused_in_its_sentence() {
    let (base, _served) = endpoint(
        "500 Internal Server Error",
        r#"{"error":{"message":"llama-server stopped while loading m: it exited with status 1. It printed: gguf_init_from_reader: invalid magic characters","type":"server_error","code":"model_load_failed"}}"#,
    )
    .await;
    let refusal = prove(
        &gglib_proxy::loopback::client(),
        &base,
        "m",
        None,
        &[],
        Duration::from_secs(5),
        Duration::from_secs(5),
    )
    .await
    .expect_err("refused");
    assert_eq!(
        refusal.reason,
        "llama-server stopped while loading m: it exited with status 1. It printed: \
         gguf_init_from_reader: invalid magic characters"
    );
    assert!(refusal.load_failed);
}

#[tokio::test]
async fn a_listener_that_never_comes_is_said_within_the_bind_timeout() {
    // A port nothing listens on: bound and dropped, so it is free.
    let free = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let base = format!("http://{free}/v1");
    let begun = std::time::Instant::now();
    let refusal = prove(
        &gglib_proxy::loopback::client(),
        &base,
        "m",
        None,
        &[],
        Duration::from_millis(300),
        Duration::from_secs(1),
    )
    .await
    .expect_err("no listener");
    assert!(
        refusal.reason.contains("did not answer /health"),
        "{}",
        refusal.reason
    );
    // Said within the bind timeout, not some longer wait of its own.
    assert!(
        begun.elapsed() < Duration::from_secs(2),
        "{:?}",
        begun.elapsed()
    );
}

#[tokio::test]
async fn a_listener_that_never_answers_is_said_within_the_bind_timeout() {
    // Bound and held, never accepted from: the connection is made and the
    // request sent, and no answer comes, so one attempt is slow on every OS.
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/v1", held.local_addr().unwrap());
    let begun = std::time::Instant::now();
    let refusal = prove(
        &gglib_proxy::loopback::client(),
        &base,
        "m",
        None,
        &[],
        Duration::from_millis(300),
        Duration::from_secs(1),
    )
    .await
    .expect_err("no answer");
    assert!(
        refusal.reason.contains("did not answer /health"),
        "{}",
        refusal.reason
    );
    // An attempt gets no more than is left of the bind timeout.
    assert!(
        begun.elapsed() < Duration::from_secs(1),
        "{:?}",
        begun.elapsed()
    );
    drop(held);
}

/// The answer's keys are the contract's, which the CLI's mirror is held to.
#[test]
fn the_answer_carries_exactly_the_fields_the_contract_names() {
    let outcome = ProofOutcome {
        answered_by: "m".to_owned(),
        asked_for: "m".to_owned(),
        elapsed_ms: 1,
    };
    let value = serde_json::to_value(outcome).unwrap();
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    let mut named = gglib_core::contracts::http::daemon::PROXY_PROVE_FIELDS.to_vec();
    named.sort_unstable();
    assert_eq!(keys, named);
}

/// A model the catalogue holds but no file backs: enough for the endpoint
/// to pass the bearer check and refuse on the file, which is what these
/// tests read.
async fn a_catalogued_model(core: &gglib_core::services::AppCore) -> i64 {
    core.models()
        .add(gglib_core::domain::NewModel::new(
            "qwen3-8b".to_owned(),
            std::path::PathBuf::from("/models/absent.gguf"),
            Some(8.0),
            chrono::Utc::now(),
        ))
        .await
        .expect("a model")
        .id
}

/// The proof presents the key the endpoint demands of a request now, not
/// the one the run started with: an endpoint started open that has a key
/// stored later (`gglib remote enable`, a rotate) demands the stored key,
/// and the proof gets past the bearer check with it. Waits out the
/// endpoint's settings cache, as the endpoint itself does.
#[tokio::test]
async fn the_proof_presents_the_key_the_endpoint_demands_now() {
    let (core, proxy) = crate::test_support::test_core_and_proxy().await;
    let id = a_catalogued_model(&core).await;
    proxy
        .start(
            gglib_runtime::proxy::ProxyConfig {
                port: 0,
                ..gglib_runtime::proxy::ProxyConfig::default()
            },
            None,
        )
        .await
        .expect("the proxy starts");
    let past_the_bearer = |refusal: &crate::GuiError| {
        let text = refusal.to_string();
        assert!(!text.contains("401"), "{text}");
        assert!(text.contains("404"), "{text}");
    };
    past_the_bearer(&proxy.prove(Some(id)).await.expect_err("no file"));

    core.settings()
        .update(gglib_core::SettingsUpdate {
            proxy_api_key: Some(Some("stored-later".to_owned())),
            ..gglib_core::SettingsUpdate::default()
        })
        .await
        .unwrap();
    // The endpoint's settings cache lives five seconds (`SettingsCache`).
    tokio::time::sleep(Duration::from_secs(6)).await;
    assert_eq!(
        proxy.key_state().await,
        Some(gglib_core::domain::KeyState::Required)
    );
    past_the_bearer(&proxy.prove(Some(id)).await.expect_err("no file"));

    // An id the catalogue does not hold is a not-found, before any request.
    assert!(matches!(
        proxy.client_recipes(Some(id + 1000)).await,
        Err(crate::GuiError::NotFound { .. })
    ));
    // With no pin, a request must name the model.
    assert!(matches!(
        proxy.prove(None).await,
        Err(crate::GuiError::ValidationFailed(_))
    ));
    proxy.stop().await.expect("stop");
}

/// The endpoint config every proof test starts with: loopback, any port.
fn any_port() -> gglib_runtime::proxy::ProxyConfig {
    gglib_runtime::proxy::ProxyConfig {
        port: 0,
        ..gglib_runtime::proxy::ProxyConfig::default()
    }
}

/// With no model named, a pinned endpoint is described and proved by its
/// pin's id: another model in the catalogue, listed first, is never the one
/// asked for.
#[tokio::test]
async fn the_pin_is_proved_and_described_when_no_model_is_named() {
    let (core, proxy) = crate::test_support::test_core_and_proxy().await;
    let other = a_catalogued_model(&core).await;
    let pin = core
        .models()
        .add(gglib_core::domain::NewModel::new(
            "gemma-4b".to_owned(),
            std::path::PathBuf::from("/models/absent-too.gguf"),
            Some(4.0),
            chrono::Utc::now(),
        ))
        .await
        .expect("a model")
        .id;
    proxy
        .start(
            any_port(),
            Some(gglib_core::ports::PinnedSpec {
                id: pin,
                name: "gemma-4b".to_owned(),
                launch_overrides: gglib_core::server_config::ServerConfigOptions::default(),
            }),
        )
        .await
        .expect("the proxy starts pinned");
    let listed = crate::models::openai_ids(&core.models().list().await.unwrap());

    let recipes = proxy.client_recipes(None).await.expect("the pin's recipes");
    assert_eq!(recipes.model_id, listed[&pin]);
    assert_ne!(recipes.model_id, listed[&other]);

    // The proof reaches the endpoint, which admits the pin and refuses on
    // its missing file; a proof that named no model would never get there.
    let refusal = proxy.prove(None).await.expect_err("no file").to_string();
    assert!(refusal.contains("404"), "{refusal}");
    proxy.stop().await.expect("stop");
}

/// An image model is refused before any request: the proof is a chat
/// completion, which the endpoint refuses for a model that draws.
#[tokio::test]
async fn an_image_model_is_refused_before_any_request() {
    let (core, proxy) = crate::test_support::test_core_and_proxy().await;
    let mut new = gglib_core::domain::NewModel::new(
        "flux-schnell".to_owned(),
        std::path::PathBuf::from("/models/absent-flux.gguf"),
        Some(12.0),
        chrono::Utc::now(),
    );
    new.image_family = Some(gglib_core::domain::ImageFamily::Flux1);
    let id = core.models().add(new).await.expect("a model").id;
    proxy
        .start(any_port(), None)
        .await
        .expect("the proxy starts");
    match proxy.prove(Some(id)).await {
        Err(crate::GuiError::ValidationFailed(reason)) => {
            assert!(reason.contains("draws pictures"), "{reason}");
        }
        other => panic!("an image model must be refused up front: {other:?}"),
    }
    proxy.stop().await.expect("stop");
}

/// llama-server names the model in its answer by the file it was started
/// with; the proof says it as the Model ID of the library model that file
/// belongs to (the one asked for, when two share the file), and keeps an
/// answer that is no library file as it came.
#[test]
fn an_answer_naming_a_library_file_is_said_as_its_model_id() {
    let qwen = std::path::PathBuf::from("/Users/m/models/Qwen3-4B-Instruct-2507-Q8_0.gguf");
    let gemma = std::path::PathBuf::from("/Users/m/models/gemma-3-4b-it-Q4_K_M.gguf");
    let library = [
        (gemma, "Gemma-3-4B-It".to_owned()),
        (qwen.clone(), "Qwen3-4B-Instruct-2507".to_owned()),
        (qwen, "Qwen3-4B-Copy".to_owned()),
    ];
    let path = "/Users/m/models/Qwen3-4B-Instruct-2507-Q8_0.gguf";
    assert_eq!(
        named_as_listed(path, "Gemma-3-4B-It", &library),
        "Qwen3-4B-Instruct-2507"
    );
    assert_eq!(
        named_as_listed(path, "Qwen3-4B-Copy", &library),
        "Qwen3-4B-Copy"
    );
    assert_eq!(
        named_as_listed("/elsewhere/other.gguf", "x", &library),
        "/elsewhere/other.gguf"
    );
    assert_eq!(named_as_listed("qwen3-8b", "x", &library), "qwen3-8b");
}

/// The proof itself names the answer from the library it is given: an
/// endpoint whose answer carries llama-server's file path is said by Model ID.
#[tokio::test]
async fn a_proof_answered_by_a_file_path_says_the_model_id() {
    let (base, _served) = endpoint("200 OK", r#"{"model":"/m/q.gguf","choices":[]}"#).await;
    let outcome = prove(
        &gglib_proxy::loopback::client(),
        &base,
        "Q",
        None,
        &[(std::path::PathBuf::from("/m/q.gguf"), "Q".to_owned())],
        Duration::from_secs(5),
        Duration::from_secs(5),
    )
    .await
    .expect("answered");
    assert_eq!(outcome.answered_by, "Q");
    assert_eq!(outcome.asked_for, "Q");
}

// ---------------------------------------------------------------------
// What the daemon says of a proof whose model did not load (#1959 C5)
// ---------------------------------------------------------------------

/// A runtime that has kept, for each model id, the last load of it that
/// failed and when, as the real one does; it runs nothing.
#[derive(Debug, Default)]
struct Kept(std::collections::HashMap<u32, (Instant, gglib_core::ports::LoadFailure)>);

#[async_trait::async_trait]
impl gglib_core::ports::ModelRuntimePort for Kept {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: gglib_core::ports::LaunchOverrides,
    ) -> Result<gglib_core::ports::Admission, gglib_core::ports::ModelRuntimeError> {
        Err(gglib_core::ports::ModelRuntimeError::Internal(
            "nothing runs here".to_owned(),
        ))
    }
    async fn current_model(&self) -> Option<gglib_core::ports::RunningTarget> {
        None
    }
    async fn stop_current(&self) -> Result<(), gglib_core::ports::ModelRuntimeError> {
        Ok(())
    }
    async fn stop_model(
        &self,
        _model_id: u32,
    ) -> Result<bool, gglib_core::ports::ModelRuntimeError> {
        Ok(false)
    }
    fn last_load_failure(
        &self,
        model_id: u32,
    ) -> Option<(Instant, gglib_core::ports::LoadFailure)> {
        self.0.get(&model_id).cloned()
    }
}

/// Model `name`'s load, stopped with exit 1 after printing `line`.
fn stopped(name: &str, line: &str) -> gglib_core::ports::LoadFailure {
    gglib_core::ports::LoadFailure {
        model: name.to_owned(),
        runtime: gglib_core::domain::RuntimeKind::Llama,
        cause: gglib_core::ports::LoadCause::Stopped {
            failure: Some(gglib_core::utils::process::bounded::Failure::Failed { code: 1 }),
            said: Some(line.to_owned()),
        },
        fitted_context: None,
    }
}

/// What the endpoint refused the proof of `qwen` with: the short form.
fn short_refusal() -> ProofRefusal {
    ProofRefusal {
        reason: stopped("qwen", "gguf_init_from_reader: invalid magic").remote_sentence(),
        load_failed: true,
    }
}

/// The daemon's sentence for `refused`, or a panic naming what came back.
fn load_failed_sentence(refused: crate::GuiError) -> String {
    match refused {
        crate::GuiError::LoadFailed(sentence) => sentence,
        other => panic!("a failed load, not {other:?}"),
    }
}

/// (a) This model's load, failed after the proof was asked: its whole
/// sentence, with the line the engine printed.
#[test]
fn a_failure_of_this_proofs_load_is_said_in_full() {
    let asked = Instant::now();
    let failure = stopped("qwen", "gguf_init_from_reader: invalid magic");
    let runtime = Kept([(3, (Instant::now(), failure.clone()))].into());

    let said = load_failed_sentence(refused(short_refusal(), &runtime, 3, asked));

    assert_eq!(said, failure.sentence());
    assert!(said.contains("It printed: gguf_init_from_reader"), "{said}");
}

/// (b) A failure of this model kept from before the proof was asked is an
/// earlier load's, not this one's: the endpoint's own words stand.
#[test]
fn a_failure_from_before_the_proof_is_not_its_reason() {
    let earlier = Instant::now();
    let runtime = Kept([(3, (earlier, stopped("qwen", "an earlier load's line")))].into());
    std::thread::sleep(Duration::from_millis(5));
    let asked = Instant::now();

    let said = load_failed_sentence(refused(short_refusal(), &runtime, 3, asked));

    assert_eq!(said, short_refusal().reason);
}

/// (c) Another model's failure, however recent, is never said as the reason
/// this model did not answer.
#[test]
fn another_models_failure_is_not_its_reason() {
    let asked = Instant::now();
    let runtime = Kept(
        [(
            7,
            (
                Instant::now(),
                stopped("llama-3-8b", "another model's line"),
            ),
        )]
        .into(),
    );

    let said = load_failed_sentence(refused(short_refusal(), &runtime, 3, asked));

    assert_eq!(said, short_refusal().reason);
    assert!(!said.contains("another model's line"), "{said}");
}

/// (d) Nothing kept: the endpoint's own words; and a refusal that is not a
/// failed load stays what it was.
#[test]
fn with_nothing_kept_the_endpoints_words_stand() {
    let asked = Instant::now();
    let runtime = Kept::default();

    let said = load_failed_sentence(refused(short_refusal(), &runtime, 3, asked));
    let other = refused(
        ProofRefusal {
            reason: "404 Not Found: {}".to_owned(),
            load_failed: false,
        },
        &runtime,
        3,
        asked,
    );

    assert_eq!(said, short_refusal().reason);
    assert!(
        matches!(&other, crate::GuiError::ValidationFailed(r) if r == "404 Not Found: {}"),
        "{other:?}"
    );
}
