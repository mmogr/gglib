//! Who may be asked to chat: a model served by llama.cpp, never one that
//! draws.
//!
//! An image model is served by stable-diffusion.cpp's `sd-server`, which
//! has no chat endpoint, so every door that admits a model for chat asks
//! [`refuse_unless_chats`] first, before anything is loaded or evicted: the
//! proxy's chat completions, a device's turn on a hub chat, the web agent
//! run, and `gglib chat` / `gglib q`. The code and the words are held here
//! once, so each door refuses alike.

use crate::domain::RuntimeKind;

/// The refusal of a chat for a model that draws images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrawsImages;

impl DrawsImages {
    /// The error code a client matches on.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        "image_model_cannot_chat"
    }

    /// What the user is told: which model, and why.
    #[must_use]
    pub fn message(&self, model: &str) -> String {
        format!(
            "Model '{model}' is an image model: it draws images and cannot serve chat \
             completions. Name a chat model here."
        )
    }
}

/// Refuse a chat for a model served by any runtime but llama.cpp.
///
/// `runtime` is the model's (`Model::runtime`, `ModelLaunchSpec::runtime`,
/// or a running server's).
///
/// # Errors
///
/// [`DrawsImages`] for a model that draws.
pub const fn refuse_unless_chats(runtime: RuntimeKind) -> Result<(), DrawsImages> {
    match runtime {
        RuntimeKind::Llama => Ok(()),
        RuntimeKind::StableDiffusion => Err(DrawsImages),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_llama_model_may_chat() {
        assert_eq!(refuse_unless_chats(RuntimeKind::Llama), Ok(()));
        assert_eq!(
            refuse_unless_chats(RuntimeKind::StableDiffusion),
            Err(DrawsImages)
        );
    }

    #[test]
    fn the_refusal_names_the_model_and_its_code() {
        assert_eq!(DrawsImages.code(), "image_model_cannot_chat");
        assert_eq!(
            DrawsImages.message("flux"),
            "Model 'flux' is an image model: it draws images and cannot serve chat \
             completions. Name a chat model here."
        );
    }
}
