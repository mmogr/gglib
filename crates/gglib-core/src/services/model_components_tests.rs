//! Tests for [`super`]: a retag reading a family from a model's file.
//!
//! The files are the measured goldens' text written to disk, and
//! [`GoldenParser`] reads them back as the real parser reads a weights file's
//! tensor table; the binary round trip of the same goldens through the
//! fixture writers is pinned in `gglib-gguf`, which this crate cannot depend
//! on.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::Utc;

use super::super::ModelService;
use super::super::model_projector::tests::OneModelRepo;
use crate::domain::image_family_goldens::{self, Golden};
use crate::domain::{ImageFamily, Model, NewModel};
use crate::ports::{GgufCapabilities, GgufMetadata, GgufParseError, GgufParserPort, TensorTable};

/// Reads a file's tensor table from the golden text it holds; a file that
/// is not there is not found, as the real parser says.
pub(crate) struct GoldenParser;

impl GgufParserPort for GoldenParser {
    fn parse(&self, file_path: &Path) -> Result<GgufMetadata, GgufParseError> {
        Err(GgufParseError::InvalidFormat(format!(
            "no header is read here, and {} was asked",
            file_path.display()
        )))
    }
    fn detect_capabilities(&self, _metadata: &GgufMetadata) -> GgufCapabilities {
        GgufCapabilities::empty()
    }
    fn tensor_table(&self, path: &Path) -> Result<TensorTable, GgufParseError> {
        let text = std::fs::read_to_string(path)
            .map_err(|_| GgufParseError::NotFound(path.display().to_string()))?;
        let name = path.file_name().unwrap().to_string_lossy();
        Ok(image_family_goldens::parse(&name, &text))
    }
    fn tensor_table_of_head(&self, _head: &[u8]) -> Result<TensorTable, GgufParseError> {
        Err(GgufParseError::InvalidFormat(
            "no head is read here".to_owned(),
        ))
    }
}

/// Write `golden`'s text into `dir` under the measured file's name.
pub(crate) fn write_golden(dir: &Path, golden: Golden) -> PathBuf {
    let path = dir.join(golden.file_name());
    std::fs::write(&path, golden.text()).unwrap();
    path
}

/// A service over model 1, whose weights are `file` and whose family is
/// `family`.
fn service(file: &Path, family: Option<ImageFamily>) -> (ModelService, Arc<OneModelRepo>) {
    let mut new = NewModel::new("model".to_owned(), file.to_path_buf(), 12.0, Utc::now());
    new.image_family = family;
    let repo = Arc::new(OneModelRepo(Mutex::new(Model::stored(1, &new))));
    (ModelService::new(repo.clone()), repo)
}

fn stored_family(repo: &OneModelRepo) -> Option<ImageFamily> {
    repo.0.lock().unwrap().image_family
}

#[tokio::test]
async fn a_retag_fills_a_missing_family_from_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_golden(dir.path(), Golden::FluxSchnellQ8);
    let (service, repo) = service(&file, None);

    let diff = service
        .retag_model(1, &GoldenParser, false)
        .await
        .unwrap()
        .expect("finding a family is a change");

    assert_eq!(diff.family_found, Some(ImageFamily::Flux1));
    assert!(diff.is_changed());
    assert_eq!(stored_family(&repo), Some(ImageFamily::Flux1));
}

/// A family already set is never changed, even by a file that now reads as
/// another, and in a full rebuild too.
#[tokio::test]
async fn a_retag_never_changes_a_set_family() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_golden(dir.path(), Golden::QwenImage21Q8);
    let (service, repo) = service(&file, Some(ImageFamily::Flux1));

    for full in [false, true] {
        let diff = service.retag_model(1, &GoldenParser, full).await.unwrap();
        assert_eq!(diff, None, "full = {full}");
        assert_eq!(stored_family(&repo), Some(ImageFamily::Flux1));
    }
}

/// Tags are re-derived from stored metadata, so the file does not have to
/// exist; a missing one is no error and finds no family.
#[tokio::test]
async fn a_retag_skips_a_missing_file_without_error() {
    let dir = tempfile::tempdir().unwrap();
    let (service, repo) = service(&dir.path().join("gone.gguf"), None);

    let diff = service.retag_model(1, &GoldenParser, false).await.unwrap();

    assert_eq!(diff, None);
    assert_eq!(stored_family(&repo), None);
}

#[tokio::test]
async fn a_retag_of_a_component_or_a_chat_file_finds_no_family() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_golden(dir.path(), Golden::Qwen3Vl8bQ8);
    let (service, repo) = service(&file, None);

    let diff = service.retag_model(1, &GoldenParser, true).await.unwrap();

    assert_eq!(diff, None);
    assert_eq!(stored_family(&repo), None);
}
