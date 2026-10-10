//! An image model through the repository: `models.image_family` and the
//! `model_components` rows written, read back by every read, kept by a
//! re-registration, replaced by an update, and gone with their model.

use std::path::PathBuf;

use chrono::Utc;
use gglib_core::NewModel;
use gglib_core::domain::{ComponentRole, ImageFamily, ModelComponent};

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

// ── Components ──────────────────────────────────────────────────────────────

fn link(role: ComponentRole, path: &str) -> ModelComponent {
    ModelComponent {
        role,
        path: PathBuf::from(path),
    }
}

fn flux(name: &str, components: Vec<ModelComponent>) -> NewModel {
    let mut new = new_model(name);
    new.image_family = Some(ImageFamily::Flux1);
    new.components = components;
    new
}

/// The links of model `id`, read back by every read the repository has.
async fn read_by_every_door(repo: &SqliteModelRepository, id: i64) -> Vec<ModelComponent> {
    let by_id = repo.get_by_id(id).await.unwrap();
    let by_name = repo.get_by_name(&by_id.name).await.unwrap();
    let by_path = repo.find_by_path(&by_id.file_path).await.unwrap().unwrap();
    let listed = repo
        .list()
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.id == id)
        .unwrap();
    assert_eq!(by_name.components, by_id.components);
    assert_eq!(by_path.components, by_id.components);
    assert_eq!(listed.components, by_id.components);
    by_id.components
}

#[tokio::test]
async fn components_a_new_model_carries_are_read_back_in_role_order() {
    let repo = repo().await;
    let inserted = repo
        .insert(&flux(
            "flux",
            vec![
                link(ComponentRole::T5xxl, "/models/t5xxl_fp16.safetensors"),
                link(ComponentRole::Vae, "/models/ae.safetensors"),
                link(ComponentRole::ClipL, "/models/clip_l.safetensors"),
            ],
        ))
        .await
        .unwrap();

    let expected = vec![
        link(ComponentRole::Vae, "/models/ae.safetensors"),
        link(ComponentRole::ClipL, "/models/clip_l.safetensors"),
        link(ComponentRole::T5xxl, "/models/t5xxl_fp16.safetensors"),
    ];
    assert_eq!(inserted.components, expected);
    assert_eq!(read_by_every_door(&repo, inserted.id).await, expected);
    assert!(inserted.missing_components().is_empty());
}

/// The listing reads every model's rows in one query and gives each model
/// its own.
#[tokio::test]
async fn the_listing_gives_each_model_its_own_components() {
    let repo = repo().await;
    let a = repo
        .insert(&flux(
            "a",
            vec![link(ComponentRole::Vae, "/models/a-vae.safetensors")],
        ))
        .await
        .unwrap();
    let b = repo
        .insert(&flux(
            "b",
            vec![link(ComponentRole::ClipL, "/models/b-clip.safetensors")],
        ))
        .await
        .unwrap();
    let chat = repo.insert(&new_model("chat")).await.unwrap();

    let listed = repo.list().await.unwrap();
    let of = |id: i64| {
        listed
            .iter()
            .find(|m| m.id == id)
            .unwrap()
            .components
            .clone()
    };
    assert_eq!(
        of(a.id),
        vec![link(ComponentRole::Vae, "/models/a-vae.safetensors")]
    );
    assert_eq!(
        of(b.id),
        vec![link(ComponentRole::ClipL, "/models/b-clip.safetensors")]
    );
    assert_eq!(of(chat.id), vec![]);
}

/// A re-registration (a repair downloads the model again) keeps the link
/// the owner chose for a role and adds the roles it had none for.
#[tokio::test]
async fn a_re_registration_keeps_a_held_link_and_adds_the_rest() {
    let repo = repo().await;
    let chosen = link(ComponentRole::Vae, "/models/my-own-ae.safetensors");
    let inserted = repo
        .insert(&flux("flux", vec![chosen.clone()]))
        .await
        .unwrap();

    let again = repo
        .insert(&flux(
            "flux",
            vec![
                link(ComponentRole::Vae, "/models/ae.safetensors"),
                link(ComponentRole::ClipL, "/models/clip_l.safetensors"),
            ],
        ))
        .await
        .unwrap();

    assert_eq!(again.id, inserted.id);
    assert_eq!(
        again.components,
        vec![
            chosen,
            link(ComponentRole::ClipL, "/models/clip_l.safetensors")
        ]
    );
}

#[tokio::test]
async fn update_replaces_the_set() {
    let repo = repo().await;
    let mut model = repo
        .insert(&flux(
            "flux",
            vec![
                link(ComponentRole::Vae, "/models/ae.safetensors"),
                link(ComponentRole::ClipL, "/models/clip_l.safetensors"),
            ],
        ))
        .await
        .unwrap();

    model.components = vec![link(ComponentRole::T5xxl, "/models/t5xxl_fp16.safetensors")];
    repo.update(&model).await.unwrap();
    assert_eq!(
        read_by_every_door(&repo, model.id).await,
        vec![link(ComponentRole::T5xxl, "/models/t5xxl_fp16.safetensors")]
    );

    model.components.clear();
    repo.update(&model).await.unwrap();
    assert_eq!(read_by_every_door(&repo, model.id).await, vec![]);
}

/// An update of a model that is not there changes no rows of its own
/// table either.
#[tokio::test]
async fn an_update_of_a_missing_model_writes_no_component() {
    let repo = repo().await;
    let mut model = repo.insert(&flux("flux", vec![])).await.unwrap();
    repo.delete(model.id).await.unwrap();
    model.components = vec![link(ComponentRole::Vae, "/models/ae.safetensors")];

    let result = repo.update(&model).await;

    assert!(
        matches!(result, Err(RepositoryError::NotFound(_))),
        "{result:?}"
    );
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM model_components")
        .fetch_one(&repo.pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

/// Removing a model removes its rows; a file another model links stays
/// linked there.
#[tokio::test]
async fn a_delete_cascades_and_a_shared_path_survives() {
    let repo = repo().await;
    let shared = link(ComponentRole::T5xxl, "/models/t5xxl_fp16.safetensors");
    let schnell = repo
        .insert(&flux("schnell", vec![shared.clone()]))
        .await
        .unwrap();
    let dev = repo
        .insert(&flux("dev", vec![shared.clone()]))
        .await
        .unwrap();

    repo.delete(schnell.id).await.unwrap();

    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM model_components WHERE model_id = ?")
        .bind(schnell.id)
        .fetch_one(&repo.pool)
        .await
        .unwrap();
    assert_eq!(left, 0, "the removed model's rows go with it");
    assert_eq!(read_by_every_door(&repo, dev.id).await, vec![shared]);
}

/// Paths stored resolved, as the projector's is, through a symlinked folder.
#[cfg(unix)]
mod through_a_symlink {
    use super::*;

    /// A folder `real` holding a VAE, and `via`, a symlink to it.
    fn folders(dir: &std::path::Path) -> (PathBuf, PathBuf) {
        let real = dir.join("real");
        std::fs::create_dir(&real).unwrap();
        std::fs::write(real.join("ae.safetensors"), b"vae").unwrap();
        let via = dir.join("via");
        std::os::unix::fs::symlink(&real, &via).unwrap();
        (real, via)
    }

    #[tokio::test]
    async fn an_inserted_component_path_is_stored_resolved() {
        let dir = tempfile::tempdir().unwrap();
        let (real, via) = folders(dir.path());
        let repo = repo().await;

        let inserted = repo
            .insert(&flux(
                "flux",
                vec![link(
                    ComponentRole::Vae,
                    via.join("ae.safetensors").to_str().unwrap(),
                )],
            ))
            .await
            .unwrap();

        let resolved = std::fs::canonicalize(real.join("ae.safetensors")).unwrap();
        assert_eq!(inserted.components[0].path, resolved);
    }

    #[tokio::test]
    async fn an_updated_component_path_is_stored_resolved() {
        let dir = tempfile::tempdir().unwrap();
        let (real, via) = folders(dir.path());
        let repo = repo().await;
        let mut model = repo.insert(&flux("flux", vec![])).await.unwrap();

        model.components = vec![link(
            ComponentRole::Vae,
            via.join("ae.safetensors").to_str().unwrap(),
        )];
        repo.update(&model).await.unwrap();

        let resolved = std::fs::canonicalize(real.join("ae.safetensors")).unwrap();
        assert_eq!(
            repo.get_by_id(model.id).await.unwrap().components[0].path,
            resolved
        );
    }
}
