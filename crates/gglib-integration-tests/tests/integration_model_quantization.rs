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
    assert_eq!(import(&named).await.unwrap().quant_label(), Some("Q8_0"));
    assert_eq!(import(&unstated).await.unwrap().quant_label(), None);
}

/// A model added before the header's facts were kept has none of them; the
/// daemon's pass reads them from its file, summing every shard of a split,
/// once per library, and leaves a model whose file is gone as it is.
#[tokio::test]
async fn the_daemons_pass_reads_the_facts_of_a_model_that_has_none_once() {
    let pool = setup_test_pool().await.unwrap();
    let repo = Arc::new(SqliteModelRepository::new(pool.clone()));
    let service = ModelService::new(repo.clone());
    let parser = Arc::new(GgufParser::new());
    let dir = tempfile::tempdir().unwrap();

    // A split in two: the first states the type and the split, and each
    // holds 64 weights, in 48 and then 80 bytes: 8 bits each, together.
    let first = dir.path().join("big-00001-of-00002.gguf");
    let second = dir.path().join("big-00002-of-00002.gguf");
    let pairs = [
        ("general.file_type", GgufValue::U32(7)),
        ("split.count", GgufValue::U16(2)),
    ];
    write_gguf(&first, &pairs, &[("a.weight", &[8, 8])], 48);
    write_gguf(&second, &[], &[("b.weight", &[8, 8])], 80);
    let gone = dir.path().join("gone.gguf");
    write_gguf(&gone, &pairs[..1], &[("a.weight", &[8, 8])], 64);

    let mut ids = Vec::new();
    for path in [&first, &gone] {
        let model = service
            .import_from_file(path, parser.as_ref(), None, ImportMode::Fresh)
            .await
            .unwrap();
        assert!(model.file_type.is_some(), "an import reads them itself");
        ids.push(model.id);
    }
    // As a library from before the facts were kept holds them: not at all.
    let held = &service;
    let forget = |id| async move {
        let mut model = held.get_by_id(id).await.unwrap().unwrap();
        model.file_type = None;
        model.bits_per_weight = None;
        held.update(&model).await.unwrap();
    };
    forget(ids[0]).await;
    forget(ids[1]).await;
    std::fs::remove_file(&gone).unwrap();

    let pass = gglib_db::fill_header_facts_once(&pool, parser.clone());
    assert_eq!(pass.await.unwrap(), Some(1));
    let split = service.get_by_id(ids[0]).await.unwrap().unwrap();
    assert_eq!(split.quant_label(), Some("Q8_0"));
    assert_eq!(split.bits_per_weight, Some(8.0));
    assert_eq!(
        split.variant_label, None,
        "nothing names an uploader's label"
    );
    let missing = service.get_by_id(ids[1]).await.unwrap().unwrap();
    assert_eq!(missing.file_type, None);

    // Once per library: a later start reads no file again.
    forget(ids[0]).await;
    let pass = gglib_db::fill_header_facts_once(&pool, parser);
    assert_eq!(pass.await.unwrap(), None);
    let split = service.get_by_id(ids[0]).await.unwrap().unwrap();
    assert_eq!(split.file_type, None);
}
