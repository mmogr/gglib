//! Columns read leniently: text this build cannot read degrades to none
//! rather than failing every read of the row, and an edit to another field
//! keeps it.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::Utc;
use gglib_core::NewModel;
use gglib_core::domain::{DefaultsOrigin, InferenceConfig, TemplateCaps};

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

/// Write `text` into `column` of row `id`, past the repository.
async fn plant(repo: &SqliteModelRepository, id: i64, column: &str, text: &str) {
    sqlx::query(&format!("UPDATE models SET {column} = ? WHERE id = ?"))
        .bind(text)
        .bind(id)
        .execute(&repo.pool)
        .await
        .unwrap();
}

/// The text `column` of row `id` holds.
async fn stored(repo: &SqliteModelRepository, id: i64, column: &str) -> Option<String> {
    sqlx::query_scalar(&format!("SELECT {column} FROM models WHERE id = ?"))
        .bind(id)
        .fetch_one(&repo.pool)
        .await
        .unwrap()
}

/// Defaults this build cannot read, with the origin stored for them.
async fn plant_unreadable_defaults(repo: &SqliteModelRepository, id: i64) {
    plant(repo, id, "inference_defaults", r#"{"temperature":"warm"}"#).await;
    plant(repo, id, "defaults_origin", "published").await;
}

fn temperature(value: f32) -> InferenceConfig {
    InferenceConfig {
        temperature: Some(value),
        ..Default::default()
    }
}

/// Garbage in the column degrades to "never observed" rather than failing
/// every read of the row.
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

/// Each lenient column, holding text this build cannot read.
const UNREADABLE: [(&str, &str); 9] = [
    ("tags", "not json"),
    ("metadata", r#"{"general.name":1}"#),
    ("inference_defaults", r#"{"temperature":"warm"}"#),
    ("defaults_origin", "published"),
    ("server_defaults", "not json"),
    ("template_caps", "not json"),
    ("image_family", "flux9"),
    ("download_date", "yesterday"),
    ("last_update_check", "yesterday"),
];

#[tokio::test]
async fn a_rename_keeps_every_column_this_build_cannot_read() {
    let repo = repo().await;
    let id = repo.insert(&make_model("Lambda")).await.unwrap().id;
    for (column, text) in UNREADABLE {
        plant(&repo, id, column, text).await;
    }

    let mut model = repo.get_by_id(id).await.unwrap();
    model.name = "Renamed".to_owned();
    repo.update(&model).await.unwrap();

    let mut rewritten = Vec::new();
    for (column, text) in UNREADABLE {
        if stored(&repo, id, column).await.as_deref() != Some(text) {
            rewritten.push(column);
        }
    }
    assert_eq!(rewritten, Vec::<&str>::new(), "columns the rename rewrote");
    assert_eq!(stored(&repo, id, "name").await.as_deref(), Some("Renamed"));
}

#[tokio::test]
async fn a_tag_edit_keeps_unreadable_defaults_and_their_origin() {
    let repo = repo().await;
    let id = repo.insert(&make_model("Mu")).await.unwrap().id;
    plant_unreadable_defaults(&repo, id).await;

    let mut model = repo.get_by_id(id).await.unwrap();
    model.tags = vec!["chat".to_owned()];
    repo.update(&model).await.unwrap();

    assert_eq!(
        stored(&repo, id, "tags").await.as_deref(),
        Some(r#"["chat"]"#)
    );
    assert_eq!(
        stored(&repo, id, "inference_defaults").await.as_deref(),
        Some(r#"{"temperature":"warm"}"#)
    );
    assert_eq!(
        stored(&repo, id, "defaults_origin").await.as_deref(),
        Some("published")
    );
}

#[tokio::test]
async fn a_new_value_replaces_unreadable_text() {
    let repo = repo().await;
    let id = repo.insert(&make_model("Nu")).await.unwrap().id;
    plant_unreadable_defaults(&repo, id).await;
    plant(&repo, id, "template_caps", "not json").await;
    let caps = TemplateCaps {
        supports_tools: Some(true),
        ..Default::default()
    };

    let mut model = repo.get_by_id(id).await.unwrap();
    model.inference_defaults = Some(temperature(0.4));
    model.defaults_origin = Some(DefaultsOrigin::User);
    model.template_caps = Some(caps.clone());
    repo.update(&model).await.unwrap();

    let read = repo.get_by_id(id).await.unwrap();
    assert_eq!(read.inference_defaults, Some(temperature(0.4)));
    assert_eq!(read.defaults_origin, Some(DefaultsOrigin::User));
    assert_eq!(read.template_caps, Some(caps));
}

/// The rest of the nine take a new value too, each on its own: changing one
/// date leaves the other's text alone.
#[tokio::test]
async fn a_new_value_replaces_unreadable_text_in_each_other_column() {
    let repo = repo().await;
    let id = repo.insert(&make_model("Omicron")).await.unwrap().id;
    for (column, text) in [
        ("metadata", r#"{"general.name":1}"#),
        ("image_family", "flux9"),
        ("download_date", "yesterday"),
        ("last_update_check", "yesterday"),
    ] {
        plant(&repo, id, column, text).await;
    }

    let mut model = repo.get_by_id(id).await.unwrap();
    model.metadata = HashMap::from([("general.name".to_string(), "Omicron".to_string())]);
    model.image_family = Some(ImageFamily::Sdxl);
    model.download_date = Some(Utc::now());
    repo.update(&model).await.unwrap();

    let read = repo.get_by_id(id).await.unwrap();
    assert_eq!(read.metadata, model.metadata);
    assert_eq!(read.image_family, Some(ImageFamily::Sdxl));
    assert_eq!(read.download_date, model.download_date);
    assert_eq!(
        stored(&repo, id, "last_update_check").await.as_deref(),
        Some("yesterday")
    );

    model.last_update_check = Some(Utc::now());
    repo.update(&model).await.unwrap();
    let read = repo.get_by_id(id).await.unwrap();
    assert_eq!(read.last_update_check, model.last_update_check);
}

#[tokio::test]
async fn clearing_readable_defaults_clears_them_and_their_origin() {
    let repo = repo().await;
    let mut model = repo.insert(&make_model("Xi")).await.unwrap();
    model.inference_defaults = Some(temperature(0.4));
    model.defaults_origin = Some(DefaultsOrigin::Published);
    repo.update(&model).await.unwrap();

    model.inference_defaults = None;
    model.defaults_origin = None;
    repo.update(&model).await.unwrap();

    assert_eq!(stored(&repo, model.id, "inference_defaults").await, None);
    assert_eq!(stored(&repo, model.id, "defaults_origin").await, None);
}

/// The edge `update` documents: unreadable defaults already read as none, so
/// an edit that clears them changes nothing it can see, and keeps them.
#[tokio::test]
async fn clearing_unreadable_defaults_keeps_them() {
    let repo = repo().await;
    let id = repo.insert(&make_model("Omicron")).await.unwrap().id;
    plant_unreadable_defaults(&repo, id).await;

    let mut model = repo.get_by_id(id).await.unwrap();
    model.inference_defaults = None;
    model.defaults_origin = None;
    repo.update(&model).await.unwrap();

    assert_eq!(
        stored(&repo, id, "inference_defaults").await.as_deref(),
        Some(r#"{"temperature":"warm"}"#)
    );
    assert_eq!(
        stored(&repo, id, "defaults_origin").await.as_deref(),
        Some("published")
    );
}

/// A field a later build added survives inside defaults this build reads.
#[tokio::test]
async fn an_unknown_field_in_readable_defaults_survives_a_rename() {
    let repo = repo().await;
    let id = repo.insert(&make_model("Pi")).await.unwrap().id;
    let text = r#"{"temperature":0.4,"laterKnob":1}"#;
    plant(&repo, id, "inference_defaults", text).await;

    let mut model = repo.get_by_id(id).await.unwrap();
    assert_eq!(model.inference_defaults, Some(temperature(0.4)));
    model.name = "Renamed".to_owned();
    repo.update(&model).await.unwrap();

    assert_eq!(
        stored(&repo, id, "inference_defaults").await.as_deref(),
        Some(text)
    );
}

/// An origin this build cannot name reads as the user's; new defaults
/// replace it with the origin the edit carries.
#[tokio::test]
async fn new_defaults_rewrite_an_unknown_origin() {
    let repo = repo().await;
    let id = repo.insert(&make_model("Rho")).await.unwrap().id;
    plant(&repo, id, "inference_defaults", r#"{"temperature":0.4}"#).await;
    plant(&repo, id, "defaults_origin", "calibrated").await;

    let mut model = repo.get_by_id(id).await.unwrap();
    assert_eq!(model.defaults_origin, Some(DefaultsOrigin::User));
    model.inference_defaults = Some(temperature(0.7));
    repo.update(&model).await.unwrap();

    assert_eq!(
        stored(&repo, id, "defaults_origin").await.as_deref(),
        Some("user")
    );
}
