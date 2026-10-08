//! One counter per file.
//!
//! A transport reports what it sees as a [`RawProgress`]: bytes on disk and
//! bytes off the network, which are different numbers. [`FileCounter`] turns
//! those readings into the one [`FileProgress`] every consumer is shown.

use std::sync::{Arc, Mutex, PoisonError};

/// How far one file has got.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileProgress {
    /// Bytes of the file on disk. Never falls, except once when the
    /// accelerator fails and the native transport starts the file again.
    /// Reaches `size` only when the file is complete.
    pub bytes: u64,
    /// Bytes received from the network for this file, over every attempt of
    /// this run. Never falls. Bytes found already on disk are not in it.
    pub wire: u64,
    /// The file's size, when known.
    pub size: Option<u64>,
}

/// Sink for a file's progress.
pub type ProgressCallback = Arc<dyn Fn(FileProgress) + Send + Sync>;

/// One reading from a transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawProgress {
    /// Bytes of the file on disk, counting any an earlier run left there.
    pub written: u64,
    /// Bytes this attempt has received from the network.
    pub received: u64,
    /// The file's size as the transport knows it.
    pub total: Option<u64>,
}

impl RawProgress {
    /// A reading of `written` bytes on disk and `received` off the network.
    #[must_use]
    pub const fn new(written: u64, received: u64, total: Option<u64>) -> Self {
        Self {
            written,
            received,
            total,
        }
    }
}

/// Sink a transport reports its readings to.
pub type RawCallback = Arc<dyn Fn(RawProgress) + Send + Sync>;

/// A size of zero is a size nobody knows: `HuggingFace` metadata that carries
/// no size arrives as 0.
pub(crate) fn known_size(size: Option<u64>) -> Option<u64> {
    size.filter(|&bytes| bytes > 0)
}

/// Turns one file's transport readings into its [`FileProgress`].
pub(crate) struct FileCounter {
    state: Mutex<State>,
    sink: Option<ProgressCallback>,
}

#[derive(Default)]
struct State {
    progress: FileProgress,
    /// Network bytes of the attempts before the current one.
    wire_base: u64,
    /// The next reading may lower `bytes`: set by [`FileCounter::restart`].
    rewind: bool,
}

impl FileCounter {
    /// A counter for a file of `expected_size`, reporting to `sink`.
    pub(crate) fn new(expected_size: Option<u64>, sink: Option<ProgressCallback>) -> Self {
        let progress = FileProgress {
            size: known_size(expected_size),
            ..FileProgress::default()
        };
        Self {
            state: Mutex::new(State {
                progress,
                ..State::default()
            }),
            sink,
        }
    }

    /// Take a reading from the transport.
    ///
    /// `bytes` follows `written` alone and stops one byte short of the size:
    /// a transport can have written every byte of a file that then fails its
    /// check, and only [`finish`](Self::finish) may say it is complete.
    pub(crate) fn observe(&self, raw: RawProgress) {
        self.update(|state| {
            let progress = &mut state.progress;
            progress.size = progress.size.or_else(|| known_size(raw.total));

            let written = progress
                .size
                .map_or(raw.written, |size| raw.written.min(size.saturating_sub(1)));
            progress.bytes = if std::mem::take(&mut state.rewind) {
                written
            } else {
                progress.bytes.max(written)
            };
            progress.wire = progress.wire.max(state.wire_base + raw.received);
        });
    }

    /// The transport failed and another starts the file again.
    ///
    /// The network count carries on from where it stood. The new transport
    /// may have none of the old one's bytes on disk, so its first reading is
    /// taken as it comes, even when that is lower.
    pub(crate) fn restart(&self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.wire_base = state.progress.wire;
        state.rewind = true;
    }

    /// The file is complete and `len` bytes long on disk.
    pub(crate) fn finish(&self, len: u64) {
        self.update(|state| {
            state.progress.bytes = len;
            state.progress.size = Some(len);
        });
    }

    /// The reader a transport reports to.
    pub(crate) fn raw_callback(self: &Arc<Self>) -> RawCallback {
        let counter = Arc::clone(self);
        Arc::new(move |raw| counter.observe(raw))
    }

    fn update(&self, change: impl FnOnce(&mut State)) {
        let progress = {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            change(&mut state);
            state.progress
        };
        if let Some(sink) = self.sink.as_ref() {
            sink(progress);
        }
    }
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
