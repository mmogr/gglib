//! Which model is the library's only chat model.

use chrono::Utc;

use super::the_only_chat_model;
use crate::domain::{ImageFamily, Model, NewModel};

/// A stored chat model `id`.
fn chats(id: i64) -> Model {
    let new = NewModel::new(
        format!("chat-{id}"),
        format!("/models/chat-{id}.gguf").into(),
        Some(8.0),
        Utc::now(),
    );
    Model::stored(id, &new)
}

/// A stored image model `id`.
fn draws(id: i64) -> Model {
    let mut model = chats(id);
    model.image_family = Some(ImageFamily::Flux1);
    model
}

#[test]
fn a_library_of_one_chat_model_has_an_only_one() {
    let only = the_only_chat_model(vec![chats(4)]).expect("the only one");
    assert_eq!(only.id, 4);
}

#[test]
fn image_models_are_not_counted() {
    let only = the_only_chat_model(vec![draws(1), chats(2), draws(3)]).expect("the only one");
    assert_eq!(only.id, 2);
}

#[test]
fn no_chat_model_or_several_is_no_only_one() {
    assert!(the_only_chat_model(Vec::new()).is_none());
    assert!(the_only_chat_model(vec![draws(1)]).is_none());
    assert!(the_only_chat_model(vec![chats(1), chats(2)]).is_none());
}
