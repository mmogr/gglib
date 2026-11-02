//! The local-file import path takes a model's type from its header, through
//! the real `GgufParser`, and never from its name: a file is imported under
//! whatever name its owner gave it.

mod common;

use std::sync::Arc;

use common::setup_test_pool;

use gglib_core::domain::gguf::GgufValue;
use gglib_core::services::{ImportMode, ModelService};
use gglib_db::SqliteModelRepository;
use gglib_gguf::{GgufParser, write_gguf};

#[tokio::test]
async fn local_import_takes_the_type_from_the_header_not_the_name() {
    let pool = setup_test_pool().await.unwrap();
    let repo = Arc::new(SqliteModelRepository::new(pool));
    let service = ModelService::new(repo);

    let dir = tempfile::tempdir().unwrap();
    let named = dir.path().join("qwen3-8b-UD-Q4_K_XL.gguf");
    write_gguf(
        &named,
        &[
            (
                "general.architecture",
                GgufValue::String("qwen3".to_owned()),
            ),
            ("general.file_type", GgufValue::U32(7)),
        ],
        &[],
        0,
    );
    let unstated = dir.path().join("qwen3-8b-Q6_K.gguf");
    write_gguf(
        &unstated,
        &[(
            "general.architecture",
            GgufValue::String("qwen3".to_owned()),
        )],
        &[],
        0,
    );

    let parser = GgufParser::new();
    let import = |path| service.import_from_file(path, &parser, None, ImportMode::Fresh);
    assert_eq!(
        import(&named).await.unwrap().quantization.as_deref(),
        Some("Q8_0")
    );
    assert_eq!(import(&unstated).await.unwrap().quantization, None);
}
