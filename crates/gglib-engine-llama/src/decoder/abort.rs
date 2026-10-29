//! The flag that ends a generation, or a load, from outside it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Raised to end a generation or a load; a decoder checks it before every
/// step of a generation and at every report of a load's progress.
///
/// Clones share one flag, so the party that raises it (a client gone, a
/// stop string found, a model being swapped, a runtime shutting down) and
/// the decoder that reads it each hold their own handle. Once raised it
/// stays raised: a new generation takes a new `Abort`.
#[derive(Debug, Clone, Default)]
pub struct Abort(Arc<AtomicBool>);

impl Abort {
    /// A flag not yet raised.
    pub fn new() -> Self {
        Self::default()
    }

    /// Raise it.
    pub fn raise(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Whether it has been raised.
    pub fn is_raised(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    /// The flag itself, where llama.cpp's abort callback reads it. It lives
    /// as long as any clone of this `Abort`.
    #[cfg(gglib_llama)]
    pub(crate) fn as_ptr(&self) -> *const AtomicBool {
        Arc::as_ptr(&self.0)
    }
}
