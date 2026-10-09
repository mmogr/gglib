//! What a tool's content items become: text the model reads, images kept,
//! and a sentence for each image instead of its bytes.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use gglib_core::domain::{AttachmentBlob, AttachmentId, AttachmentInfo};
use gglib_core::ports::{AttachmentError, AttachmentStore};
use gglib_core::request_pipeline::MAX_IMAGE_BYTES;
use gglib_core::services::AttachmentService;
use serde_json::{Value, json};

use super::split_content;

/// A real 1x1 PNG, as an MCP server would send it.
pub(crate) const ONE_PIXEL_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

/// A store in memory, which keeps what it is given.
#[derive(Default)]
pub(crate) struct Kept(Mutex<BTreeMap<AttachmentId, Vec<u8>>>);

impl Kept {
    /// The ids it holds.
    pub(crate) fn ids(&self) -> Vec<AttachmentId> {
        self.0.lock().unwrap().keys().cloned().collect()
    }
}

#[async_trait]
impl AttachmentStore for Kept {
    async fn put(&self, info: &AttachmentInfo, bytes: &[u8]) -> Result<(), AttachmentError> {
        self.0
            .lock()
            .unwrap()
            .insert(info.id.clone(), bytes.to_vec());
        Ok(())
    }

    async fn info(&self, _id: &AttachmentId) -> Result<Option<AttachmentInfo>, AttachmentError> {
        unreachable!("a tool's images are only stored")
    }

    async fn size(&self, _id: &AttachmentId) -> Result<Option<usize>, AttachmentError> {
        unreachable!("a tool's images are only stored")
    }

    async fn blob(&self, _id: &AttachmentId) -> Result<Option<AttachmentBlob>, AttachmentError> {
        unreachable!("a tool's images are only stored")
    }
}

/// An attachment service over an empty [`Kept`], and the store.
pub(crate) fn images() -> (Arc<AttachmentService>, Arc<Kept>) {
    let kept = Arc::new(Kept::default());
    let service = Arc::new(AttachmentService::new(Arc::clone(&kept) as _));
    (service, kept)
}

/// A PNG's signature and `IHDR` for `width` by `height`, then `padding`
/// zero bytes.
fn png(width: u32, height: u32, padding: usize) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend([0, 0, 0, 13]);
    bytes.extend(b"IHDR");
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes.resize(bytes.len() + padding, 0);
    bytes
}

fn image_item(data: &str) -> Value {
    json!({ "type": "image", "data": data, "mimeType": "image/png" })
}

#[tokio::test]
async fn text_and_an_image_are_the_text_and_a_sentence_and_the_image_is_kept() {
    let (images, kept) = images();
    let bytes = png(1024, 768, 0);
    let data = STANDARD.encode(&bytes);
    let content = json!([
        { "type": "text", "text": "Here it is." },
        image_item(&data),
        { "type": "text", "text": "Anything else?" },
    ]);

    let got = split_content(&content, &images).await;

    assert_eq!(
        got.text,
        "Here it is.\n[image 1024x768 PNG stored]\nAnything else?"
    );
    let id = AttachmentId::of(&bytes);
    assert_eq!(
        got.images,
        [AttachmentInfo {
            id: id.clone(),
            mime: "image/png".to_owned(),
            width: 1024,
            height: 768,
        }]
    );
    assert_eq!(kept.ids(), std::slice::from_ref(&id));
    assert_eq!(kept.0.lock().unwrap()[&id], bytes);
    assert!(!got.text.contains(&data));
}

/// An image alone is named, never sent: the text has no base64 in it.
#[tokio::test]
async fn an_image_alone_is_named_and_its_bytes_are_not_in_the_text() {
    let (images, kept) = images();

    let got = split_content(&json!([image_item(ONE_PIXEL_PNG)]), &images).await;

    assert_eq!(got.text, "[image 1x1 PNG stored]");
    assert!(!got.text.contains("data"));
    assert!(!got.text.contains(&ONE_PIXEL_PNG[..16]));
    assert_eq!(got.images.len(), 1);
    assert_eq!(kept.ids(), [got.images[0].id.clone()]);
}

/// The sentence for a refused image, and that nothing was stored.
async fn refused(item: Value) -> String {
    let (images, kept) = images();
    let got = split_content(&json!([item]), &images).await;
    assert!(got.images.is_empty());
    assert!(kept.ids().is_empty(), "a refused image is not stored");
    got.text
}

#[tokio::test]
async fn a_webp_is_refused_in_a_sentence_and_not_stored() {
    let webp = STANDARD.encode(b"RIFF\x24\x00\x00\x00WEBPVP8 ");
    let text = refused(json!({ "type": "image", "data": webp, "mimeType": "image/webp" })).await;
    assert_eq!(
        text,
        "[image not stored: The file is not an image that can be attached: only PNG and JPEG are read.]"
    );
    assert!(!text.contains(&webp));
}

#[tokio::test]
async fn an_image_over_the_limit_is_refused_in_a_sentence_and_not_stored() {
    let big = STANDARD.encode(png(4096, 4096, MAX_IMAGE_BYTES));
    let text = refused(image_item(&big)).await;
    assert_eq!(
        text,
        format!("[image not stored: {}]", AttachmentError::TooLarge)
    );
    assert!(text.contains("MiB"), "{text}");
}

#[tokio::test]
async fn data_that_is_not_base64_or_missing_is_refused_in_a_sentence() {
    let not_base64 = refused(image_item("not base64 at all!")).await;
    assert_eq!(not_base64, "[image not stored: The data is not base64.]");
    let missing = refused(json!({ "type": "image", "mimeType": "image/png" })).await;
    assert_eq!(missing, not_base64);
}

/// A store that fails every write.
struct Broken;

#[async_trait]
impl AttachmentStore for Broken {
    async fn put(&self, _info: &AttachmentInfo, _bytes: &[u8]) -> Result<(), AttachmentError> {
        Err(AttachmentError::Storage("disk full".to_owned()))
    }

    async fn info(&self, _id: &AttachmentId) -> Result<Option<AttachmentInfo>, AttachmentError> {
        unreachable!("a tool's images are only stored")
    }

    async fn size(&self, _id: &AttachmentId) -> Result<Option<usize>, AttachmentError> {
        unreachable!("a tool's images are only stored")
    }

    async fn blob(&self, _id: &AttachmentId) -> Result<Option<AttachmentBlob>, AttachmentError> {
        unreachable!("a tool's images are only stored")
    }
}

/// A store fault is a sentence for the model too, with no image listed.
#[tokio::test]
async fn a_store_fault_is_a_sentence_and_lists_no_image() {
    let images = AttachmentService::new(Arc::new(Broken));
    let got = split_content(&json!([image_item(ONE_PIXEL_PNG)]), &images).await;
    assert_eq!(
        got.text,
        "[image not stored: Attachment storage error: disk full]"
    );
    assert!(got.images.is_empty());
}

/// Text alone is joined as it always was, and nothing is stored.
#[tokio::test]
async fn text_alone_is_joined_with_newlines_and_a_string_is_itself() {
    let (images, kept) = images();

    let got = split_content(
        &json!([
            { "type": "text", "text": "line 1" },
            { "type": "text", "text": "line 2" },
        ]),
        &images,
    )
    .await;
    assert_eq!(got.text, "line 1\nline 2");
    assert!(got.images.is_empty());

    let got = split_content(&json!("already a string"), &images).await;
    assert_eq!(got.text, "already a string");
    assert!(kept.ids().is_empty());
}

/// A result with neither text nor an image is described by what it held,
/// never dumped as JSON: an embedded blob stays out of the text.
#[tokio::test]
async fn content_with_no_text_or_image_is_described_not_dumped() {
    let (images, kept) = images();
    let content = json!([
        { "type": "resource", "resource": {
            "uri": "file:///tmp/cat.png", "mimeType": "image/png", "blob": ONE_PIXEL_PNG
        } },
        { "type": "resource", "resource": { "uri": "file:///notes.md", "text": "# Notes" } },
        { "type": "resource_link", "uri": "file:///big.bin", "name": "big" },
        { "type": "audio", "data": ONE_PIXEL_PNG, "mimeType": "audio/wav" },
        { "unexpected": "shape" },
    ]);

    let got = split_content(&content, &images).await;

    assert_eq!(
        got.text,
        "[resource file:///tmp/cat.png]\n[resource file:///notes.md]\n# Notes\n\
         [resource link file:///big.bin]\n[audio content not shown]\n[content not shown]"
    );
    assert!(!got.text.contains(&ONE_PIXEL_PNG[..16]));
    assert!(got.images.is_empty() && kept.ids().is_empty());

    let object = split_content(&json!({ "unexpected": "shape" }), &images).await;
    assert_eq!(object.text, "[content not shown]");
    let scalars = split_content(&json!([42, true, null, "c2VjcmV0"]), &images).await;
    assert_eq!(
        scalars.text, "42\ntrue\nnull\n[content not shown]",
        "a string item could be base64, so only it is not shown"
    );
    let number = split_content(&json!(1.5), &images).await;
    assert_eq!(number.text, "1.5");
    let empty = split_content(&json!([]), &images).await;
    assert_eq!(empty.text, "[no content]");
}
