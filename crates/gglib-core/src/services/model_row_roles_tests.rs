//! Tests that a model's files are what their headers say, whatever their
//! names, and what its link says when no header can be read.

use std::path::Path;

use chrono::Utc;

use super::*;
use crate::domain::NewModel;
use crate::ports::FirstBytesParser;

/// Model 1, whose weights are `weights` in `dir`, linked to `projector` there
/// when one is given.
fn model(dir: &Path, weights: &str, projector: Option<&str>) -> Model {
    let mut new = NewModel::new("zeta".to_owned(), dir.join(weights), 7.0, Utc::now());
    new.projector_path = projector.map(|name| dir.join(name));
    Model::stored(1, &new)
}

/// The roles of `names`, the rows of `model`, read beside its weights.
fn roles(model: &Model, names: &[&str]) -> Vec<GgufFileRole> {
    let rows: Vec<ModelFile> = names
        .iter()
        .zip(0..)
        .map(|(name, index)| ModelFile {
            id: i64::from(index) + 1,
            model_id: 1,
            file_path: (*name).to_owned(),
            file_index: index,
            expected_size: 1,
            hf_oid: None,
            last_verified_at: None,
        })
        .collect();
    let folder = ModelFolder::new(&model.file_path, None);
    row_roles(model, &folder, &rows, &FirstBytesParser)
}

#[test]
fn a_projector_named_like_weights_is_the_projector() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("zeta.Q8_0.gguf"), "weights").unwrap();
    std::fs::write(dir.path().join("vision-F16.gguf"), "projector").unwrap();
    let model = model(dir.path(), "zeta.Q8_0.gguf", None);

    let found = roles(&model, &["zeta.Q8_0.gguf", "vision-F16.gguf"]);

    assert_eq!(found, [GgufFileRole::Weights, GgufFileRole::Projector]);
}

#[test]
fn weights_named_like_a_projector_are_weights() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("mmproj-F16.gguf"), "weights").unwrap();
    let model = model(dir.path(), "mmproj-F16.gguf", None);

    let found = roles(&model, &["mmproj-F16.gguf"]);

    assert_eq!(found, [GgufFileRole::Weights]);
}

/// No header can be read from a missing file: the one the model links as
/// its projector is the projector, and any other counts as weights, whatever
/// it is named.
#[test]
fn a_missing_file_is_the_projector_only_when_the_model_links_it() {
    let dir = tempfile::tempdir().unwrap();
    let model = model(dir.path(), "zeta.Q8_0.gguf", Some("vision-F16.gguf"));

    let found = roles(
        &model,
        &["zeta.Q8_0.gguf", "vision-F16.gguf", "mmproj-Q8_0.gguf"],
    );

    assert_eq!(
        found,
        [
            GgufFileRole::Weights,
            GgufFileRole::Projector,
            GgufFileRole::Weights
        ]
    );
}

/// A header that can be read wins over the link.
#[test]
fn a_linked_file_whose_header_says_weights_is_weights() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("vision-F16.gguf"), "weights").unwrap();
    let model = model(dir.path(), "zeta.Q8_0.gguf", Some("vision-F16.gguf"));

    let found = roles(&model, &["vision-F16.gguf"]);

    assert_eq!(found, [GgufFileRole::Weights]);
}
