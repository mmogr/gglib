//! Tests that a model's files are what their headers say, whatever their
//! names, and what its link says when no header can be read.

use std::path::Path;

use chrono::Utc;

use super::*;
use crate::domain::NewModel;
use crate::ports::{FirstBytesParser, ParsedPaths};

/// Model 1, whose weights are `weights` in `dir`, linked to `projector` there
/// when one is given.
fn model(dir: &Path, weights: &str, projector: Option<&str>) -> Model {
    let mut new = NewModel::new("zeta".to_owned(), dir.join(weights), 7.0, Utc::now());
    new.projector_path = projector.map(|name| dir.join(name));
    Model::stored(1, &new)
}

/// The rows `names`, of model 1, in order.
fn rows(names: &[&str]) -> Vec<ModelFile> {
    names
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
        .collect()
}

const WEIGHTS: Option<GgufFileRole> = Some(GgufFileRole::Weights);
const PROJECTOR: Option<GgufFileRole> = Some(GgufFileRole::Projector);

/// The roles of `names`, the rows of `model`, read beside its weights.
async fn roles(model: &Model, names: &[&str]) -> Vec<Option<GgufFileRole>> {
    let folder = ModelFolder::new(&model.file_path, None);
    let parser: Arc<dyn GgufParserPort> = Arc::new(FirstBytesParser);
    row_roles(model, &folder, &rows(names), &parser).await
}

#[tokio::test]
async fn a_projector_named_like_weights_is_the_projector() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("zeta.Q8_0.gguf"), "weights").unwrap();
    std::fs::write(dir.path().join("vision-F16.gguf"), "projector").unwrap();
    let model = model(dir.path(), "zeta.Q8_0.gguf", None);

    let found = roles(&model, &["zeta.Q8_0.gguf", "vision-F16.gguf"]).await;

    assert_eq!(found, [WEIGHTS, PROJECTOR]);
}

#[tokio::test]
async fn weights_named_like_a_projector_are_weights() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("mmproj-F16.gguf"), "weights").unwrap();
    let model = model(dir.path(), "mmproj-F16.gguf", None);

    let found = roles(&model, &["mmproj-F16.gguf"]).await;

    assert_eq!(found, [WEIGHTS]);
}

/// No header can be read from a missing file: the one the model links as
/// its projector is the projector, and any other is unknown, whatever it is
/// named.
#[tokio::test]
async fn a_missing_file_is_the_projector_only_when_the_model_links_it() {
    let dir = tempfile::tempdir().unwrap();
    let model = model(dir.path(), "zeta.Q8_0.gguf", Some("vision-F16.gguf"));

    let found = roles(
        &model,
        &["zeta.Q8_0.gguf", "vision-F16.gguf", "mmproj-Q8_0.gguf"],
    )
    .await;

    assert_eq!(found, [WEIGHTS, PROJECTOR, None]);
}

/// A file that is there but holds no header the model's link does not
/// name is unknown, not weights.
#[tokio::test]
async fn an_unlinked_file_with_a_corrupt_header_is_unknown() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("zeta.Q8_0.gguf"), "weights").unwrap();
    std::fs::write(dir.path().join("zeta-00002.gguf"), "damaged").unwrap();
    let model = model(dir.path(), "zeta.Q8_0.gguf", None);

    let found = roles(&model, &["zeta.Q8_0.gguf", "zeta-00002.gguf"]).await;

    assert_eq!(found, [WEIGHTS, None]);
}

/// A header that can be read wins over the link.
#[tokio::test]
async fn a_linked_file_whose_header_says_weights_is_weights() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("vision-F16.gguf"), "weights").unwrap();
    let model = model(dir.path(), "zeta.Q8_0.gguf", Some("vision-F16.gguf"));

    let found = roles(&model, &["vision-F16.gguf"]).await;

    assert_eq!(found, [WEIGHTS]);
}

/// The model's own file is its weights without a read; every other row has
/// its header read, once.
#[tokio::test]
async fn the_weights_file_is_weights_unread_and_every_other_row_is_read() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("zeta.Q8_0.gguf"), "projector").unwrap();
    std::fs::write(dir.path().join("vision-F16.gguf"), "projector").unwrap();
    std::fs::write(dir.path().join("zeta-00002.gguf"), "weights").unwrap();
    let model = model(dir.path(), "zeta.Q8_0.gguf", None);
    let folder = ModelFolder::new(&model.file_path, None);
    let names = ["zeta.Q8_0.gguf", "vision-F16.gguf", "zeta-00002.gguf"];
    let spy = Arc::new(ParsedPaths::default());
    let parser: Arc<dyn GgufParserPort> = spy.clone();

    let found = row_roles(&model, &folder, &rows(&names), &parser).await;

    assert_eq!(found, [WEIGHTS, PROJECTOR, WEIGHTS]);
    assert_eq!(
        spy.parsed(),
        [dir.path().join(names[1]), dir.path().join(names[2])]
    );
}
