//! A turn on the paired machine's model, from a chat of this machine's
//! (ADR 0014): the page names its images by this machine's ids, which the
//! far machine does not hold, so this machine's adapter sends each as a
//! data URL read from its own store, with the far machine's key.

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::{AttachmentBlob, AttachmentId, AttachmentInfo};
use gglib_core::ports::{AttachmentError, AttachmentStore, LlmCompletionPort as _};
use gglib_core::retry::RetryPolicy;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

use super::super::{FarMachine, LlmCompletionAdapter};

/// A store holding one image's bytes.
struct One(AttachmentId, AttachmentBlob);

#[async_trait]
impl AttachmentStore for One {
    async fn put(&self, _info: &AttachmentInfo, _bytes: &[u8]) -> Result<(), AttachmentError> {
        unreachable!("the adapter only reads")
    }

    async fn info(&self, _id: &AttachmentId) -> Result<Option<AttachmentInfo>, AttachmentError> {
        unreachable!("the adapter reads bytes, not facts")
    }

    async fn size(&self, _id: &AttachmentId) -> Result<Option<usize>, AttachmentError> {
        unreachable!("the adapter reads bytes, not sizes")
    }

    async fn blob(&self, id: &AttachmentId) -> Result<Option<AttachmentBlob>, AttachmentError> {
        Ok((*id == self.0).then(|| self.1.clone()))
    }

    async fn ids_starting_with(&self, prefix: &str) -> Result<Vec<AttachmentId>, AttachmentError> {
        Ok(gglib_core::ports::attachment_store::ids_starting_with(
            [&self.0],
            prefix,
        ))
    }
}

/// The end of a request's head, and where its body starts.
pub(super) fn head_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|at| at + 4)
}

/// Accept one request, read it whole (head, then body), answer an empty
/// stream, and give back what was read.
pub(super) async fn capture_one(listener: TcpListener) -> String {
    let (request, mut socket) = gglib_test_http::accept(&listener).await.unwrap();
    let answer = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: 14\r\nconnection: close\r\n\r\ndata: [DONE]\n\n";
    socket.write_all(answer.as_bytes()).await.unwrap();
    let _ = socket.shutdown().await;
    request.head + &String::from_utf8(request.body).unwrap()
}

#[tokio::test]
async fn a_turn_for_the_paired_machines_model_sends_data_urls_not_ids() {
    let bytes = vec![0x89, b'P', b'N', b'G', 7, 7, 7, 7];
    let id = AttachmentId::of(&bytes);
    let blob = AttachmentBlob {
        mime: "image/png".to_owned(),
        data: bytes,
    };
    let store: Arc<dyn AttachmentStore> = Arc::new(One(id.clone(), blob));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let far = FarMachine {
        key: "far-key".to_owned(),
        name: "desk".to_owned(),
    };
    let adapter = LlmCompletionAdapter::new(base, None)
        .with_far_machine(Some(far))
        .with_attachments(Some(store))
        .with_retry_policy(RetryPolicy {
            max_attempts: 1,
            ..RetryPolicy::default()
        });
    let messages = [AgentMessage::User {
        content: "what is this?".to_owned(),
        images: vec![id.clone()],
    }];

    let (sent, answered) = tokio::join!(capture_one(listener), async {
        adapter.chat_stream(&messages, &[]).await.map(|_| ())
    });

    answered.unwrap();
    let start = head_end(sent.as_bytes()).unwrap();
    let (head, body) = sent.split_at(start);
    assert!(
        head.to_ascii_lowercase()
            .contains("authorization: bearer far-key")
    );
    let body: serde_json::Value = serde_json::from_str(body).unwrap();
    let parts = &body["messages"][0]["content"];
    assert_eq!(parts[1]["type"], "image_url");
    assert_eq!(
        parts[1]["image_url"]["url"],
        "data:image/png;base64,iVBORwcHBwc="
    );
    assert!(!sent.contains(id.as_str()), "the id crossed: {sent}");
}
