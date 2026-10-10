//! `--component` and `--no-component` against a real library and real
//! weights headers, from the command line to the stored row, and the
//! preview's lines.

use std::collections::BTreeMap;
use std::path::PathBuf;

use gglib_core::domain::{ComponentRole, ModelComponent};

use super::super::test_library::{
    FLUX_CLIP_L, FLUX_T5XXL, FLUX_VAE, image_library, library, run, stored, write_component,
};
use super::*;

/// Each `--component` links its role through `set_component`, beside the
/// rest of the same update; the roles not named stay missing.
#[tokio::test]
async fn each_component_is_linked_and_the_rest_of_the_update_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = image_library(dir.path()).await;
    let vae = write_component(dir.path(), "ae.safetensors", FLUX_VAE);
    let t5 = write_component(dir.path(), "t5xxl_fp16.safetensors", FLUX_T5XXL);
    let id = model.id.to_string();
    assert_eq!(
        model.image_family,
        Some(gglib_core::domain::ImageFamily::Flux1)
    );

    run(
        &ctx,
        &[
            "gglib",
            "model",
            "update",
            &id,
            "--force",
            "--name",
            "Flux",
            "--component",
            &format!("vae={}", vae.display()),
            "--component",
            &format!("t5xxl={}", t5.display()),
        ],
    )
    .await
    .unwrap();

    let row = stored(&ctx, model.id).await;
    assert_eq!(row.name, "Flux");
    assert_eq!(
        row.components,
        [
            ModelComponent {
                role: ComponentRole::Vae,
                path: vae
            },
            ModelComponent {
                role: ComponentRole::T5xxl,
                path: t5
            },
        ]
    );
    assert_eq!(row.missing_components(), [ComponentRole::ClipL]);
}

/// `--no-component` unlinks its role and leaves the others linked.
#[tokio::test]
async fn no_component_unlinks_its_role_only() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = image_library(dir.path()).await;
    let vae = write_component(dir.path(), "ae.safetensors", FLUX_VAE);
    let clip = write_component(dir.path(), "clip_l.safetensors", FLUX_CLIP_L);
    let id = model.id.to_string();
    let update = ["gglib", "model", "update", &id, "--force"];
    let vae_flag = format!("vae={}", vae.display());
    let clip_flag = format!("clip_l={}", clip.display());
    let link = [
        &update[..],
        &["--component", &vae_flag, "--component", &clip_flag],
    ]
    .concat();
    run(&ctx, &link).await.unwrap();

    run(&ctx, &[&update[..], &["--no-component", "vae"]].concat())
        .await
        .unwrap();

    let row = stored(&ctx, model.id).await;
    let roles: Vec<_> = row.components.iter().map(|c| c.role).collect();
    assert_eq!(roles, [ComponentRole::ClipL]);
    assert_eq!(
        row.missing_components(),
        [ComponentRole::Vae, ComponentRole::T5xxl]
    );
}

/// A file whose tensors are another role's is refused in the rule's own
/// words, naming the file, and nothing is linked.
#[tokio::test]
async fn a_file_of_another_role_is_refused_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = image_library(dir.path()).await;
    let clip = write_component(dir.path(), "clip_l.safetensors", FLUX_CLIP_L);
    let id = model.id.to_string();
    let flag = format!("t5xxl={}", clip.display());

    let refused = run(
        &ctx,
        &[
            "gglib",
            "model",
            "update",
            &id,
            "--force",
            "--component",
            &flag,
        ],
    )
    .await
    .unwrap_err()
    .to_string();

    assert!(refused.contains("clip_l.safetensors"), "{refused}");
    assert!(refused.contains("is not a t5xxl"), "{refused}");
    assert!(stored(&ctx, model.id).await.components.is_empty());
}

/// A model that chats has no components to link, and says so.
#[tokio::test]
async fn a_chat_model_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let vae = write_component(dir.path(), "ae.safetensors", FLUX_VAE);
    let id = model.id.to_string();
    let flag = format!("vae={}", vae.display());

    let refused = run(
        &ctx,
        &[
            "gglib",
            "model",
            "update",
            &id,
            "--force",
            "--component",
            &flag,
        ],
    )
    .await
    .unwrap_err()
    .to_string();

    assert!(refused.contains("not an image model"), "{refused}");
    assert!(stored(&ctx, model.id).await.components.is_empty());
}

fn model_with(components: &[(ComponentRole, &str)]) -> Model {
    let mut new = gglib_core::NewModel::new(
        "flux".to_owned(),
        PathBuf::from("/models/flux.gguf"),
        12.0,
        chrono::Utc::now(),
    );
    new.image_family = Some(gglib_core::domain::ImageFamily::Flux1);
    new.components = components
        .iter()
        .map(|(role, path)| ModelComponent {
            role: *role,
            path: PathBuf::from(path),
        })
        .collect();
    Model::stored(1, &new)
}

/// The preview names each role whose link changes, old and new, under one
/// heading.
#[test]
fn the_preview_names_each_changed_link() {
    let linked = model_with(&[(ComponentRole::Vae, "/m/ae.safetensors")]);
    let changes = BTreeMap::from([
        (ComponentRole::Vae, None),
        (ComponentRole::T5xxl, Some(Path::new("/m/t5.safetensors"))),
    ]);

    assert_eq!(
        preview(&linked, &changes),
        [
            "  Components:",
            "    vae: /m/ae.safetensors → --",
            "    t5xxl: -- → /m/t5.safetensors",
        ]
    );
}

/// A change to what is already there, or no change at all, previews
/// nothing, not even the heading.
#[test]
fn a_change_that_changes_nothing_previews_nothing() {
    let linked = model_with(&[(ComponentRole::Vae, "/m/ae.safetensors")]);

    let same = BTreeMap::from([(ComponentRole::Vae, Some(Path::new("/m/ae.safetensors")))]);
    assert!(preview(&linked, &same).is_empty());
    let unlinked = BTreeMap::from([(ComponentRole::ClipL, None)]);
    assert!(preview(&linked, &unlinked).is_empty());
    assert!(preview(&linked, &BTreeMap::new()).is_empty());
}
