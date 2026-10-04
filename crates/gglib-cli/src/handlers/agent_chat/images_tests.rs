//! Attaching files to a turn: the receipt, the refusals by path, and the
//! refusal of a session whose model cannot see.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use gglib_core::domain::attachment::AttachmentBlob;
use gglib_core::ports::AttachmentStore;

use super::*;

/// A store in memory.
#[derive(Default)]
pub(crate) struct MemoryStore {
    pub(super) kept: Mutex<BTreeMap<AttachmentId, AttachmentInfo>>,
}

#[async_trait]
impl AttachmentStore for MemoryStore {
    async fn put(&self, info: &AttachmentInfo, _bytes: &[u8]) -> Result<(), AttachmentError> {
        let kept = info.clone();
        self.kept
            .lock()
            .unwrap()
            .entry(info.id.clone())
            .or_insert(kept);
        Ok(())
    }

    async fn info(&self, id: &AttachmentId) -> Result<Option<AttachmentInfo>, AttachmentError> {
        Ok(self.kept.lock().unwrap().get(id).cloned())
    }

    async fn blob(&self, _id: &AttachmentId) -> Result<Option<AttachmentBlob>, AttachmentError> {
        Ok(None)
    }
}

pub(crate) fn service() -> (AttachmentService, Arc<MemoryStore>) {
    let store = Arc::new(MemoryStore::default());
    (AttachmentService::new(store.clone()), store)
}

/// A PNG's signature and its `IHDR` chunk, for `width` by `height`.
pub(crate) fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend([0, 0, 0, 13]);
    bytes.extend(b"IHDR");
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

/// `bytes` in a file called `name` under `dir`.
pub(crate) fn file(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).expect("the file is written");
    path
}

/// `--image <path>…`: the images, and what went to stderr.
pub(crate) async fn attached<'a>(
    service: &'a AttachmentService,
    paths: &[PathBuf],
    quiet: bool,
) -> Result<(TurnImages<'a>, String)> {
    let mut receipts = Vec::new();
    let images = TurnImages::attach(service, paths, quiet, &mut receipts).await?;
    Ok((images, String::from_utf8(receipts).expect("text")))
}

/// Why `--image <path>` is refused, with nothing stored.
async fn refused(path: &Path) -> String {
    let (service, store) = service();
    let refusal = attached(&service, &[path.to_owned()], false).await;
    assert!(store.kept.lock().unwrap().is_empty());
    refusal.err().expect("the file is refused").to_string()
}

#[tokio::test]
async fn each_image_gets_one_receipt_line_with_its_name_size_and_cost() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = [
        file(&dir, "shot.png", &png(2560, 1440)),
        file(&dir, "icon.png", &png(64, 32)),
    ];
    let (service, _) = service();

    let (_, receipts) = attached(&service, &paths, false).await.unwrap();

    assert_eq!(
        receipts,
        "  image shot.png: 2560x1440, ~3600 tokens\n  image icon.png: 64x32, ~2 tokens\n"
    );
}

#[tokio::test]
async fn quiet_attaches_the_image_and_prints_no_receipt() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = file(&dir, "shot.png", &png(64, 32));
    let (service, _) = service();

    let (mut images, receipts) = attached(&service, &[path], true).await.unwrap();

    assert_eq!(receipts, "");
    assert_eq!(images.take().len(), 1);
}

#[tokio::test]
async fn the_images_go_to_one_message_in_the_order_given_by_the_hash_of_the_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (first, second) = (png(64, 32), png(32, 64));
    let paths = [file(&dir, "a.png", &first), file(&dir, "b.png", &second)];
    let (service, store) = service();

    let (mut images, _) = attached(&service, &paths, false).await.unwrap();

    let ids = images.take();
    assert_eq!(ids, [AttachmentId::of(&first), AttachmentId::of(&second)]);
    assert!(store.kept.lock().unwrap().contains_key(&ids[0]));
    assert!(images.take().is_empty(), "a later message carries none");
}

#[tokio::test]
async fn a_missing_file_is_an_error_that_names_its_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gone.png");

    let refusal = refused(&path).await;

    assert!(refusal.starts_with(&format!("cannot read image '{}': ", path.display())));
}

#[tokio::test]
async fn a_file_that_is_no_png_or_jpeg_is_an_error_that_names_its_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = file(&dir, "notes.txt", b"plain text, whatever it is called");

    let refusal = refused(&path).await;

    assert_eq!(
        refusal,
        format!(
            "image '{}': {}",
            path.display(),
            AttachmentError::Unsupported
        )
    );
    assert!(refusal.contains("PNG and JPEG"));
}

#[tokio::test]
async fn a_file_over_the_cap_is_an_error_that_names_its_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = file(&dir, "huge.png", &png(64, 32));
    let huge = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    huge.set_len(MAX_IMAGE_BYTES as u64 + 1).unwrap();

    let refusal = refused(&path).await;

    assert_eq!(
        refusal,
        format!("image '{}': {}", path.display(), AttachmentError::TooLarge)
    );
}

#[tokio::test]
async fn a_file_at_the_cap_is_attached() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = file(&dir, "large.png", &png(64, 32));
    let large = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    large.set_len(MAX_IMAGE_BYTES as u64).unwrap();
    let (service, _) = service();

    assert!(attached(&service, &[path], true).await.is_ok());
}

#[tokio::test]
async fn a_bad_file_after_a_good_one_fails_the_whole_command() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = [
        file(&dir, "shot.png", &png(64, 32)),
        dir.path().join("gone.png"),
    ];
    let (service, _) = service();

    let refusal = attached(&service, &paths, false).await.err().unwrap();

    assert!(refusal.to_string().contains("gone.png"));
}

/// One user message carrying an image, as a resumed chat's history has it.
fn image_turn() -> AgentMessage {
    AgentMessage::User {
        content: "what is this?".to_owned(),
        images: vec![AttachmentId::of(b"an image sent earlier")],
    }
}

#[tokio::test]
async fn an_attached_image_is_refused_for_a_model_that_cannot_see() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = file(&dir, "shot.png", &png(64, 32));
    let (service, _) = service();
    let (mut images, _) = attached(&service, &[path], true).await.unwrap();

    let refused = images.judge(Sight::catalogue("qwen", false), &[]).await;

    assert!(
        refused
            .expect_err("qwen has no projector")
            .to_string()
            .starts_with("Model 'qwen' cannot read images")
    );
}

#[tokio::test]
async fn an_image_in_the_history_is_refused_for_a_model_that_cannot_see() {
    let (service, _) = service();
    let (mut images, _) = attached(&service, &[], true).await.unwrap();

    let refused = images
        .judge(Sight::catalogue("qwen", false), &[image_turn()])
        .await;

    assert!(refused.is_err());
}

#[tokio::test]
async fn a_session_with_no_image_runs_on_a_model_that_cannot_see() {
    let (service, _) = service();
    let (mut images, _) = attached(&service, &[], true).await.unwrap();
    let text_only = AgentMessage::User {
        content: "hello".to_owned(),
        images: Vec::new(),
    };

    let judged = images
        .judge(Sight::catalogue("qwen", false), &[text_only])
        .await;

    assert!(judged.is_ok());
}
