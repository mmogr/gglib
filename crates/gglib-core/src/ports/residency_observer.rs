//! Told of every model that becomes resident and every one that stops
//! being resident, whoever caused it.

use std::fmt;

use super::process_runner::ProcessHandle;

/// Told of every change to what is resident, whoever caused it.
///
/// A start from the GUI or the CLI, the proxy swapping one model out for
/// another on a request, a resident recycled for a failed health check or
/// a changed launch, an image model retired after its render: each reaches
/// the observer. The resident set calls it after the queue has recorded the
/// change, with no lock held, and before the process is stopped where the
/// order is its to choose. Must not block: a slow observer delays the
/// admission or the stop it describes.
pub trait ResidencyObserver: Send + Sync + fmt::Debug {
    /// `server` is resident and answered its health check.
    fn loaded(&self, server: ProcessHandle);

    /// Model `model_id` is no longer resident: its server is about to be
    /// stopped, or, for a render's retirement, was stopped a moment ago.
    /// Every model this is told of was told [`Self::loaded`] first.
    fn unloaded(&self, model_id: u32);
}

/// Told nothing: for tests, and a runtime nobody listens to.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopResidencyObserver;

impl ResidencyObserver for NoopResidencyObserver {
    fn loaded(&self, _server: ProcessHandle) {}

    fn unloaded(&self, _model_id: u32) {}
}
