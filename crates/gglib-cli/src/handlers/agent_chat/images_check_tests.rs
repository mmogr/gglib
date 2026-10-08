//! The daemon's check before a run, run by the CLI before a session's model
//! is asked for anything: an image the session carries that is not stored,
//! and images over the cap together, history and first message alike.

use gglib_core::request_pipeline::MAX_REQUEST_IMAGE_BYTES;

use super::images_tests::{attached, file, image_turn, keep_earlier, png, service};
use super::*;
use crate::handlers::agent_chat::sight::sight_tests::props_server;

#[tokio::test]
async fn a_history_that_names_an_image_not_stored_is_refused_before_the_server_is_asked() {
    let (service, _) = service();
    let (mut images, _) = attached(&service, &[], true).await.unwrap();
    // A server that cannot see: it would refuse the image, were it asked.
    let server = props_server(r#"{"modalities":{"vision":false}}"#);
    let sight = Sight::server("qwen", reqwest::Client::new(), server.port);

    let refused = images.judge(sight, &[image_turn()]).await;

    let missing = AttachmentError::NotFound(AttachmentId::of(b"an image sent earlier"));
    assert_eq!(
        refused.expect_err("the image is gone").to_string(),
        missing.to_string()
    );
    assert!(server.requests().is_empty());
}

/// The history's image and the one waiting for the first message are
/// counted together: at the cap the session goes on to ask the server what
/// it sees, and a byte over it is refused first.
#[tokio::test]
async fn images_over_the_cap_together_are_refused_before_the_server_is_asked() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bytes = png(64, 32);
    let path = file(&dir, "shot.png", &bytes);
    let at_the_cap = MAX_REQUEST_IMAGE_BYTES - bytes.len();
    for (earlier, fits) in [(at_the_cap, true), (at_the_cap + 1, false)] {
        let (service, store) = service();
        keep_earlier(&store, earlier);
        let paths = std::slice::from_ref(&path);
        let (mut images, _) = attached(&service, paths, true).await.unwrap();
        let server = props_server(r#"{"modalities":{"vision":true}}"#);
        let sight = Sight::server("qwen", reqwest::Client::new(), server.port);

        let judged = images.judge(sight, &[image_turn()]).await;

        if fits {
            judged.expect("at the cap");
            assert_eq!(server.requests(), ["GET /props HTTP/1.1"]);
        } else {
            assert_eq!(
                judged.expect_err("over the cap").to_string(),
                AttachmentError::RequestTooLarge.to_string()
            );
            assert!(server.requests().is_empty());
        }
    }
}

/// A history under the cap, every image stored, on a model that sees.
#[tokio::test]
async fn a_history_whose_images_are_stored_and_under_the_cap_is_accepted() {
    let (service, store) = service();
    keep_earlier(&store, 3 << 20);
    let (mut images, _) = attached(&service, &[], true).await.unwrap();
    let turns = [image_turn(), image_turn()];

    assert!(
        images
            .judge(Sight::catalogue("qwen", true), &turns)
            .await
            .is_ok()
    );
}
