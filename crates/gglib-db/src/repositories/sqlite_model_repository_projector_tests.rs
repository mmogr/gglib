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

/// A download registered through the real repository, by a path that is not
/// the resolved one the repository stores.
#[cfg(unix)]
mod through_a_symlink {
    use std::path::Path;
    use std::sync::Arc;

    use gglib_core::download::Quantization;
    use gglib_core::ports::{CompletedDownload, GgufParserPort, ModelRegistrarPort, TensorTable};
    use gglib_core::{GgufCapabilities, GgufFileRole, GgufMetadata, GgufParseError};

    use super::super::*;
    use crate::CoreFactory;
    use crate::setup::setup_test_database;

    /// Reads every header as a projector's.
    struct ProjectorHeader;

    impl GgufParserPort for ProjectorHeader {
        fn parse(&self, _file_path: &Path) -> Result<GgufMetadata, GgufParseError> {
            Ok(GgufMetadata {
                role: GgufFileRole::Projector,
                ..Default::default()
            })
        }
        fn detect_capabilities(&self, _metadata: &GgufMetadata) -> GgufCapabilities {
            GgufCapabilities::empty()
        }
        fn tensor_table(&self, _path: &Path) -> Result<TensorTable, GgufParseError> {
            Err(GgufParseError::InvalidFormat("no tensor table".to_owned()))
        }
        fn tensor_table_of_head(&self, _head: &[u8]) -> Result<TensorTable, GgufParseError> {
            Err(GgufParseError::InvalidFormat("no tensor table".to_owned()))
        }
    }

    /// The models directory is reached through a symlink. The model's owner
    /// linked another projector by hand; downloading the model again, as a
    /// repair does, finds the model it already holds and keeps that link.
    #[tokio::test]
    async fn a_model_downloaded_again_through_a_symlink_keeps_its_chosen_link() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let models = dir.path().join("models");
        std::os::unix::fs::symlink(&real, &models).unwrap();
        let weights = models.join("zeta.Q8_0.gguf");
        let projector = models.join("mmproj-F16.gguf");
        std::fs::write(&weights, b"x").unwrap();
        std::fs::write(&projector, b"x").unwrap();
        let download = CompletedDownload {
            primary_path: weights.clone(),
            all_paths: vec![weights, projector.clone()],
            projector_path: Some(projector),
            components: vec![],
            quantization: Quantization::Q8_0,
            repo_id: "owner/zeta-GGUF".to_owned(),
            commit_sha: "abc123".to_owned(),
            is_sharded: false,
            file_paths: None,
            hf_tags: vec![],
            hf_file_entries: vec![],
        };
        let pool = setup_test_database().await.unwrap();
        let repo = SqliteModelRepository::new(pool.clone());
        let registrar = CoreFactory::model_registrar_for_test(pool, Arc::new(ProjectorHeader));
        let mut model = registrar.register_model(&download).await.unwrap().model;
        assert_eq!(
            model.projector_path,
            Some(std::fs::canonicalize(real.join("mmproj-F16.gguf")).unwrap())
        );
        let chosen = dir.path().join("chosen").join("mmproj-BF16.gguf");
        model.projector_path = Some(chosen.clone());
        repo.update(&model).await.unwrap();

        let again = registrar.register_model(&download).await.unwrap();

        assert_eq!(again.model.id, model.id);
        let stored = repo.get_by_id(model.id).await.unwrap();
        assert_eq!(stored.projector_path, Some(chosen));
        let kept = again.projector_refusal.expect("the kept link is reported");
        assert!(kept.contains("mmproj-BF16.gguf"), "{kept}");
    }
}
