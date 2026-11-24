//! A daemon call refused for Hugging Face's rate limit, against a stand-in
//! daemon that answers the refusal the daemon writes.

use std::sync::{Arc, Mutex};

use gglib_core::download::DownloadId;
use gglib_core::hub_limit::LimitNotice;

use super::super::QueueDownloadBody;
use super::super::stand_in::against;

/// The daemon's refusal for the Hub's rate limit, lifting in `secs`.
fn hub_limited(secs: u64) -> (u16, String) {
    let body = serde_json::json!({
        "error": format!("Could not queue the download: Hugging Face's rate limit lifts in 0:{secs:02}"),
        "status": 429,
        "type": "hub_rate_limited",
        "metadata": { "bucket": "api", "retry_after_secs": secs },
    });
    (429, body.to_string())
}

fn body() -> QueueDownloadBody {
    QueueDownloadBody {
        model_id: "owner/repo".to_owned(),
        quantization: Some("Q8_0".to_owned()),
    }
}

/// A notice that keeps what it is told, in order.
fn noting() -> (LimitNotice, Arc<Mutex<Vec<Option<String>>>>) {
    let told = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&told);
    let notice: LimitNotice = Arc::new(move |line: Option<&str>| {
        sink.lock().unwrap().push(line.map(str::to_owned));
    });
    (notice, told)
}

/// A queue the Hub's rate limit refused is sent again once it lifts, the
/// wait told in the words every surface shows, and answers the ID.
#[tokio::test]
async fn a_queue_refused_for_the_hubs_limit_is_sent_again() {
    let (notice, told) = noting();
    let replies = vec![
        hub_limited(1),
        (200, r#"{"id":"owner/repo:Q8_0"}"#.to_owned()),
    ];

    let (queued, taken) = against(replies, |handle| async move {
        handle.queue_download(&body(), &notice).await
    })
    .await;

    assert_eq!(
        queued.expect("queued once the limit lifted"),
        DownloadId::new("owner/repo", Some("Q8_0"))
    );
    assert_eq!(taken.len(), 2, "sent once, and once again");
    assert_eq!(taken[0].body, taken[1].body, "the same request");
    let told = told.lock().unwrap().clone();
    assert_eq!(
        told,
        [
            Some("waiting for Hugging Face's rate limit, 0:01".to_owned()),
            None
        ]
    );
}

/// Refused five times in a row, the call gives up with the daemon's words.
#[tokio::test]
async fn a_call_refused_five_times_in_a_row_gives_up() {
    let (notice, _) = noting();
    let replies = vec![hub_limited(0); 6];

    let (queued, taken) = against(replies, |handle| async move {
        handle.queue_download(&body(), &notice).await
    })
    .await;

    let refused = queued.expect_err("the limit kept refusing it");
    assert_eq!(taken.len(), 5);
    assert_eq!(
        refused.to_string(),
        "daemon answered 429 Too Many Requests: Could not queue the download: Hugging \
         Face's rate limit lifts in 0:00"
    );
}

/// A 429 of another type is not the Hub's limit: the call is not sent
/// again.
#[tokio::test]
async fn another_429_is_not_waited_out() {
    let (notice, told) = noting();
    let busy = serde_json::json!({ "error": "busy", "status": 429, "type": "agent_busy" });
    let replies = vec![(429, busy.to_string()), (200, r#"{"id":"x"}"#.to_owned())];

    let (queued, taken) = against(replies, |handle| async move {
        handle.queue_download(&body(), &notice).await
    })
    .await;

    assert!(queued.is_err());
    assert_eq!(taken.len(), 1);
    assert!(told.lock().unwrap().is_empty());
}
