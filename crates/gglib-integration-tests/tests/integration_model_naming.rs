//! Parity tests for model naming across the local-file and `HuggingFace`
//! add paths.
//!
//! These exercise the real [`gglib_gguf::GgufParser`] against hand-written
//! GGUF fixtures, through both [`ModelService::import_from_file`] and
//! [`ModelRegistrar::register_model`], to prove the two paths resolve the
//! same `models.name` for the same underlying file — the divergence this
//! naming ladder fixes.

mod common;

use std::path::Path;
use std::sync::Arc;

use common::{setup_test_pool, write_gguf_fixture};

use gglib_core::services::ModelService;
use gglib_core::{CompletedDownload, ModelRegistrarPort, ModelRepository, Quantization};
use gglib_db::{CoreFactory, SqliteModelRepository};
use gglib_gguf::GgufParser;

async fn import_locally(file_path: &Path) -> gglib_core::Model {
    let pool = setup_test_pool().await.unwrap();
    let repo = Arc::new(SqliteModelRepository::new(pool));
    let service = ModelService::new(repo);
    service
        .import_from_file(file_path, &GgufParser::new(), None)
        .await
        .unwrap()
}

async fn register_from_hf(
    file_path: &Path,
    repo_id: &str,
    quantization: &str,
) -> gglib_core::Model {
    let pool = setup_test_pool().await.unwrap();
    let registrar = CoreFactory::model_registrar_for_test(pool, Arc::new(GgufParser::new()));
    let download = CompletedDownload {
        primary_path: file_path.to_path_buf(),
        all_paths: vec![file_path.to_path_buf()],
        quantization: Quantization::from_filename(quantization),
        repo_id: repo_id.to_string(),
        commit_sha: "abc123".to_string(),
        is_sharded: false,
        file_paths: None,
        hf_tags: vec![],
        hf_file_entries: vec![],
    };
    registrar.register_model(&download).await.unwrap()
}

#[tokio::test]
async fn local_and_hf_agree_on_name_when_general_name_is_present() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("qwen3-8b-q4_k_m.gguf");
    write_gguf_fixture(
        &path,
        &[
            ("general.name", "Qwen3-8B"),
            ("general.architecture", "qwen3"),
        ],
    );

    let local = import_locally(&path).await;
    let hf = register_from_hf(&path, "unsloth/Qwen3-8B-GGUF", "Q4_K_M").await;

    assert_eq!(local.name, "Qwen3-8B");
    assert_eq!(hf.name, "Qwen3-8B");
}

#[tokio::test]
async fn local_and_hf_diverge_predictably_without_general_name() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("qwen3-8b-q4_k_m.gguf");
    write_gguf_fixture(&path, &[("general.architecture", "qwen3")]);

    let local = import_locally(&path).await;
    let hf = register_from_hf(&path, "unsloth/Qwen3-8B-GGUF", "Q4_K_M").await;

    // No general.name on either path: local falls to the file stem, HF
    // falls to the repo short name (owner and -GGUF stripped). Different
    // signals, both still names -- not a "Qwen3-8B-GGUF"-vs-"Qwen3-8B" gap.
    assert_eq!(local.name, "qwen3-8b-q4_k_m");
    assert_eq!(hf.name, "Qwen3-8B");
}

#[tokio::test]
async fn duplicate_names_from_distinct_repos_both_persist_and_resolve() {
    let dir = tempfile::tempdir().unwrap();
    let path_a = dir.path().join("model-a.gguf");
    let path_b = dir.path().join("model-b.gguf");
    write_gguf_fixture(&path_a, &[("general.name", "Qwen3-8B")]);
    write_gguf_fixture(&path_b, &[("general.name", "Qwen3-8B")]);

    let pool = setup_test_pool().await.unwrap();
    let repo = Arc::new(SqliteModelRepository::new(pool.clone()));
    let registrar = CoreFactory::model_registrar_for_test(pool, Arc::new(GgufParser::new()));

    for (path, repo_id) in [
        (&path_a, "bartowski/Qwen3-8B-GGUF"),
        (&path_b, "unsloth/Qwen3-8B-GGUF"),
    ] {
        let download = CompletedDownload {
            primary_path: path.clone(),
            all_paths: vec![path.clone()],
            quantization: Quantization::from_filename("Q4_K_M"),
            repo_id: repo_id.to_string(),
            commit_sha: "abc123".to_string(),
            is_sharded: false,
            file_paths: None,
            hf_tags: vec![],
            hf_file_entries: vec![],
        };
        registrar.register_model(&download).await.unwrap();
    }

    let all = repo.list().await.unwrap();
    assert_eq!(all.len(), 2);
    assert!(all.iter().all(|m| m.name == "Qwen3-8B"));

    // Resolution is deterministic (not corrupted) even with a name collision.
    let resolved = repo.get_by_identifier("Qwen3-8B").await.unwrap();
    assert!(resolved.is_some());
}
