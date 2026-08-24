//! Observable events for the llama.cpp pre-built install pipeline.
//!
//! `LlamaProgressEvent` is produced by `download_prebuilt_binaries` and
//! consumed by all three surfaces, each adapting the stream to its own output
//! medium:
//!
//! | Consumer    | Crate         | Output                                                              |
//! |-------------|---------------|---------------------------------------------------------------------|
//! | CLI         | `gglib-cli`   | `indicatif` progress bar via `consume_install_events_cli`            |
//! | Axum        | `gglib-axum`  | SSE stream at `POST /api/config/system/install-llama`               |
//! | Tauri       | `gglib-tauri` | `llama-install-progress` event to the WebView                       |
//!
//! The sender end is a `tokio::sync::mpsc::Sender<LlamaProgressEvent>` with
//! capacity 64. When the sender is dropped the consumer loop terminates
//! naturally.
//!
//! Rate and ETA are measured once, in the producer, by the same
//! [`RateEstimator`](gglib_core::download::RateEstimator) the model-download
//! path uses. Surfaces render [`LlamaProgressEvent::Progress`] and must not
//! derive a rate of their own from successive `downloaded` values — three
//! surfaces each deriving their own is what this event type replaced.
//!
//! The event type is **not** feature-gated: [`LlamaProgressEvent`] and
//! [`InstallPhase`] are imported unconditionally. Only the pipeline that
//! *produces* the events (in `download/`) is gated behind
//! `feature = "prebuilt"`.

use serde::Serialize;

// =============================================================================
// InstallPhase
// =============================================================================

/// A discrete stage within the llama.cpp pre-built install pipeline.
///
/// Phases execute in the order they are listed. [`LlamaProgressEvent::PhaseStarted`]
/// and [`LlamaProgressEvent::PhaseCompleted`] bracket each stage so consumers
/// can drive a multi-step progress indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallPhase {
    /// Deciding whether this platform has a pre-built binary at all.
    CheckAvailability,

    /// Resolving the pinned llama.cpp release and its platform asset on GitHub.
    FetchRelease,

    /// Streaming the release archive to disk.
    Download,

    /// Unpacking `llama-server` and its shared libraries.
    Extract,

    /// Windows + CUDA only: fetching the CUDA runtime DLLs the CUDA build
    /// needs on a machine with no CUDA toolkit installed.
    CudaRuntime,

    /// Confirming the binary landed where the launcher will look for it.
    Verify,
}

impl InstallPhase {
    /// How this phase reads in a one-line progress indicator.
    ///
    /// Lives here rather than in each surface because all three were
    /// inventing their own wording for the same stage.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::CheckAvailability => "Checking platform availability...",
            Self::FetchRelease => "Fetching release information...",
            Self::Download => "Downloading llama.cpp binaries...",
            Self::Extract => "Extracting binaries and libraries...",
            Self::CudaRuntime => "Downloading CUDA runtime libraries...",
            Self::Verify => "Verifying installation...",
        }
    }
}

// =============================================================================
// LlamaProgressEvent
// =============================================================================

/// An observable event emitted by the llama.cpp pre-built install pipeline.
///
/// Every notable state change produces exactly one variant. Consumers decide
/// how to render them: the CLI drives an `indicatif` bar; Axum serialises to
/// `data: <json>\n\n` frames; Tauri emits them to the WebView.
///
/// # Serde tag
///
/// `#[serde(tag = "type", rename_all = "snake_case")]` produces e.g.
/// `{"type":"phase_started","phase":"download"}`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LlamaProgressEvent {
    /// A pipeline stage is beginning.
    PhaseStarted {
        /// The stage that is about to execute.
        phase: InstallPhase,
    },

    /// Bytes moved during [`InstallPhase::Download`].
    Progress {
        /// Bytes written to disk so far.
        downloaded: u64,
        /// Total bytes expected, or `0` when the server sent no length.
        total: u64,
        /// Current throughput in bytes per second.
        ///
        /// Absent until the estimator has warmed up. Deliberately not `0.0`:
        /// zero is a real reading meaning "stalled", and conflating the two is
        /// what rendered `0 B/s` on a healthy download.
        #[serde(skip_serializing_if = "Option::is_none")]
        rate_bps: Option<f64>,
        /// Estimated seconds remaining; absent when not yet known.
        #[serde(skip_serializing_if = "Option::is_none")]
        eta_seconds: Option<f64>,
    },

    /// A pipeline stage has finished successfully.
    PhaseCompleted {
        /// The stage that just finished.
        phase: InstallPhase,
    },

    /// The entire download-and-install pipeline completed successfully.
    Completed {
        /// The llama.cpp release tag that was installed (e.g. `b10327`).
        version: String,
    },

    /// The pipeline terminated with an unrecoverable error.
    Failed {
        /// Human-readable description of the failure.
        message: String,
    },
}
