//! Dependency injection for GuiBackend.
//!
//! All dependencies are injected as trait objects to maintain adapter neutrality.

use std::sync::Arc;

use gglib_core::events::ServerEvents;
use gglib_core::ports::{
    AppEventEmitter, DownloadManagerPort, GgufParserPort, HfClientPort, ModelRepository,
    ProcessRunner, SystemProbePort, ToolSupportDetectorPort, VoicePipelinePort,
};
use gglib_core::services::AppCore;
use gglib_mcp::McpService;
use gglib_runtime::proxy::ProxySupervisor;

/// Dependencies required to construct a `GuiBackend`.
///
/// All fields are private to enforce construction via `GuiDeps::new()`.
/// This prevents partial injection and ensures consistent initialization.
///
/// # Example
///
/// ```ignore
/// let deps = GuiDeps::new(core, downloads, hf, runner, mcp);
/// let backend = GuiBackend::new(deps);
/// ```
pub struct GuiDeps {
    /// Core application facade providing access to domain services.
    pub(crate) core: Arc<AppCore>,
    /// Download manager for queue operations.
    pub(crate) downloads: Arc<dyn DownloadManagerPort>,
    /// HuggingFace client for model discovery.
    pub(crate) hf: Arc<dyn HfClientPort>,
    /// Process runner for server lifecycle management.
    pub(crate) runner: Arc<dyn ProcessRunner>,
    /// MCP service for MCP server management.
    pub(crate) mcp: Arc<McpService>,
    /// Event emitter for application events.
    pub(crate) emitter: Arc<dyn AppEventEmitter>,
    /// Server lifecycle event emitter.
    pub(crate) server_events: Arc<dyn ServerEvents>,
    /// Tool support detector for capability detection.
    pub(crate) tool_detector: Arc<dyn ToolSupportDetectorPort>,
    /// Proxy supervisor for proxy lifecycle management.
    pub(crate) proxy_supervisor: Arc<ProxySupervisor>,
    /// Model repository for catalog access (proxy use).
    pub(crate) model_repo: Arc<dyn ModelRepository>,
    /// System probe for hardware detection and memory info.
    pub(crate) system_probe: Arc<dyn SystemProbePort>,
    /// GGUF parser for file validation and metadata extraction.
    pub(crate) gguf_parser: Arc<dyn GgufParserPort>,
    /// Voice pipeline for data/config operations (Phase 1 ops only).
    pub(crate) voice: Arc<dyn VoicePipelinePort>,
}

impl GuiDeps {
    /// Create a new `GuiDeps` with all required dependencies.
    ///
    /// This is the only way to construct `GuiDeps`, ensuring all
    /// dependencies are provided upfront.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        core: Arc<AppCore>,
        downloads: Arc<dyn DownloadManagerPort>,
        hf: Arc<dyn HfClientPort>,
        runner: Arc<dyn ProcessRunner>,
        mcp: Arc<McpService>,
        emitter: Arc<dyn AppEventEmitter>,
        server_events: Arc<dyn ServerEvents>,
        tool_detector: Arc<dyn ToolSupportDetectorPort>,
        proxy_supervisor: Arc<ProxySupervisor>,
        model_repo: Arc<dyn ModelRepository>,
        system_probe: Arc<dyn SystemProbePort>,
        gguf_parser: Arc<dyn GgufParserPort>,
        voice: Arc<dyn VoicePipelinePort>,
    ) -> Self {
        Self {
            core,
            downloads,
            hf,
            runner,
            mcp,
            emitter,
            server_events,
            tool_detector,
            proxy_supervisor,
            model_repo,
            system_probe,
            gguf_parser,
            voice,
        }
    }

    // =========================================================================
    // Convenience accessors for ops modules
    // =========================================================================

    /// Access the model service.
    pub fn models(&self) -> &gglib_core::services::ModelService {
        self.core.models()
    }

    /// Access the settings service.
    pub fn settings(&self) -> &gglib_core::services::SettingsService {
        self.core.settings()
    }

    /// Access the server service.
    pub fn servers(&self) -> &gglib_core::services::ServerService {
        self.core.servers()
    }

    /// Access the event emitter.
    pub fn emitter(&self) -> &Arc<dyn AppEventEmitter> {
        &self.emitter
    }

    /// Access the server event emitter.
    pub fn server_events(&self) -> &Arc<dyn ServerEvents> {
        &self.server_events
    }

    /// Access the GGUF parser.
    pub fn gguf_parser(&self) -> &Arc<dyn GgufParserPort> {
        &self.gguf_parser
    }
}
