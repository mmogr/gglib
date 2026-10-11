//! What an `sd-server` launch is given: [`SdServerConfig`].
//!
//! The image runtime's twin of `gglib_core::ports::ServerConfig`, and much
//! smaller: `sd-server` takes the model's files and the port, and everything
//! else it needs comes from the family's recipe.

use std::path::PathBuf;

use gglib_core::domain::{ImageFamily, ModelComponent};

/// An `sd-server` launch for one image model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdServerConfig {
    /// Database ID of the model to serve.
    pub model_id: i64,
    /// Human-readable model name.
    pub model_name: String,
    /// Path to the model's main weights file.
    pub model_path: PathBuf,
    /// The family it draws as, whose recipe says how the main file is loaded
    /// and gives the steps, guidance, sampler and flash attention.
    pub family: ImageFamily,
    /// The files it draws with beside its weights, each in its role, in any
    /// order.
    pub components: Vec<ModelComponent>,
    /// Port to listen on; `None` takes the next free one from the base port.
    pub port: Option<u16>,
}
