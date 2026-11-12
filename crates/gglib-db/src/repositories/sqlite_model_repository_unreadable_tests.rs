//! Columns read leniently: text this build cannot read degrades to none
//! rather than failing every read of the row.

use std::path::PathBuf;

use chrono::Utc;
use gglib_core::NewModel;

use super::*;
use crate::setup::setup_test_database;

/// A model named `name`, in a file of its own.
fn make_model(name: &str) -> NewModel {
    NewModel::new(
        name.to_string(),
        PathBuf::from(format!("/models/{name}.gguf")),
        Some(7.0),
        Utc::now(),
    )
}

async fn repo() -> SqliteModelRepository {
    SqliteModelRepository::new(setup_test_database().await.unwrap())
}

/// Rows written before the column existed (or carrying garbage) read
/// as "no spec" — the tag fallback applies downstream.
#[tokio::test]
async fn unreadable_dialect_spec_reads_as_none() {
    let repo = repo().await;
    let inserted = repo.insert(&make_model("Legacy")).await.unwrap();
    assert_eq!(inserted.dialect_spec, None);

    sqlx::query("UPDATE models SET dialect_spec = 'not json' WHERE id = ?")
        .bind(inserted.id)
        .execute(&repo.pool)
        .await
        .unwrap();
    let fetched = repo.get_by_id(inserted.id).await.unwrap();
    assert_eq!(fetched.dialect_spec, None);
}

/// Same tolerance as `dialect_spec`: garbage in the column degrades to
/// "never observed" rather than failing every read of the row.
#[tokio::test]
async fn unreadable_template_caps_reads_as_none() {
    let repo = repo().await;
    let inserted = repo.insert(&make_model("Iota")).await.unwrap();

    sqlx::query("UPDATE models SET template_caps = 'not json' WHERE id = ?")
        .bind(inserted.id)
        .execute(&repo.pool)
        .await
        .unwrap();

    let fetched = repo.get_by_id(inserted.id).await.unwrap();
    assert_eq!(fetched.template_caps, None);
}
