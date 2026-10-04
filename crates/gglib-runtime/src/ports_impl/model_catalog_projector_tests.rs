//! A model's projector on its launch specification.

use super::*;
use chrono::Utc;
use gglib_core::domain::NewModel;
use std::path::Path;

fn model(weights: &Path, projector: Option<&Path>) -> Model {
    let mut new = NewModel::new("qwen3".to_owned(), weights.to_path_buf(), 7.0, Utc::now());
    new.projector_path = projector.map(Path::to_path_buf);
    Model::stored(7, &new)
}

/// The launch gets the path to pass as `--mmproj`, and budgets memory for
/// both files it will load.
#[test]
fn a_linked_model_launches_with_its_projector_and_is_sized_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let weights = dir.path().join("qwen3.Q8_0.gguf");
    let projector = dir.path().join("mmproj-F16.gguf");
    std::fs::write(&weights, vec![0u8; 4096]).unwrap();
    std::fs::write(&projector, vec![0u8; 512]).unwrap();

    let spec = model_to_launch_spec(model(&weights, Some(&projector)));

    assert_eq!(spec.projector.as_deref(), Some(projector.as_path()));
    assert_eq!(spec.file_size_bytes, 4608);
}

#[test]
fn an_unlinked_model_launches_without_one() {
    let dir = tempfile::tempdir().unwrap();
    let weights = dir.path().join("qwen3.Q8_0.gguf");
    std::fs::write(&weights, vec![0u8; 4096]).unwrap();

    let spec = model_to_launch_spec(model(&weights, None));

    assert_eq!(spec.projector, None);
    assert_eq!(spec.file_size_bytes, 4096);
}

/// The list says a model reads images exactly when it is linked.
#[test]
fn a_summary_reports_image_input_from_the_link() {
    let weights = Path::new("/models/qwen3.Q8_0.gguf");
    let projector = Path::new("/models/mmproj-F16.gguf");

    assert!(model_to_summary(&model(weights, Some(projector))).image_input);
    assert!(!model_to_summary(&model(weights, None)).image_input);
}
