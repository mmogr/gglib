//! What `gglib model load` asks the daemon to start: `--context`, from the
//! flag to the launch options the daemon makes of it.

use std::io::Write as _;
use std::net::TcpListener;

use gglib_app_services::launch_options::plan_bare_launch;
use gglib_core::Settings;
use gglib_test_http::blocking;

use gglib_app_services::types::StartServerRequest;
use gglib_core::domain::Model;

use super::*;
use crate::bootstrap::test_context;
use crate::daemon_client::{DaemonHandle, STAND_IN_PORT, paths};

/// A model in a fresh catalogue, trained on a window of 131072 tokens.
async fn a_model(dir: &tempfile::TempDir) -> Model {
    let ctx = test_context(dir.path()).await;
    let mut model = gglib_core::domain::NewModel::new(
        "qwen".to_owned(),
        dir.path().join("qwen.gguf"),
        8.0,
        chrono::Utc::now(),
    );
    model.context_length = Some(131_072);
    ctx.app.models().add(model).await.expect("registered")
}

/// Send `body` as a load does, to a stand-in that answers as the daemon
/// would, and give back the request it was sent: its first line and its body.
async fn sent(body: &StartServerBody) -> (String, String) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let daemon = std::thread::spawn(move || {
        let (request, mut stream) = blocking::accept(&listener).expect("the request");
        let reply = r#"{"port":9001,"message":"Server started on port 9001"}"#;
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
             content-length: {}\r\nconnection: close\r\n\r\n{reply}",
            reply.len()
        );
        (request.line().to_owned(), request.body_text())
    });
    let handle = DaemonHandle {
        client: gglib_proxy::loopback::client(),
        api_key: None,
    };

    let started = STAND_IN_PORT
        .scope(port, handle.start_model_server(body))
        .await
        .expect("the daemon's answer");

    assert_eq!(started.message, "Server started on port 9001");
    daemon.join().expect("the stand-in ran")
}

/// `--context` reaches the launch. The load's body carries it under the
/// name the daemon reads, and from the bytes that were sent the daemon's own
/// request type and the planner a start runs make it the launch's explicit
/// context. `max` is the model's trained window, and no flag is no opinion.
#[tokio::test]
async fn a_ctx_size_flag_reaches_the_launch_options_the_daemon_plans() {
    let dir = tempfile::tempdir().expect("tempdir");
    let model = a_model(&dir).await;

    for (flag, want) in [
        (Some("8192"), Some(8192)),
        (Some("max"), Some(131_072)),
        (None, None),
    ] {
        let body = start_body(&model, flag).expect("a flag of the right shape");

        let (line, sent) = sent(&body).await;

        let route = paths::SERVERS_START_PATH;
        assert_eq!(line, format!("POST {route} HTTP/1.1"));
        let request: StartServerRequest =
            serde_json::from_str(&sent).expect("the daemon's request, flat beside the id");
        let (fallback, launch) = plan_bare_launch(&model, &Settings::default(), &request);
        assert_eq!(launch.options.context_size, want, "{flag:?} sent {sent}");
        assert_eq!(fallback, want, "{flag:?} sent {sent}");
        let sent: serde_json::Value = serde_json::from_str(&sent).expect("a JSON body");
        assert_eq!(sent["id"], model.id);
    }

    // Neither a number nor `max`: refused here, with nothing to send.
    assert!(start_body(&model, Some("banana")).is_err());
}
