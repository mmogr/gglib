//! An image model through the repository: `models.image_family` written,
//! read back, and kept by a re-registration that read none.

use std::path::PathBuf;

use chrono::Utc;
use gglib_core::NewModel;
use gglib_core::domain::ImageFamily;

use super::*;
use crate::setup::setup_test_database;

fn new_model(name: &str) -> NewModel {
    NewModel::new(
        name.to_string(),
        PathBuf::from(format!("/models/{name}.gguf")),
        12.0,
        Utc::now(),
    )
}

async fn repo() -> SqliteModelRepository {
    SqliteModelRepository::new(setup_test_database().await.unwrap())
}

#[tokio::test]
async fn the_family_a_new_model_carries_is_read_back_by_every_read() {
    let repo = repo().await;
    let mut new = new_model("flux1-schnell-q8_0");
    new.image_family = Some(ImageFamily::Flux1);

    let inserted = repo.insert(&new).await.unwrap();

    assert_eq!(inserted.image_family, Some(ImageFamily::Flux1));
    assert!(inserted.generates_images());
    let by_id = repo.get_by_id(inserted.id).await.unwrap();
    assert_eq!(by_id.image_family, Some(ImageFamily::Flux1));
    let by_name = repo.get_by_name("flux1-schnell-q8_0").await.unwrap();
    assert_eq!(by_name.image_family, Some(ImageFamily::Flux1));
    assert_eq!(
        repo.list().await.unwrap()[0].image_family,
        Some(ImageFamily::Flux1)
    );
    let stored: Option<String> = sqlx::query_scalar("SELECT image_family FROM models")
        .fetch_one(&repo.pool)
        .await
        .unwrap();
    assert_eq!(stored.as_deref(), Some("flux1"), "stored by its wire name");
}

#[tokio::test]
async fn a_chat_model_has_no_family() {
    let repo = repo().await;
    let inserted = repo.insert(&new_model("qwen3")).await.unwrap();
    assert_eq!(inserted.image_family, None);
    assert!(!inserted.generates_images());
}

/// A re-registration whose table read no family keeps the stored one, as
/// the projector is kept.
#[tokio::test]
async fn a_re_registration_with_no_family_keeps_the_stored_one() {
    let repo = repo().await;
    let mut new = new_model("qwen_image_2.1-Q8_0");
    new.image_family = Some(ImageFamily::QwenImage21);
    let inserted = repo.insert(&new).await.unwrap();

    let again = repo
        .insert(&new_model("qwen_image_2.1-Q8_0"))
        .await
        .unwrap();

    assert_eq!(again.id, inserted.id);
    assert_eq!(again.image_family, Some(ImageFamily::QwenImage21));
}

#[tokio::test]
async fn update_writes_the_family() {
    let repo = repo().await;
    let mut model = repo.insert(&new_model("sd_xl_base_1.0")).await.unwrap();

    model.image_family = Some(ImageFamily::Sdxl);
    repo.update(&model).await.unwrap();
    assert_eq!(
        repo.get_by_id(model.id).await.unwrap().image_family,
        Some(ImageFamily::Sdxl)
    );

    model.image_family = None;
    repo.update(&model).await.unwrap();
    assert_eq!(repo.get_by_id(model.id).await.unwrap().image_family, None);
}

/// A name a later build wrote reads as no family, never as a failed row.
#[tokio::test]
async fn an_unknown_family_name_reads_as_none() {
    let repo = repo().await;
    let inserted = repo.insert(&new_model("future")).await.unwrap();
    sqlx::query("UPDATE models SET image_family = 'flux3' WHERE id = ?")
        .bind(inserted.id)
        .execute(&repo.pool)
        .await
        .unwrap();
    assert_eq!(
        repo.get_by_id(inserted.id).await.unwrap().image_family,
        None
    );
}
