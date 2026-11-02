//! A re-registration of one file and the header's two facts: they are
//! replaced together or kept together, never paired across two reads.

use std::path::PathBuf;

use chrono::Utc;
use gglib_core::NewModel;
use gglib_core::domain::FileType;

use super::*;
use crate::setup::setup_test_database;

/// The file `/models/a.gguf`, read as `file_type` and `bits`.
fn read_as(file_type: Option<u32>, bits: Option<f64>) -> NewModel {
    let mut new = NewModel::new(
        "a".to_owned(),
        PathBuf::from("/models/a.gguf"),
        7.0,
        Utc::now(),
    );
    new.file_type = file_type.map(FileType);
    new.bits_per_weight = bits;
    new
}

/// The type and the bits the one stored row holds.
async fn stored(repo: &SqliteModelRepository) -> (Option<FileType>, Option<f64>) {
    let [model] = &repo.list().await.unwrap()[..] else {
        panic!("one row");
    };
    (model.file_type, model.bits_per_weight)
}

#[tokio::test]
async fn a_reimport_replaces_the_type_and_the_bits_together_or_keeps_both() {
    let repo = SqliteModelRepository::new(setup_test_database().await.unwrap());
    repo.insert(&read_as(Some(15), Some(4.5))).await.unwrap();

    repo.insert(&read_as(Some(7), Some(8.5))).await.unwrap();
    assert_eq!(stored(&repo).await, (Some(FileType(7)), Some(8.5)));

    // A header that states no type but measured its bits: the old type is
    // not kept beside the new bits.
    repo.insert(&read_as(None, Some(6.0))).await.unwrap();
    assert_eq!(stored(&repo).await, (None, Some(6.0)));

    repo.insert(&read_as(Some(18), None)).await.unwrap();
    assert_eq!(stored(&repo).await, (Some(FileType(18)), None));

    // A header that could not be read at all keeps the pair stored.
    repo.insert(&read_as(None, None)).await.unwrap();
    assert_eq!(stored(&repo).await, (Some(FileType(18)), None));
}
