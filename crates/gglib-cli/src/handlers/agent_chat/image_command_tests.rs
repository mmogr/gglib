//! The REPL's `/image` command: what counts as it, what it attaches, what
//! it answers, and the marker a stored image is shown by.

use super::images_tests::{attached, file, png, service};
use super::*;

#[test]
fn the_image_command_takes_the_rest_of_the_line_as_its_path() {
    let attach = |path: &str| Some(ImageCommand::Attach(PathBuf::from(path)));

    assert_eq!(image_command("/image shot.png"), attach("shot.png"));
    assert_eq!(
        image_command("/image   my shots/one two.png  "),
        attach("my shots/one two.png")
    );
    assert_eq!(image_command("/image\t\"a b.png\""), attach("a b.png"));
    assert_eq!(image_command("/image 'a b.png'"), attach("a b.png"));
    assert_eq!(image_command("/image"), Some(ImageCommand::Usage));
    assert_eq!(image_command("/image   "), Some(ImageCommand::Usage));
}

#[test]
fn a_line_that_is_not_the_image_command_is_a_message() {
    for line in [
        "/images shot.png",
        "/imagine a boat",
        "image shot.png",
        "see /image x",
    ] {
        assert_eq!(image_command(line), None, "{line}");
    }
}

/// A REPL session on a model that can see, or cannot.
async fn repl(service: &AttachmentService, image_input: bool) -> TurnImages<'_> {
    let (mut images, _) = attached(service, &[], true).await.unwrap();
    let sight = Sight::catalogue("qwen", image_input);
    images.judge(sight, &[]).await.expect("no image yet");
    images
}

#[tokio::test]
async fn the_image_command_attaches_to_the_next_message_and_says_what_it_costs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bytes = png(640, 320);
    let path = file(&dir, "shot.png", &bytes);
    let (service, _) = service();
    let mut images = repl(&service, true).await;

    let reply = images.command(&format!("/image {}", path.display())).await;

    assert_eq!(
        reply.as_deref(),
        Some("  image shot.png: 640x320, ~200 tokens")
    );
    assert_eq!(images.take(), [AttachmentId::of(&bytes)]);
    assert!(images.take().is_empty());
}

#[tokio::test]
async fn the_image_command_is_refused_and_stores_nothing_for_a_model_that_cannot_see() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = file(&dir, "shot.png", &png(64, 32));
    let (service, store) = service();
    let mut images = repl(&service, false).await;

    let reply = images.command(&format!("/image {}", path.display())).await;

    assert!(
        reply
            .unwrap()
            .starts_with("Model 'qwen' cannot read images")
    );
    assert!(store.kept.lock().unwrap().is_empty());
    assert!(images.take().is_empty());
}

#[tokio::test]
async fn the_image_command_names_a_file_it_cannot_attach_and_attaches_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = file(&dir, "notes.txt", b"plain text");
    let (service, _) = service();
    let mut images = repl(&service, true).await;

    let reply = images.command(&format!("/image {}", path.display())).await;

    assert!(
        reply
            .unwrap()
            .starts_with(&format!("image '{}': ", path.display()))
    );
    assert!(images.take().is_empty());
}

#[tokio::test]
async fn the_image_command_with_no_path_says_how_it_is_used() {
    let (service, _) = service();
    let mut images = repl(&service, true).await;

    assert_eq!(images.command("/image").await.as_deref(), Some(USAGE));
}

#[tokio::test]
async fn any_other_line_is_left_for_the_model() {
    let (service, _) = service();
    let mut images = repl(&service, true).await;

    assert_eq!(images.command("what is in /image?").await, None);
}

#[test]
fn stored_images_are_shown_as_one_marker_each_in_order() {
    let image = |width, height| AttachmentInfo {
        id: AttachmentId::of(&png(width, height)),
        mime: "image/png".to_owned(),
        width,
        height,
    };

    let (wide, small) = (image(2560, 1440), image(64, 32));

    assert_eq!(
        markers(&[wide.clone(), small.clone()]),
        format!(
            " [image 2560x1440 {}] [image 64x32 {}]",
            &wide.id.as_str()[..8],
            &small.id.as_str()[..8]
        )
    );
    assert_eq!(markers(&[]), "");
}
