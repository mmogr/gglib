//! [`BuiltCore`] — the fully wired output of [`crate::CoreBootstrap::build`].

use std::sync::Arc;

use gglib_core::ports::{
    DownloadManagerPort, GgufParserPort, HfClientPort, ModelRegistrarPort, ProcessRunner, Repos,
};
use gglib_core::services::AppCore;

/// Fully wired infrastructure produced by [`crate::CoreBootstrap::build`].
///
/// All fields are `pub` so adapter bootstrap modules can access every
/// service they need to assemble their own context structs.
pub struct BuiltCore {
    /// Core application facade with verification service attached.
    pub app: Arc<AppCore>,
    /// Process runner for llama-server lifecycle management.
    pub runner: Arc<dyn ProcessRunner>,
    /// Download manager trait object.
    pub downloads: Arc<dyn DownloadManagerPort>,
    /// `HuggingFace` HTTP client.
    pub hf_client: Arc<dyn HfClientPort>,
    /// GGUF file parser for metadata extraction and capability detection.
    pub gguf_parser: Arc<dyn GgufParserPort>,
    /// Repository set (models, settings, MCP servers, chat history).
    ///
    /// Adapters need this to construct the MCP service and other
    /// infrastructure that requires direct repository access.
    pub repos: Repos,
    /// Model registrar shared between the download manager and direct
    /// registration code paths (e.g., CLI `model add` command).
    pub model_registrar: Arc<dyn ModelRegistrarPort>,
}
