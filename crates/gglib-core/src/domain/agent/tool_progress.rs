//! How a running tool says how far it has got: [`ToolProgressUpdate`], sent
//! to a [`ToolProgressSink`], with an optional [`PreviewFrame`].
//!
//! A tool that takes minutes (an image render) reports its stage and step so
//! a person sees it moving. The agent loop turns each update into an
//! [`AgentEvent::ToolProgress`](super::AgentEvent::ToolProgress), and a
//! preview into an [`AgentEvent::ToolPreview`](super::AgentEvent::ToolPreview),
//! which is shown and never logged.

use std::sync::Arc;

use serde::{Serialize, Serializer};

/// Where a long-running tool has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStage {
    /// Waiting in line behind other work.
    Queued,
    /// Loading what it needs (a model) before the first step.
    Loading,
    /// Stepping through its work: see `pass`, `done` and `total`.
    Sampling,
    /// The last step is done; turning the result into its output.
    Decoding,
    /// Storing and returning what it made.
    Finishing,
}

/// A small preview of an image being made, as base64 PNG bytes.
///
/// The bytes are shared, so passing a frame from the tool to every reader of
/// a run copies a pointer, not the image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewFrame {
    /// The frame's media type; always `"image/png"`.
    pub mime: &'static str,
    /// The step this frame shows.
    pub step: u32,
    /// How many steps the pass takes.
    pub total: u32,
    /// The frame's bytes, base64.
    #[serde(serialize_with = "serialize_shared_str")]
    pub b64: Arc<str>,
}

impl PreviewFrame {
    /// A PNG frame at `step` of `total`.
    #[must_use]
    pub fn png(step: u32, total: u32, b64: impl Into<Arc<str>>) -> Self {
        Self {
            mime: "image/png",
            step,
            total,
            b64: b64.into(),
        }
    }
}

fn serialize_shared_str<S: Serializer>(value: &Arc<str>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(value)
}

/// One report from a running tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolProgressUpdate {
    /// Where the tool has got to.
    pub stage: ToolStage,
    /// Which pass is running, 1-based, when the work has several (one per image).
    pub pass: Option<u32>,
    /// Steps done in this pass.
    pub done: Option<u32>,
    /// Steps this pass takes.
    pub total: Option<u32>,
    /// Place in line while [`ToolStage::Queued`], 1 being next.
    pub position: Option<u32>,
    /// The latest preview, when the tool has one.
    pub preview: Option<PreviewFrame>,
}

impl ToolProgressUpdate {
    /// An update that says only the stage.
    #[must_use]
    pub const fn stage(stage: ToolStage) -> Self {
        Self {
            stage,
            pass: None,
            done: None,
            total: None,
            position: None,
            preview: None,
        }
    }
}

/// Where a running tool sends its [`ToolProgressUpdate`]s.
///
/// Called from inside the tool's work, so it must never block or await: an
/// implementation that cannot keep up drops updates rather than slowing the
/// tool down.
pub trait ToolProgressSink: Send + Sync {
    /// The tool has got this far.
    fn progress(&self, update: ToolProgressUpdate);
}

/// A sink that drops every update: for a caller with nobody to tell.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoProgress;

impl ToolProgressSink for NoProgress {
    fn progress(&self, _update: ToolProgressUpdate) {}
}
