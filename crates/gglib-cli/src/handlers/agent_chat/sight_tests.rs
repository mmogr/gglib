//! What each kind of session knows of its model's image input, and the
//! `--port` path against a server that answers `/props`.

use std::io::Write as _;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use gglib_test_http::blocking;

use super::*;
use crate::bootstrap::test_context;
use crate::target::target_tests::{UNREADABLE, with_unreadable_catalogue};

/// One short reply, as llama-server streams it.
const REPLY: &str = "data: {\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\
     \"content\":\"A cat.\"},\"finish_reason\":null}]}\n\n\
     data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
     data: [DONE]\n\n";

/// A stand-in for llama-server on a loopback port. It reads each request
/// whole before it answers: `/props` with the JSON it was made with, and
/// anything else with [`REPLY`] as an event stream.
pub(crate) struct FakeServer {
    pub port: u16,
    /// Each request's first line, and its body, in order.
    seen: Arc<Mutex<Vec<(String, String)>>>,
}

impl FakeServer {
    /// Every request it was sent: the first line, and the body.
    fn seen(&self) -> Vec<(String, String)> {
        self.seen.lock().unwrap().clone()
    }

    /// The request lines it was sent.
    pub(crate) fn requests(&self) -> Vec<String> {
        self.seen().into_iter().map(|(line, _)| line).collect()
    }

    /// The body of the one request whose line starts with `line`.
    pub(crate) fn body_of(&self, line: &str) -> String {
        let seen = self.seen();
        let mut bodies = seen.iter().filter(|(sent, _)| sent.starts_with(line));
        let body = bodies.next().expect("the request was sent").1.clone();
        assert!(bodies.next().is_none(), "{line} was sent more than once");
        body
    }
}

/// A [`FakeServer`] whose `/props` is `props`.
pub(crate) fn props_server(props: &'static str) -> FakeServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let requests = Arc::clone(&seen);
    std::thread::spawn(move || {
        loop {
            let Ok(taken) = blocking::incoming(&listener) else {
                continue;
            };
            // A connection that sends no whole request still counts as one.
            let Ok((request, mut stream)) = taken.request() else {
                requests
                    .lock()
                    .unwrap()
                    .push((String::new(), String::new()));
                continue;
            };
            let line = request.line().to_owned();
            let (kind, body) = if line.starts_with("GET /props ") {
                ("application/json", props)
            } else {
                ("text/event-stream", REPLY)
            };
            requests.lock().unwrap().push((line, request.body_text()));
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: {kind}\r\ncontent-length: {}\r\n\
                 connection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    FakeServer { port, seen }
}

/// A loopback port nothing listens on.
fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    listener.local_addr().expect("its address").port()
}

/// What a `--port` session on a server answering `body` says to an image.
async fn on_server(body: &'static str) -> Result<()> {
    let server = props_server(body);
    Sight::server("qwen", reqwest::Client::new(), server.port, None)
        .admit(true)
        .await
}

#[tokio::test]
async fn a_catalogue_model_with_no_projector_is_refused_by_name() {
    let refused = Sight::catalogue("qwen", false)
        .admit(true)
        .await
        .expect_err("it cannot see");

    assert_eq!(
        refused.to_string(),
        gglib_core::request_pipeline::CannotReadImages.message("qwen")
    );
    assert!(
        refused
            .to_string()
            .contains("gglib model update qwen --projector")
    );
}

#[tokio::test]
async fn a_catalogue_model_with_a_projector_is_sent_the_image() {
    assert!(Sight::catalogue("qwen", true).admit(true).await.is_ok());
}

#[tokio::test]
async fn a_run_with_no_image_is_never_refused() {
    assert!(Sight::catalogue("qwen", false).admit(false).await.is_ok());
}

#[tokio::test]
async fn a_session_nothing_can_judge_proceeds() {
    assert!(Sight::unjudged().admit(true).await.is_ok());
}

#[tokio::test]
async fn a_server_that_says_it_sees_is_sent_the_image() {
    assert!(on_server(r#"{"modalities":{"vision":true}}"#).await.is_ok());
}

#[tokio::test]
async fn a_server_that_says_it_cannot_see_is_refused_with_the_same_words() {
    let refused = on_server(r#"{"modalities":{"vision":false,"audio":false}}"#)
        .await
        .expect_err("the server cannot see");

    assert_eq!(
        refused.to_string(),
        gglib_core::request_pipeline::CannotReadImages.message("qwen")
    );
}

#[tokio::test]
async fn a_server_that_names_no_modalities_proceeds() {
    assert!(on_server(r#"{"total_slots":1}"#).await.is_ok());
}

#[tokio::test]
async fn a_server_that_does_not_answer_proceeds() {
    let sight = Sight::server("qwen", reqwest::Client::new(), closed_port(), None);

    assert!(sight.admit(true).await.is_ok());
}

#[tokio::test]
async fn the_server_is_asked_its_props_and_only_when_an_image_is_sent() {
    let server = props_server(r#"{"modalities":{"vision":true}}"#);
    let sight = Sight::server("qwen", reqwest::Client::new(), server.port, None);

    sight.admit(false).await.unwrap();
    assert!(server.requests().is_empty());

    sight.admit(true).await.unwrap();
    assert_eq!(server.requests(), ["GET /props HTTP/1.1"]);
}

/// The parameters of a local session on `identifier`.
fn params(identifier: &str, target: Target, port: Option<u16>) -> AgentSessionParams {
    AgentSessionParams {
        model_identifier: identifier.to_owned(),
        ctx_size: None,
        port,
        target,
        tools: Vec::new(),
        model_name: None,
        retry_policy: gglib_core::retry::RetryPolicy::default(),
        profile: None,
        turn: None,
        drawing: None,
    }
}

/// A context whose catalogue holds `qwen`, linked to a projector or not.
async fn with_qwen(dir: &tempfile::TempDir, linked: bool) -> CliContext {
    let ctx = test_context(dir.path()).await;
    let mut model = gglib_core::domain::NewModel::new(
        "qwen".to_owned(),
        dir.path().join("qwen.gguf"),
        8.0,
        chrono::Utc::now(),
    );
    model.projector_path = linked.then(|| dir.path().join("mmproj-F16.gguf"));
    ctx.app.models().add(model).await.expect("registered");
    ctx
}

#[tokio::test]
async fn a_local_session_is_judged_by_its_catalogue_row() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = with_qwen(&dir, false).await;

    let sight = Sight::of_session(&ctx, &params("qwen", Target::Local, None)).await;
    let sight = sight.expect("judged");

    let refused = sight.admit(true).await.expect_err("no projector linked");
    assert!(
        refused
            .to_string()
            .starts_with("Model 'qwen' cannot read images")
    );
}

#[tokio::test]
async fn a_local_model_the_catalogue_does_not_hold_is_not_judged() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;

    let sight = Sight::of_session(&ctx, &params("qwen", Target::Local, None)).await;

    assert!(sight.expect("judged").admit(true).await.is_ok());
}

/// A catalogue that cannot be read ends the session. Its model is not left
/// unjudged, which would send an image to a model that may not read one.
#[tokio::test]
async fn a_local_session_whose_catalogue_cannot_be_read_is_an_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = with_unreadable_catalogue(&dir).await;

    let judged = Sight::of_session(&ctx, &params("qwen", Target::Local, None)).await;

    let error = judged.err().expect("the read failed").to_string();
    assert_eq!(error, UNREADABLE);
}

/// A session on `--port` whose catalogue cannot be read is still judged by
/// its server, since the server answers the turn.
#[tokio::test]
async fn a_port_session_whose_catalogue_cannot_be_read_is_judged_by_its_server() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = with_unreadable_catalogue(&dir).await;
    let server = props_server(r#"{"modalities":{"vision":false}}"#);
    let session = params("qwen", Target::Local, Some(server.port));

    let sight = Sight::of_session(&ctx, &session).await.expect("judged");

    assert!(sight.admit(true).await.is_err(), "the server cannot see");
}

/// The catalogue says `qwen` cannot see; the server `--port` names says it
/// can, and it is the one that answers the turn.
#[tokio::test]
async fn a_port_session_is_judged_by_its_server_not_the_catalogue() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = with_qwen(&dir, false).await;
    let server = props_server(r#"{"modalities":{"vision":true}}"#);
    let session = params("qwen", Target::Local, Some(server.port));

    let sight = Sight::of_session(&ctx, &session).await.expect("judged");

    assert!(sight.admit(true).await.is_ok());
    assert_eq!(server.requests().len(), 1);
}

/// A server that does not say leaves it to the row: `qwen` has no
/// projector, so it is refused before llama-server would answer 500.
#[tokio::test]
async fn a_port_session_whose_server_does_not_say_is_judged_by_the_row() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = with_qwen(&dir, false).await;
    let server = props_server(r#"{"total_slots":1}"#);
    let session = params("qwen", Target::Local, Some(server.port));

    let sight = Sight::of_session(&ctx, &session).await.expect("judged");

    let refused = sight
        .admit(true)
        .await
        .expect_err("the row has no projector");
    assert_eq!(
        refused.to_string(),
        gglib_core::request_pipeline::CannotReadImages.message("qwen")
    );
}

/// `qwen` is linked to a projector and the server cannot see: it was
/// started without it, and linking one would change nothing.
#[tokio::test]
async fn a_port_server_started_without_the_linked_projector_is_named() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = with_qwen(&dir, true).await;
    let server = props_server(r#"{"modalities":{"vision":false}}"#);
    let session = params("qwen", Target::Local, Some(server.port));

    let sight = Sight::of_session(&ctx, &session).await.expect("judged");

    let refused = sight.admit(true).await.expect_err("the server cannot see");
    let sentence = gglib_core::request_pipeline::CannotReadImages
        .started_without_projector("qwen", server.port);
    assert_eq!(refused.to_string(), sentence);
    assert!(sentence.contains(&format!("port {}", server.port)));
    assert!(sentence.contains("drop `--port`"));
}

/// The paired machine's model is not this catalogue's to judge, even when
/// a model of the same name here has no projector.
#[tokio::test]
async fn a_far_session_is_not_judged_here() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = with_qwen(&dir, false).await;

    let sight = Sight::of_session(&ctx, &params("qwen", Target::Remote, None)).await;

    assert!(sight.expect("judged").admit(true).await.is_ok());
}
