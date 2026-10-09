//! Ingest: what is taken in as an image, what is refused, and by which code.

use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;

use super::*;
use crate::request_pipeline::image_fixtures::{jpeg, png};

/// A store in memory, which counts the times it is asked to keep something.
#[derive(Default)]
struct MemoryStore {
    kept: Mutex<BTreeMap<AttachmentId, (AttachmentInfo, Vec<u8>)>>,
    puts: Mutex<usize>,
}

#[async_trait]
impl AttachmentStore for MemoryStore {
    async fn put(&self, info: &AttachmentInfo, bytes: &[u8]) -> Result<(), AttachmentError> {
        *self.puts.lock().unwrap() += 1;
        self.kept
            .lock()
            .unwrap()
            .entry(info.id.clone())
            .or_insert_with(|| (info.clone(), bytes.to_vec()));
        Ok(())
    }

    async fn info(&self, id: &AttachmentId) -> Result<Option<AttachmentInfo>, AttachmentError> {
        Ok(self.kept.lock().unwrap().get(id).map(|kept| kept.0.clone()))
    }

    async fn size(&self, id: &AttachmentId) -> Result<Option<usize>, AttachmentError> {
        Ok(self.kept.lock().unwrap().get(id).map(|kept| kept.1.len()))
    }

    async fn blob(&self, id: &AttachmentId) -> Result<Option<AttachmentBlob>, AttachmentError> {
        Ok(self
            .kept
            .lock()
            .unwrap()
            .get(id)
            .map(|kept| AttachmentBlob {
                mime: kept.0.mime.clone(),
                data: kept.1.clone(),
            }))
    }

    async fn ids_starting_with(&self, prefix: &str) -> Result<Vec<AttachmentId>, AttachmentError> {
        let kept = self.kept.lock().unwrap();
        Ok(crate::ports::attachment_store::ids_starting_with(
            kept.keys(),
            prefix,
        ))
    }
}

fn service() -> (AttachmentService, Arc<MemoryStore>) {
    let store = Arc::new(MemoryStore::default());
    (AttachmentService::new(store.clone()), store)
}

/// The refusal of `bytes`, by its code, with nothing kept.
async fn refused(bytes: &[u8]) -> &'static str {
    let (service, store) = service();
    let refusal = service.ingest(bytes).await.unwrap_err();
    assert!(store.kept.lock().unwrap().is_empty());
    assert_eq!(*store.puts.lock().unwrap(), 0);
    refusal.code().expect("a refusal has a code")
}

#[tokio::test]
async fn a_png_is_stored_under_the_sha256_of_its_bytes_with_its_size_and_cost() {
    let (service, store) = service();
    let bytes = png(2560, 1440);
    let upload = service.ingest(&bytes).await.unwrap();
    assert_eq!(upload.info.id, AttachmentId::of(&bytes));
    assert_eq!(upload.info.mime, "image/png");
    assert_eq!((upload.info.width, upload.info.height), (2560, 1440));
    assert_eq!(upload.image_tokens, 3600);
    // The bytes are kept as they were sent.
    assert_eq!(store.kept.lock().unwrap()[&upload.info.id].1, bytes);
}

#[tokio::test]
async fn a_jpeg_is_stored_as_a_jpeg() {
    let (service, _) = service();
    let bytes = jpeg(980, 460);
    let upload = service.ingest(&bytes).await.unwrap();
    assert_eq!(upload.info.id, AttachmentId::of(&bytes));
    assert_eq!(upload.info.mime, "image/jpeg");
    assert_eq!((upload.info.width, upload.info.height), (980, 460));
    assert_eq!(upload.image_tokens, 465);
}

#[tokio::test]
async fn the_same_bytes_twice_are_one_image_and_the_same_answer() {
    let (service, store) = service();
    let bytes = png(64, 64);
    let first = service.ingest(&bytes).await.unwrap();
    let second = service.ingest(&bytes).await.unwrap();
    assert_eq!(first, second);
    assert_eq!(store.kept.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn anything_but_a_png_or_a_jpeg_is_refused_by_code() {
    let mut webp = b"RIFF\x24\0\0\0WEBPVP8 ".to_vec();
    webp.resize(64, 0);
    for bytes in [
        b"GIF89a\x01\0\x01\0\x80\0\0".as_slice(),
        &webp,
        b"not an image at all",
        b"<svg xmlns='http://www.w3.org/2000/svg'/>",
        b"",
    ] {
        assert_eq!(refused(bytes).await, "unsupported_image");
    }
}

#[tokio::test]
async fn the_refusal_of_a_format_names_the_two_that_are_read() {
    let (service, _) = service();
    let message = service.ingest(b"GIF89a").await.unwrap_err().to_string();
    assert!(
        message.contains("PNG") && message.contains("JPEG"),
        "{message}"
    );
}

#[tokio::test]
async fn an_image_whose_size_cannot_be_read_is_refused() {
    // A PNG cut short before its size, one with no `IHDR`, one of no width,
    // and a JPEG that reaches its scan with no frame header.
    let whole = png(640, 480);
    let mut no_ihdr = whole.clone();
    no_ihdr[12..16].copy_from_slice(b"tEXt");
    let end_of_image = [0xFF, 0xD8, 0xFF, 0xD9];
    for bytes in [
        &whole[..8],
        &whole[..20],
        no_ihdr.as_slice(),
        png(0, 480).as_slice(),
        end_of_image.as_slice(),
    ] {
        assert_eq!(refused(bytes).await, "unsupported_image");
    }
}

#[tokio::test]
async fn an_image_over_the_cap_is_refused_by_code_and_one_at_it_is_stored() {
    let mut bytes = png(64, 64);
    bytes.resize(MAX_IMAGE_BYTES, 0xA5);
    let (service, _) = service();
    assert_eq!(service.ingest(&bytes).await.unwrap().info.width, 64);

    bytes.push(0xA5);
    assert_eq!(refused(&bytes).await, "image_too_large");
    assert_eq!(MAX_IMAGE_BYTES, 8 * 1024 * 1024);
}

#[tokio::test]
async fn an_id_the_store_lacks_is_the_not_found_refusal() {
    let (service, _) = service();
    let id = AttachmentId::of(b"never uploaded");
    for refusal in [
        service.info(&id).await.unwrap_err(),
        service.blob(&id).await.unwrap_err(),
    ] {
        assert!(matches!(&refusal, AttachmentError::NotFound(missing) if *missing == id));
        assert_eq!(refusal.code(), Some("attachment_not_found"));
    }
}

#[tokio::test]
async fn a_stored_image_is_read_back_as_it_was_sent() {
    let (service, _) = service();
    let bytes = jpeg(800, 600);
    let id = service.ingest(&bytes).await.unwrap().info.id;
    assert_eq!(service.info(&id).await.unwrap().mime, "image/jpeg");
    let blob = service.blob(&id).await.unwrap();
    assert_eq!((blob.mime.as_str(), blob.data), ("image/jpeg", bytes));
}

#[test]
fn each_refusal_has_its_code_and_status_and_a_store_failure_has_no_code() {
    let id = AttachmentId::of(b"x");
    assert_eq!(AttachmentError::TooLarge.code(), Some("image_too_large"));
    assert_eq!(
        AttachmentError::Unsupported.code(),
        Some("unsupported_image")
    );
    assert_eq!(
        AttachmentError::NotFound(id).code(),
        Some("attachment_not_found")
    );
    assert_eq!(
        AttachmentError::RequestTooLarge.code(),
        Some("request_images_too_large")
    );
    assert_eq!(AttachmentError::Storage("disk".into()).code(), None);
    let statuses = [
        (AttachmentError::TooLarge, 413),
        (AttachmentError::Unsupported, 400),
        (AttachmentError::NotFound(AttachmentId::of(b"x")), 400),
        (AttachmentError::RequestTooLarge, 400),
        (AttachmentError::Storage("disk".into()), 500),
    ];
    for (refusal, status) in statuses {
        assert_eq!(refusal.http_status(), status, "{refusal}");
    }
    assert!(AttachmentError::TooLarge.to_string().contains("8 MiB"));
    assert!(
        AttachmentError::RequestTooLarge
            .to_string()
            .contains("16 MiB")
    );
}

/// A read by id answers an id the store lacks with a 404, and every other
/// refusal with the status it has anywhere.
#[test]
fn a_read_by_id_answers_an_unknown_id_with_404() {
    let unknown = AttachmentError::NotFound(AttachmentId::of(b"x"));
    assert_eq!((unknown.http_status(), unknown.fetch_status()), (400, 404));
    for refusal in [
        AttachmentError::TooLarge,
        AttachmentError::Unsupported,
        AttachmentError::RequestTooLarge,
        AttachmentError::Storage("disk".into()),
    ] {
        assert_eq!(refusal.fetch_status(), refusal.http_status(), "{refusal}");
    }
}

/// The adapter is handed the store the service itself writes to.
#[tokio::test]
async fn the_services_store_is_the_one_it_writes_to() {
    let (service, _) = service();
    let stored = service.ingest(&png(8, 8)).await.unwrap();

    let blob = service.store().blob(&stored.info.id).await.unwrap();

    assert_eq!(blob.map(|blob| blob.data), Some(png(8, 8)));
}

#[tokio::test]
async fn ids_are_found_by_their_start_in_order() {
    let (service, _) = service();
    let files = [png(1, 1), png(2, 2), jpeg(3, 3)];
    let mut stored = Vec::new();
    for bytes in &files {
        stored.push(service.ingest(bytes).await.unwrap().info.id);
    }
    stored.sort();

    assert_eq!(service.ids_starting_with("").await.unwrap(), stored);
    let first = &stored[0];
    assert_eq!(
        service.ids_starting_with(first.as_str()).await.unwrap(),
        vec![first.clone()]
    );
    assert!(service.ids_starting_with("g").await.unwrap().is_empty());
}
