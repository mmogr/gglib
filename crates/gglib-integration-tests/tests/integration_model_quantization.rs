//! Regression test proving the local-file import path resolves modern
//! quantization filename conventions correctly.
//!
//! `gglib-gguf`'s filename-based quantization matcher used to have its own
//! 14-pattern hardcoded list with no Unsloth "UD-" dynamic-quant handling.
//! Delegating it to the shared `gglib_core::download::Quantization` matcher
//! fixes this at the source, but this test proves the fix actually reaches
//! the real local-add path end-to-end, through the real `GgufParser`.

mod common;

use std::sync::Arc;

use common::{setup_test_pool, write_gguf_fixture};

use gglib_core::services::{ImportMode, ModelService};
use gglib_db::SqliteModelRepository;
use gglib_gguf::GgufParser;

#[tokio::test]
async fn local_import_resolves_unsloth_dynamic_quantization_from_filename() {
    let pool = setup_test_pool().await.unwrap();
    let repo = Arc::new(SqliteModelRepository::new(pool));
    let service = ModelService::new(repo);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("qwen3-8b-UD-Q4_K_M.gguf");
    write_gguf_fixture(&path, &[("general.architecture", "qwen3")]);

    let model = service
        .import_from_file(&path, &GgufParser::new(), None, ImportMode::Fresh)
        .await
        .unwrap();

    assert_eq!(model.quantization, Some("UD-Q4_K_M".to_string()));
}
