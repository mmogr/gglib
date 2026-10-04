//! Tests for [`projector_choices`].

use std::path::Path;

use chrono::Utc;

use super::*;
use crate::domain::NewModel;

fn model(id: i64, weights: &Path, projector: Option<&Path>) -> Model {
    let mut new = NewModel::new(format!("m{id}"), weights.to_path_buf(), 7.0, Utc::now());
    new.projector_path = projector.map(Path::to_path_buf);
    Model::stored(id, &new)
}

fn file(index: i32, name: &str) -> ModelFile {
    ModelFile {
        id: i64::from(index) + 1,
        model_id: 3,
        file_path: name.to_owned(),
        file_index: index,
        expected_size: 1,
        hf_oid: None,
        last_verified_at: None,
    }
}

/// The model-3 shape: weights and a projector stored as two files of one
/// model. The projector is offered by its absolute path; the weights are not.
#[test]
fn a_models_own_projector_file_is_offered_and_its_weights_are_not() {
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path().canonicalize().unwrap();
    let weights = dir.join("X.Q8_0.gguf");
    let projector = dir.join("X.mmproj-Q8_0.gguf");
    std::fs::write(&weights, b"w").unwrap();
    std::fs::write(&projector, b"p").unwrap();
    let files = [file(0, "X.Q8_0.gguf"), file(1, "X.mmproj-Q8_0.gguf")];
    let this = model(3, &weights, None);

    let choices = projector_choices(&this, &files, std::slice::from_ref(&this));

    assert_eq!(choices, vec![projector]);
}

/// A row for a projector that was never fetched is not a choice: linking it
/// would be refused.
#[test]
fn an_own_projector_that_is_not_on_disk_is_not_offered() {
    let dir = tempfile::tempdir().unwrap();
    let weights = dir.path().join("X.Q8_0.gguf");
    std::fs::write(&weights, b"w").unwrap();
    let this = model(3, &weights, None);

    let choices = projector_choices(&this, &[file(1, "X.mmproj-Q8_0.gguf")], &[]);

    assert!(choices.is_empty(), "{choices:?}");
}

/// A projector another model loads is offered to this one, and one that two
/// models share, or that is also this model's own file, is offered once.
#[test]
fn projectors_in_use_are_offered_each_once() {
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path().canonicalize().unwrap();
    let weights = dir.join("X.Q8_0.gguf");
    let own = dir.join("X.mmproj-Q8_0.gguf");
    std::fs::write(&weights, b"w").unwrap();
    std::fs::write(&own, b"p").unwrap();
    let elsewhere = Path::new("/elsewhere/mmproj-F16.gguf");
    let this = model(3, &weights, Some(&own));
    let library = [
        this.clone(),
        model(1, Path::new("/a/a.gguf"), Some(elsewhere)),
        model(2, Path::new("/b/b.gguf"), Some(elsewhere)),
        model(4, Path::new("/c/c.gguf"), None),
    ];

    let choices = projector_choices(&this, &[file(1, "X.mmproj-Q8_0.gguf")], &library);

    let mut expected = vec![elsewhere.to_path_buf(), own];
    expected.sort();
    assert_eq!(choices, expected);
}
