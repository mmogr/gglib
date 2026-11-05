//! Tests for [`super`]: what a load and an unload send the daemon, in what
//! order, and that what they print is the daemon's answer.

use std::io::Write as _;
use std::net::TcpListener;

use gglib_app_services::types::StartServerRequest;

use super::{load_on, unload_on};
use crate::daemon_client::{DaemonHandle, STAND_IN_PORT, StartServerBody, paths};
use gglib_test_http::blocking;

/// One request the stand-in daemon took: its request line and its body.
struct Taken {
    line: String,
    body: serde_json::Value,
}

/// A stand-in daemon that answers `replies` in order, each `(status, body)`,
/// closing every connection, and `f` run against it. Answers what `f`
/// answered beside the requests the stand-in took.
async fn against<T, F, Fut>(replies: Vec<(u16, String)>, f: F) -> (T, Vec<Taken>)
where
    F: FnOnce(DaemonHandle) -> Fut,
    Fut: Future<Output = T>,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let daemon = std::thread::spawn(move || {
        let mut taken = Vec::new();
        for (status, reply) in replies {
            let (request, mut stream) = blocking::accept(&listener).expect("the request");
            let reason = if status == 200 { "OK" } else { "Refused" };
            let _ = write!(
                stream,
                "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{reply}",
                reply.len()
            );
            taken.push(Taken {
                line: request.line().to_owned(),
                body: serde_json::from_str(&request.body_text()).unwrap_or_default(),
            });
        }
        taken
    });
    let handle = DaemonHandle {
        client: gglib_proxy::loopback::client(),
        api_key: None,
    };
    let answered = STAND_IN_PORT.scope(port, f(handle)).await;
    (answered, daemon.join().expect("the stand-in ran"))
}

fn body(id: i64, context: Option<u64>) -> StartServerBody {
    StartServerBody {
        id,
        config: StartServerRequest {
            context_length: context,
            ..Default::default()
        },
    }
}

/// A load is one start request, the model's id and context flat in its
/// body, and the line printed is the daemon's own sentence.
#[tokio::test]
async fn a_load_asks_the_daemon_to_start_the_model_and_prints_its_sentence() {
    let reply = serde_json::json!({ "port": 9301, "message": "Server started on port 9301" });
    let (lines, taken) = against(vec![(200, reply.to_string())], |handle| async move {
        load_on(&handle, &body(3, Some(4096)), false).await
    })
    .await;

    assert_eq!(lines.unwrap(), ["  Server started on port 9301"]);
    assert_eq!(taken.len(), 1);
    assert_eq!(
        taken[0].line,
        format!("POST {} HTTP/1.1", paths::SERVERS_START_PATH)
    );
    assert_eq!(taken[0].body["id"], 3);
    assert_eq!(taken[0].body["contextLength"], 4096);
}

/// With `--pin` the endpoint is pinned first and the model loaded second: a
/// plain start would bring up an unpinned endpoint that the pin then refuses.
/// The pinned body carries the same options the load does.
#[tokio::test]
async fn a_pinned_load_pins_the_endpoint_before_it_loads_the_model() {
    let pinned = serde_json::json!({ "running": true, "port": 8080, "pinned_model": "qwen3" });
    let started = serde_json::json!({ "port": 9301, "message": "Server started on port 9301" });
    let (lines, taken) = against(
        vec![(200, pinned.to_string()), (200, started.to_string())],
        |handle| async move { load_on(&handle, &body(3, Some(4096)), true).await },
    )
    .await;

    assert_eq!(
        lines.unwrap(),
        [
            "  Endpoint on port 8080, pinned to qwen3",
            "  Server started on port 9301",
        ]
    );
    let lines_taken: Vec<&str> = taken.iter().map(|t| t.line.as_str()).collect();
    assert_eq!(
        lines_taken,
        [
            format!("POST {} HTTP/1.1", paths::PROXY_START_PINNED_PATH),
            format!("POST {} HTTP/1.1", paths::SERVERS_START_PATH),
        ]
    );
    assert_eq!(taken[0].body["model_id"], 3);
    assert_eq!(taken[0].body["options"]["contextLength"], 4096);
    assert!(taken[0].body["proxy"].is_object(), "{}", taken[0].body);
}

/// A refusal is the daemon's sentence, and the model is not asked for after
/// the pin was refused.
#[tokio::test]
async fn a_refused_pin_is_the_daemons_sentence_and_loads_nothing() {
    let refusal = serde_json::json!({
        "error": "the proxy is already running unpinned \u{2014} stop it first (`gglib proxy stop`)"
    });
    let (lines, taken) = against(vec![(409, refusal.to_string())], |handle| async move {
        load_on(&handle, &body(3, None), true).await
    })
    .await;

    let err = format!("{:#}", lines.expect_err("the daemon refused"));
    assert!(err.contains("already running unpinned"), "{err}");
    assert_eq!(taken.len(), 1, "nothing was asked after the refusal");
}

/// An unload is one stop request naming the model, and the line printed is
/// the daemon's sentence; a model that is not loaded is the daemon's 404,
/// in its words.
#[tokio::test]
async fn an_unload_asks_the_daemon_to_stop_the_model_and_prints_its_sentence() {
    let (line, taken) = against(
        vec![(
            200,
            serde_json::json!("Server for model 3 stopped").to_string(),
        )],
        |handle| async move { unload_on(&handle, 3).await },
    )
    .await;
    assert_eq!(line.unwrap(), "  Server for model 3 stopped");
    assert_eq!(
        taken[0].line,
        format!("POST {} HTTP/1.1", paths::SERVERS_STOP_PATH)
    );
    assert_eq!(taken[0].body["model_id"], 3);

    let missing = serde_json::json!({ "error": "server not found: 3" });
    let (line, _) = against(vec![(404, missing.to_string())], |handle| async move {
        unload_on(&handle, 3).await
    })
    .await;
    let err = format!("{:#}", line.expect_err("not loaded"));
    assert!(err.contains("server not found: 3"), "{err}");
}
