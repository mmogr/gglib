//! Tests for [`component_choices`].

use std::path::PathBuf;

use chrono::Utc;

use super::*;
use crate::domain::{ImageFamily, ModelComponent, NewModel};

fn model(id: i64, family: Option<ImageFamily>, links: &[(ComponentRole, &str)]) -> Model {
    let mut new = NewModel::new(
        format!("m{id}"),
        PathBuf::from(format!("/models/m{id}.gguf")),
        12.0,
        Utc::now(),
    );
    new.image_family = family;
    new.components = links
        .iter()
        .map(|(role, path)| ModelComponent {
            role: *role,
            path: PathBuf::from(path),
        })
        .collect();
    Model::stored(id, &new)
}

fn paths(list: &[&str]) -> Vec<PathBuf> {
    list.iter().map(PathBuf::from).collect()
}

/// Every role the recipe names gets an entry, in the recipe's order, even
/// one no model links yet.
#[test]
fn each_recipe_role_is_listed_in_the_recipes_order() {
    let this = model(1, Some(ImageFamily::Flux1), &[]);

    let choices = component_choices(&this, std::slice::from_ref(&this));

    assert_eq!(
        choices,
        vec![
            (ComponentRole::Vae, Vec::new()),
            (ComponentRole::ClipL, Vec::new()),
            (ComponentRole::T5xxl, Vec::new()),
        ]
    );
}

/// A file another model of the family links in a role is offered for that
/// role, the model's own link too, each path once, in path order.
#[test]
fn the_files_a_family_links_are_offered_once_each_in_their_role() {
    let this = model(
        1,
        Some(ImageFamily::Flux1),
        &[(ComponentRole::Vae, "/c/ae.safetensors")],
    );
    let library = [
        this.clone(),
        model(
            2,
            Some(ImageFamily::Flux1),
            &[
                (ComponentRole::Vae, "/b/ae.safetensors"),
                (ComponentRole::ClipL, "/b/clip_l.safetensors"),
            ],
        ),
        model(
            3,
            Some(ImageFamily::Flux1),
            &[(ComponentRole::Vae, "/c/ae.safetensors")],
        ),
    ];

    let choices = component_choices(&this, &library);

    assert_eq!(
        choices,
        vec![
            (
                ComponentRole::Vae,
                paths(&["/b/ae.safetensors", "/c/ae.safetensors"])
            ),
            (ComponentRole::ClipL, paths(&["/b/clip_l.safetensors"])),
            (ComponentRole::T5xxl, Vec::new()),
        ]
    );
}

/// Another family's VAE would be refused, so it is never offered; and a
/// chat model's picker has no roles at all.
#[test]
fn another_familys_files_and_a_chat_model_offer_nothing() {
    let qwen = model(
        2,
        Some(ImageFamily::QwenImage21),
        &[(ComponentRole::Vae, "/q/qwen_image_2.1_vae_bf16.safetensors")],
    );
    let flux = model(1, Some(ImageFamily::Flux1), &[]);
    let chat = model(3, None, &[]);
    let library = [flux.clone(), qwen, chat.clone()];

    let offered = component_choices(&flux, &library);

    assert_eq!(offered[0], (ComponentRole::Vae, Vec::new()));
    assert!(component_choices(&chat, &library).is_empty());
}
