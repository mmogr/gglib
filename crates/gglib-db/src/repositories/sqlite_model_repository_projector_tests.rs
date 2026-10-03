//! `models.projector_path` through the repository: written, read back,
//! cleared, and kept by a re-registration that carries none.

use std::path::PathBuf;

use chrono::Utc;
use gglib_core::NewModel;

use super::*;
use crate::setup::setup_test_database;

fn new_model(name: &str) -> NewModel {
    NewModel::new(
        name.to_string(),
        PathBuf::from(format!("/models/{name}.gguf")),
        7.0,
        Utc::now(),
    )
}

async fn repo() -> SqliteModelRepository {
    SqliteModelRepository::new(setup_test_database().await.unwrap())
}

#[tokio::test]
async fn a_new_model_has_no_projector_and_cannot_see() {
    let repo = repo().await;
    let inserted = repo.insert(&new_model("plain")).await.unwrap();

    assert_eq!(inserted.projector_path, None);
    assert!(!inserted.image_input());
    assert!(!repo.list().await.unwrap()[0].image_input());
}

#[tokio::test]
async fn update_links_and_unlinks_the_projector() {
    let repo = repo().await;
    let mut model = repo.insert(&new_model("qwen")).await.unwrap();

    model.projector_path = Some(PathBuf::from("/models/mmproj-F16.gguf"));
    repo.update(&model).await.unwrap();
    let linked = repo.get_by_id(model.id).await.unwrap();
    assert_eq!(
        linked.projector_path,
        Some(PathBuf::from("/models/mmproj-F16.gguf"))
    );
    assert!(linked.image_input());
    // The joined listing reads the same column.
    assert_eq!(
        repo.list().await.unwrap()[0].projector_path,
        linked.projector_path
    );

    model.projector_path = None;
    repo.update(&model).await.unwrap();
    assert_eq!(repo.get_by_id(model.id).await.unwrap().projector_path, None);
}

#[tokio::test]
async fn insert_stores_the_projector_a_new_model_carries() {
    let repo = repo().await;
    let mut new = new_model("qwen");
    new.projector_path = Some(PathBuf::from("/models/mmproj-F16.gguf"));

    let inserted = repo.insert(&new).await.unwrap();

    assert_eq!(inserted.projector_path, new.projector_path);
}

/// A local re-import carries no projector. It must not unlink the one the
/// model has.
#[tokio::test]
async fn a_re_registration_without_a_projector_keeps_the_link() {
    let repo = repo().await;
    let mut model = repo.insert(&new_model("qwen")).await.unwrap();
    model.projector_path = Some(PathBuf::from("/models/mmproj-F16.gguf"));
    repo.update(&model).await.unwrap();

    let again = repo.insert(&new_model("qwen")).await.unwrap();

    assert_eq!(again.id, model.id);
    assert_eq!(again.projector_path, model.projector_path);
}

/// Stored in the resolved form `file_path` takes, so one file is one string
/// whichever way it was spelled.
#[tokio::test]
async fn update_stores_the_projector_path_resolved() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("mmproj-F16.gguf");
    std::fs::write(&file, b"x").unwrap();
    let repo = repo().await;
    let mut model = repo.insert(&new_model("qwen")).await.unwrap();

    model.projector_path = Some(dir.path().join(".").join("mmproj-F16.gguf"));
    repo.update(&model).await.unwrap();

    assert_eq!(
        repo.get_by_id(model.id).await.unwrap().projector_path,
        Some(std::fs::canonicalize(&file).unwrap())
    );
}
