//! Observable events for the llama.cpp source-build pipeline.
//!
//! [`BuildEvent`] is produced by the build-from-source pipeline and consumed by three
//! surfaces, each adapting the event stream to its own output medium:
//!
//! | Consumer    | Crate        | Output                                                                    |
//! |-------------|--------------|--------------------------------------------------------------------------|
//! | CLI         | `gglib-cli`  | `indicatif` spinner + progress bar via `consume_build_events_cli`         |
//! | REST / SSE  | `gglib-axum` | Server-Sent Events at `POST /api/system/build-llama-from-source`          |
//! | Desktop GUI | `gglib-tauri`| Tauri event `llama-build-progress` emitted to the WebView                 |
//!
//! The sender end is a `tokio::sync::mpsc::Sender<BuildEvent>` with capacity 64.
//! When the sender is dropped the consumer loop terminates naturally.
//!
//! The event type is **not** feature-gated: all three surfaces import [`BuildEvent`]
//! and [`BuildPhase`] unconditionally. Only the pipeline that *produces* the events
//! (in `build/` and `install/`) is gated behind `feature = "cli"`.

use serde::Serialize;

// =============================================================================
// BuildPhase
// =============================================================================

/// A discrete stage within the llama.cpp source-build pipeline.
///
/// Phases execute in the order they are listed. [`BuildEvent::PhaseStarted`]
/// and [`BuildEvent::PhaseCompleted`] bracket each stage so consumers can
/// drive a multi-step progress indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildPhase {
    /// Checking that cmake, git, and a suitable C++ compiler are present.
    DependencyCheck,

    /// Cloning the llama.cpp repository or pulling the latest commit.
    CloneOrUpdateRepo,

    /// Running `cmake` to configure the build system and detect acceleration.
    Configure,

    /// Running `cmake --build` to compile all translation units.
    Compile,

    /// Copying the built binaries to `~/.local/share/gglib/bin/`.
    InstallBinaries,
}

// =============================================================================
// BuildEvent
// =============================================================================

/// An observable event emitted by the llama.cpp source-build pipeline.
///
/// Events are the unit of SSE emission for the build pipeline. Every notable
/// state change produces exactly one variant. Consumers decide how to render
/// them: the CLI produces `indicatif` progress bars; Axum serialises to
/// `data: <json>\n\n` frames; Tauri emits them to the WebView.
///
/// # Serde tag
///
/// `#[serde(tag = "type", rename_all = "snake_case")]` produces e.g.
/// `{"type":"phase_started","phase":"configure"}`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BuildEvent {
    /// A pipeline stage is beginning.
    PhaseStarted {
        /// The stage that is about to execute.
        phase: BuildPhase,
    },

    /// A raw log line from cmake, the compiler, or git.
    Log {
        /// The unmodified output line from the subprocess.
        message: String,
    },

    /// Compilation progress reported by cmake.
    ///
    /// Both `current` and `total` are file counts derived from cmake output
    /// patterns such as `[ 50%]` (mapped to `50/100`) or `[150/300]`.
    Progress {
        /// Files compiled so far.
        current: u64,
        /// Total files to compile.
        total: u64,
    },

    /// A pipeline stage has finished successfully.
    PhaseCompleted {
        /// The stage that just finished.
        phase: BuildPhase,
    },

    /// The entire build-and-install pipeline completed successfully.
    Completed {
        /// The llama.cpp version or commit SHA that was built.
        version: String,
        /// Human-readable name of the GPU acceleration that was compiled in
        /// (e.g. `"Metal"`, `"CUDA"`, `"CPU"`).
        acceleration: String,
    },

    /// The pipeline terminated with an unrecoverable error.
    Failed {
        /// Human-readable description of the failure.
        message: String,
    },
}
