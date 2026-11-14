//! What a far hub says of itself decides which of gglib's own calls are made
//! to it: one that does not say is asked to update, one that does not serve
//! a feature has its calls refused, and a kept answer that serves the call
//! is not read again.

use std::sync::{Arc, Mutex};

use gglib_proxy::models::{Feature, GglibInfo, THIS_VERSION};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

use super::FarProxy;
use crate::error::GuiError;
use crate::remote::far_proxy::FarError;
use crate::remote::paired_machine::FarCredentials;

/// A list from a gglib from before devices said their version.
const OLDER: &str = r#"{"object":"list","data":[],"machine_name":"desk"}"#;

/// A list from a gglib that serves runs, and a feature this build does not
/// know, but not chats.
const NO_CHATS: &str = r#"{"object":"list","data":[],"gglib":{"version":"9.9.9","features":["runs","status","later"]}}"#;

/// A fake far hub: its `/v1/models` answers what `list` holds when it is
/// asked, every other route `[]`. Every request line it read is in `seen`.
struct Hub {
    far: FarProxy,
    list: Arc<Mutex<&'static str>>,
    seen: Arc<Mutex<Vec<String>>>,
}

async fn hub(list: &'static str) -> Hub {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (answer, seen) = (Arc::new(Mutex::new(list)), Arc::new(Mutex::new(Vec::new())));
    let (answering, seeing) = (Arc::clone(&answer), Arc::clone(&seen));
    tokio::spawn(async move {
        while let Ok(incoming) = gglib_test_http::incoming(&listener).await {
            let Ok((request, mut socket)) = incoming.request().await else {
                continue;
            };
            let line = request.line().to_owned();
            let body = if line.starts_with("GET /v1/models ") {
                *answering.lock().unwrap()
            } else {
                "[]"
            };
            seeing.lock().unwrap().push(line);
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
                 connection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });
    let credentials = FarCredentials {
        key: "sk-the-key".to_owned(),
        fingerprint: "0a1b2c3d4e5f".to_owned(),
        name: Some("desk".to_owned()),
    };
    Hub {
        far: FarProxy::new(&format!("http://127.0.0.1:{port}/v1"), &credentials).unwrap(),
        list: answer,
        seen,
    }
}

impl Hub {
    /// The paths asked, in order.
    fn asked(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter_map(|line| line.split(' ').nth(1).map(str::to_owned))
            .collect()
    }
}

fn conflict(error: GuiError) -> String {
    match error {
        GuiError::Conflict(message) => message,
        other => panic!("not a conflict: {other:?}"),
    }
}

/// A hub whose list has no `gglib` object is asked to update, by name and
/// with this build's version, and none of gglib's own routes is asked.
#[tokio::test]
async fn a_hub_that_does_not_say_its_version_is_asked_to_update() {
    let hub = hub(OLDER).await;

    let chats = conflict(hub.far.list_chats().await.unwrap_err());
    let Err(FarError::Failed(lookup)) = hub.far.lookup("qwen").await else {
        panic!("the lookup was not refused here");
    };
    let Err(FarError::Failed(models)) = hub.far.models().await else {
        panic!("the list was not refused here");
    };

    let update = format!(
        "desk runs a gglib older than this machine's {THIS_VERSION} — update gglib on desk"
    );
    assert_eq!(chats, update);
    assert_eq!(conflict(lookup), update);
    assert_eq!(conflict(models), update);
    assert!(
        hub.asked().iter().all(|path| path == "/v1/models"),
        "{:?}",
        hub.asked()
    );
}

/// A hub that does not list a feature has the calls that need it refused in
/// a plain sentence; the features it lists, and one this build does not
/// know, leave the rest alone, without a second read.
#[tokio::test]
async fn a_hub_without_a_feature_refuses_only_its_calls() {
    let hub = hub(NO_CHATS).await;

    let chats = conflict(hub.far.list_chats().await.unwrap_err());
    let turn = gglib_core::domain::hub_chats::HubTurn {
        conversation_id: 7,
        content: "carry on".to_owned(),
        images: Vec::new(),
        thinking: None,
        answer_saved: false,
        draw: false,
        image_model: None,
    };
    let added = conflict(hub.far.add_turn("r1", &turn).await.unwrap_err());
    let runs = hub.far.list_runs().await.unwrap();

    let refused = "desk, on gglib 9.9.9, does not share its chats with this machine";
    assert_eq!(chats, refused);
    assert_eq!(added, refused);
    assert!(runs.status().is_success());
    assert_eq!(hub.asked(), ["/v1/models", "/v1/models", "/v1/gglib/runs"]);
}

/// A kept answer that serves the call lets it go unasked.
#[tokio::test]
async fn a_kept_answer_that_serves_the_call_is_not_read_again() {
    let hub = hub(OLDER).await;
    let far = hub.far.clone().knowing(GglibInfo::this_build(Feature::ALL));

    far.list_chats().await.unwrap();
    far.list_runs().await.unwrap();

    assert_eq!(hub.asked(), ["/v1/gglib/chats", "/v1/gglib/runs"]);
}

/// A hub updated while this machine stayed connected is served at once: an
/// answer that refused is never kept as the last word.
#[tokio::test]
async fn a_hub_updated_while_connected_is_served_at_once() {
    let hub = hub(OLDER).await;
    hub.far.list_chats().await.unwrap_err();

    *hub.list.lock().unwrap() =
        r#"{"object":"list","data":[],"gglib":{"version":"9.9.9","features":["chats"]}}"#;
    hub.far.list_chats().await.unwrap();
    hub.far.open_chat(7).await.unwrap();

    assert_eq!(
        hub.asked(),
        [
            "/v1/models",
            "/v1/models",
            "/v1/gglib/chats",
            "/v1/gglib/chats/7"
        ]
    );
}
