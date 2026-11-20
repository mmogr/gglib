//! The library's only chat model: the model a question is asked when it
//! names none and the settings name no default.
//!
//! A model chats when it draws no images ([`Model::generates_images`]).
//! With no chat model, or with several, there is no only one, and the
//! caller says what it says without this rule.

use super::ModelService;
use crate::domain::Model;
use crate::ports::CoreError;

/// The one of `models` that chats, when exactly one does.
fn the_only_chat_model(models: Vec<Model>) -> Option<Model> {
    let mut chat = models.into_iter().filter(|m| !m.generates_images());
    match (chat.next(), chat.next()) {
        (Some(only), None) => Some(only),
        _ => None,
    }
}

impl ModelService {
    /// The library's only chat model, or `None` when it holds none or
    /// several. Image models are not counted: a library of one chat model
    /// and any number of image models has an only chat model.
    ///
    /// # Errors
    ///
    /// The library could not be read.
    pub async fn only_chat_model(&self) -> Result<Option<Model>, CoreError> {
        Ok(the_only_chat_model(self.list().await?))
    }
}

#[cfg(test)]
#[path = "only_chat_model_tests.rs"]
mod only_chat_model_tests;
