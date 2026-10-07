//! A repair request as the daemon is sent it, and what the CLI reads of the
//! answer.

use std::io::Write as _;
use std::net::TcpListener;

use gglib_core::services::RepairStarted;

use super::super::{DaemonHandle, STAND_IN_PORT};
use crate::handlers::agent_chat::sight::sight_tests::read_request;

/// Answer the one request a stand-in daemon is sent with `status` and
/// `reply`, and hand back that request's first line and body.
fn stand_in(
    status: &'static str,
    reply: &'static str,
) -> (u16, std::thread::JoinHandle<(String, String)>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let daemon = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("the request");
        let request = read_request(&mut stream);
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\n\
             content-length: {}\r\nconnection: close\r\n\r\n{reply}",
            reply.len()
        );
        request
    });
    (port, daemon)
}

fn handle() -> DaemonHandle {
    DaemonHandle {
        client: gglib_proxy::loopback::client(),
        api_key: None,
    }
}

/// A repair is a POST to the model's repair route carrying the shards named,
/// and its answer is the download and the files in the daemon's reply.
#[tokio::test]
async fn a_repair_request_names_its_shards_and_answers_the_download_and_its_files() {
    let reply = r#"{"id":"owner/repo:Q8_0","files":["a.gguf","b.gguf"]}"#;
    let (port, daemon) = stand_in("200 OK", reply);

    let started = STAND_IN_PORT
        .scope(port, handle().repair_model(7, Some(vec![0, 2])))
        .await
        .expect("the daemon's answer");

    assert_eq!(
        started,
        RepairStarted {
            id: "owner/repo:Q8_0".to_owned(),
            files: vec!["a.gguf".to_owned(), "b.gguf".to_owned()],
        }
    );
    let (line, sent) = daemon.join().expect("the stand-in ran");
    assert_eq!(line, "POST /api/models/7/repair HTTP/1.1");
    let sent: serde_json::Value = serde_json::from_str(&sent).expect("a JSON body");
    assert_eq!(sent, serde_json::json!({ "shards": [0, 2] }));
}

/// With no shard named the daemon is sent none, which it reads as every
/// unhealthy file.
#[tokio::test]
async fn a_repair_of_every_unhealthy_file_names_no_shard() {
    let (port, daemon) = stand_in("200 OK", r#"{"id":"owner/repo:Q8_0","files":[]}"#);

    STAND_IN_PORT
        .scope(port, handle().repair_model(7, None))
        .await
        .expect("the daemon's answer");

    let (_, sent) = daemon.join().expect("the stand-in ran");
    let sent: serde_json::Value = serde_json::from_str(&sent).expect("a JSON body");
    assert_eq!(sent, serde_json::json!({ "shards": null }));
}

/// A repair the daemon refuses is an error in the daemon's words, which say
/// what became of the files.
#[tokio::test]
async fn a_refused_repair_is_an_error_in_the_daemons_words() {
    let reply = r#"{"error":"Failed to repair model: nothing fetches it","status":500}"#;
    let (port, daemon) = stand_in("500 Internal Server Error", reply);

    let refused = STAND_IN_PORT
        .scope(port, handle().repair_model(7, None))
        .await
        .unwrap_err();

    assert_eq!(
        refused.to_string(),
        "daemon answered 500 Internal Server Error: Failed to repair model: nothing fetches it"
    );
    daemon.join().expect("the stand-in ran");
}
