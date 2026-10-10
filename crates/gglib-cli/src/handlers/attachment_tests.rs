//! Finding a stored image by the start of its id, and writing it without
//! replacing a file by accident.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::domain::attachment::{AttachmentBlob, AttachmentInfo};
use gglib_core::ports::attachment_store::{AttachmentError, AttachmentStore};

use super::*;

/// A store of images by id; the ids need not be the hash of the bytes, so
/// two can share a start.
struct Images(BTreeMap<AttachmentId, Vec<u8>>);

#[async_trait]
impl AttachmentStore for Images {
    async fn put(&self, _: &AttachmentInfo, _: &[u8]) -> Result<(), AttachmentError> {
        unreachable!("saving reads the store")
    }

    async fn info(&self, _: &AttachmentId) -> Result<Option<AttachmentInfo>, AttachmentError> {
        unreachable!("saving reads the bytes")
    }

    async fn size(&self, _: &AttachmentId) -> Result<Option<usize>, AttachmentError> {
        unreachable!("saving reads the bytes")
    }

    async fn blob(&self, id: &AttachmentId) -> Result<Option<AttachmentBlob>, AttachmentError> {
        Ok(self.0.get(id).map(|data| AttachmentBlob {
            mime: "image/png".to_owned(),
            data: data.clone(),
        }))
    }

    async fn ids_starting_with(&self, prefix: &str) -> Result<Vec<AttachmentId>, AttachmentError> {
        Ok(gglib_core::ports::attachment_store::ids_starting_with(
            self.0.keys(),
            prefix,
        ))
    }
}

/// `start` padded with `fill` to a whole id.
fn id(start: &str, fill: char) -> AttachmentId {
    AttachmentId::parse(&format!(
        "{start}{}",
        fill.to_string().repeat(64 - start.len())
    ))
    .unwrap()
}

/// Three images: two whose ids share `3f9a2c1e`, one alone at `0badcafe`.
fn service() -> AttachmentService {
    let images = [
        (id("3f9a2c1e", '0'), b"first".to_vec()),
        (id("3f9a2c1e", '1'), b"second".to_vec()),
        (id("0badcafe", '2'), b"alone".to_vec()),
    ];
    AttachmentService::new(Arc::new(Images(images.into_iter().collect())))
}

#[tokio::test]
async fn a_unique_start_finds_its_image() {
    let service = service();
    assert_eq!(
        resolve(&service, "0badcafe").await.unwrap(),
        id("0badcafe", '2')
    );
    assert_eq!(
        resolve(&service, "3F9A2C1E1").await.unwrap(),
        id("3f9a2c1e", '1'),
        "more of the id, in capitals, picks one of two"
    );
    let whole = id("3f9a2c1e", '0');
    assert_eq!(resolve(&service, whole.as_str()).await.unwrap(), whole);
}

#[tokio::test]
async fn a_start_two_images_share_is_refused() {
    let err = resolve(&service(), "3f9a2c1e").await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "2 stored images have ids starting 3f9a2c1e; give more of the id."
    );
}

#[tokio::test]
async fn a_start_no_image_has_is_refused() {
    let err = resolve(&service(), "deadbeef").await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "No stored image has an id starting deadbeef."
    );
}

#[tokio::test]
async fn too_little_of_an_id_or_not_hex_is_refused() {
    for typed in ["3f9a2c1", "3f9a2c1g", ""] {
        let err = resolve(&service(), typed).await.unwrap_err();
        assert!(
            err.to_string().contains("is not an image id"),
            "{typed}: {err}"
        );
    }
}

#[tokio::test]
async fn an_image_is_saved_by_its_short_id_and_never_over_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let service = service();

    let saved = save(&service, "0badcafe", Some(dir.path()), false)
        .await
        .unwrap();
    assert_eq!(saved, dir.path().join("0badcafe.png"));
    assert_eq!(std::fs::read(&saved).unwrap(), b"alone");

    let named = dir.path().join("mine.png");
    std::fs::write(&named, b"keep me").unwrap();
    let err = save(&service, "0badcafe", Some(&named), false)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("already exists; pass --force"),
        "{err}"
    );
    assert_eq!(std::fs::read(&named).unwrap(), b"keep me");

    save(&service, "0badcafe", Some(&named), true)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&named).unwrap(), b"alone");
}
