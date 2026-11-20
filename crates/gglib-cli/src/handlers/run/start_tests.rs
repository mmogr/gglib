//! What `gglib run start` sends, and the files `--image` refuses before it
//! sends anything.

use std::path::{Path, PathBuf};

use gglib_core::domain::attachment::AttachmentBlob;
use gglib_core::ports::attachment_store::AttachmentError;
use gglib_core::request_pipeline::{MAX_IMAGE_BYTES, MAX_REQUEST_IMAGE_BYTES};
use serde_json::json;

use super::{read_images, request};
use crate::handlers::agent_chat::images::images_tests::{file, png};

/// `name` under `dir`, a PNG's header stretched to `len` bytes.
fn png_of(dir: &tempfile::TempDir, name: &str, len: usize) -> PathBuf {
    let path = file(dir, name, &png(64, 32));
    let stretched = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    stretched.set_len(len as u64).unwrap();
    path
}

/// Why `--image <path>…` is refused.
fn refused(paths: &[&Path]) -> String {
    let paths: Vec<PathBuf> = paths.iter().map(|path| path.to_path_buf()).collect();
    read_images(&paths)
        .expect_err("the files are refused")
        .to_string()
}

#[test]
fn a_prompt_alone_is_sent_as_the_text_it_is() {
    assert_eq!(
        request("qwen", "why is the sky blue", &[]),
        json!({
            "model": "qwen",
            "messages": [{ "role": "user", "content": "why is the sky blue" }],
        })
    );
}

#[test]
fn images_follow_the_prompt_as_data_urls_in_the_order_given() {
    let dir = tempfile::tempdir().expect("tempdir");
    let jpeg = [
        0xFF, 0xD8, 0xFF, 0xC0, 0, 11, 8, 0, 32, 0, 64, 1, 1, 0x11, 0,
    ];
    let paths = [
        file(&dir, "b.png", &png(64, 32)),
        file(&dir, "a.jpg", &jpeg),
    ];

    let images = read_images(&paths).expect("both are images");
    let body = request("qwen", "what are these?", &images);

    let url = |mime: &str, bytes: &[u8]| {
        AttachmentBlob {
            mime: mime.to_owned(),
            data: bytes.to_vec(),
        }
        .data_url()
    };
    assert_eq!(
        body["messages"],
        json!([{
            "role": "user",
            "content": [
                { "type": "text", "text": "what are these?" },
                { "type": "image_url", "image_url": { "url": url("image/png", &png(64, 32)) } },
                { "type": "image_url", "image_url": { "url": url("image/jpeg", &jpeg) } },
            ],
        }])
    );
    let first = body["messages"][0]["content"][1]["image_url"]["url"]
        .as_str()
        .expect("a url");
    assert!(
        first.starts_with("data:image/png;base64,iVBORw0KGgo"),
        "{first}"
    );
}

#[test]
fn a_file_that_is_not_there_is_refused_by_its_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gone.png");

    let refusal = refused(&[&path]);

    assert!(
        refusal.starts_with(&format!("cannot read image '{}': ", path.display())),
        "{refusal}"
    );
}

#[test]
fn a_file_that_is_no_png_or_jpeg_is_refused_by_its_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let text = file(&dir, "notes.png", b"plain text, whatever it is called");
    // A PNG's first bytes and no size to read.
    let cut = file(&dir, "cut.png", &png(64, 32)[..12]);

    for path in [&text, &cut] {
        assert_eq!(
            refused(&[path]),
            format!(
                "image '{}': {}",
                path.display(),
                AttachmentError::Unsupported
            )
        );
    }
}

#[test]
fn a_file_over_the_cap_is_refused_by_its_path_and_one_at_it_is_sent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let over = png_of(&dir, "huge.png", MAX_IMAGE_BYTES + 1);
    let at = png_of(&dir, "large.png", MAX_IMAGE_BYTES);

    assert_eq!(
        refused(&[&over]),
        format!("image '{}': {}", over.display(), AttachmentError::TooLarge)
    );
    assert_eq!(read_images(&[at]).expect("at the cap").len(), 1);
}

/// Two files at the cap are the most a request carries; the file that takes
/// them over it is the one named.
#[test]
fn files_over_the_requests_total_are_refused_by_the_one_that_goes_over() {
    assert_eq!(MAX_REQUEST_IMAGE_BYTES, 2 * MAX_IMAGE_BYTES);
    let dir = tempfile::tempdir().expect("tempdir");
    let first = png_of(&dir, "one.png", MAX_IMAGE_BYTES);
    let second = png_of(&dir, "two.png", MAX_IMAGE_BYTES);
    let third = file(&dir, "three.png", &png(64, 32));

    assert_eq!(
        read_images(&[first.clone(), second.clone()])
            .expect("at the total")
            .len(),
        2
    );
    let refusal = refused(&[&first, &second, &third]);
    assert!(
        refusal.starts_with(&format!(
            "image '{}': the images are more than 16 MiB together",
            third.display()
        )),
        "{refusal}"
    );
}
