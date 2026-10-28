//! The flag that ends a generation from outside it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Raised to end a generation; a decoder checks it before every step.
///
/// Clones share one flag, so the party that raises it (a client gone, a
/// stop string found, a model being swapped) and the decoder that reads it
/// each hold their own handle. Once raised it stays raised: a new
/// generation takes a new `Abort`.
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
}
