//! `gglib model add --component`: an image model's components linked once
//! its file is added, through the rule `model update` links them by.

use gglib_core::domain::{ComponentRole, ImageFamily};

use super::super::test_library::{
    FLUX_CLIP_L, FLUX_VAE, library, run, write_component, write_flux,
};
use crate::utils::input::TYPED;

/// A Flux.1 file added with `--component` is stored with its family and
/// the component linked; the roles not named stay missing.
#[tokio::test]
async fn an_image_model_is_added_with_its_components_linked() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, _) = library(dir.path()).await;
    let weights = write_flux(dir.path(), "flux1-schnell-q8_0.gguf");
    let vae = write_component(dir.path(), "ae.safetensors", FLUX_VAE);
    let flag = format!("vae={}", vae.display());
    let add = [
        "gglib",
        "model",
        "add",
        weights.to_str().unwrap(),
        "--component",
        &flag,
    ];

    TYPED.scope("12", run(&ctx, &add)).await.unwrap();

    let added = ctx.app.models().find_by_path(&weights).await.unwrap();
    let added = added.expect("the file has a row");
    assert_eq!(added.image_family, Some(ImageFamily::Flux1));
    let linked: Vec<_> = added.components.iter().map(|c| (c.role, &c.path)).collect();
    assert_eq!(linked, [(ComponentRole::Vae, &vae)]);
    assert_eq!(
        added.missing_components(),
        [ComponentRole::ClipL, ComponentRole::T5xxl]
    );
}

/// A refused component leaves the model added, and the error says both:
/// that it was added, under which id, and why the file was refused.
#[tokio::test]
async fn a_refused_component_leaves_the_model_added_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, _) = library(dir.path()).await;
    let weights = write_flux(dir.path(), "flux1-schnell-q8_0.gguf");
    let clip = write_component(dir.path(), "clip_l.safetensors", FLUX_CLIP_L);
    let flag = format!("vae={}", clip.display());
    let add = [
        "gglib",
        "model",
        "add",
        weights.to_str().unwrap(),
        "--component",
        &flag,
    ];

    let refused = TYPED.scope("12", run(&ctx, &add)).await.unwrap_err();

    let added = ctx.app.models().find_by_path(&weights).await.unwrap();
    let added = added.expect("the model is added whatever the component");
    assert!(added.components.is_empty());
    let said = format!("{refused:#}");
    assert!(
        said.contains(&format!("added as id {}", added.id)),
        "{said}"
    );
    assert!(said.contains("is not a vae"), "{said}");
}

/// A role named twice is refused before the file is added.
#[tokio::test]
async fn a_role_named_twice_is_refused_before_the_add() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, _) = library(dir.path()).await;
    let weights = write_flux(dir.path(), "flux1-schnell-q8_0.gguf");
    let add = [
        "gglib",
        "model",
        "add",
        weights.to_str().unwrap(),
        "--component",
        "vae=/a",
        "--no-component",
        "vae",
    ];

    let refused = TYPED.scope("12", run(&ctx, &add)).await.unwrap_err();

    assert!(
        refused.to_string().contains("vae is named twice"),
        "{refused}"
    );
    let found = ctx.app.models().find_by_path(&weights).await.unwrap();
    assert!(found.is_none(), "nothing is added");
}
