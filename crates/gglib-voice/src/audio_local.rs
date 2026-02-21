//! Local (cpal/rodio) adapters for the [`AudioSource`] and [`AudioSink`] traits.
//!
//! [`LocalAudioSource`] and [`LocalAudioSink`] are thin, zero-overhead wrappers
//! around [`AudioThreadHandle`].  They share a **single** `Arc<AudioThreadHandle>`
//! — the audio OS thread owns both the cpal capture stream and the rodio
//! playback sink, so one handle is all that is needed.
//!
//! [`Arc`] without a `Mutex` is correct here because every method on
//! [`AudioThreadHandle`] takes `&self`; internal state transitions happen on
//! the dedicated OS thread via `std::sync::mpsc` channels.  The proxy struct
//! itself is never mutably borrowed across the `Arc`.
//!
//! # Construction
//!
//! Use [`new_pair`] to create both adapters at once:
//!
//! ```no_run
//! use gglib_voice::audio_local::LocalAudioSource; // illustrative
//! # use gglib_voice::{EchoGate, VoiceError};
//! # use gglib_voice::audio_local::new_pair;
//! let echo_gate = EchoGate::new();
//! let (source, sink) = new_pair(&echo_gate)?;
//! # Ok::<(), VoiceError>(())
//! ```

use std::sync::Arc;

use crate::audio_io::{AudioSink, AudioSource};
use crate::audio_thread::AudioThreadHandle;
use crate::capture::{AudioCapture, AudioDeviceInfo};
use crate::error::VoiceError;
use crate::gate::EchoGate;

// ── LocalAudioSource ───────────────────────────────────────────────

/// Local audio input adapter — delegates to cpal via [`AudioThreadHandle`].
///
/// Created by [`new_pair`].  Shares the underlying handle with the paired
/// [`LocalAudioSink`] — both operate on the same audio OS thread.
pub struct LocalAudioSource {
    handle: Arc<AudioThreadHandle>,
}

impl AudioSource for LocalAudioSource {
    fn start_capture(&self) -> Result<(), VoiceError> {
        self.handle.start_capture()
    }

    fn stop_capture(&self) -> Result<Vec<f32>, VoiceError> {
        self.handle.stop_capture()
    }

    /// Returns `Ok(None)` for the local adapter.
    ///
    /// [`AudioThreadHandle`] does not expose a frame-by-frame polling API —
    /// the cpal stream drains into an internal buffer that is returned in one
    /// shot by [`stop_capture`](AudioSource::stop_capture).  The primary
    /// consumer of `read_vad_frame` is `WebSocketAudioSource` (Phase 3 PR 3).
    fn read_vad_frame(&self) -> Result<Option<Vec<f32>>, VoiceError> {
        Ok(None)
    }

    fn is_capturing(&self) -> bool {
        self.handle.is_recording()
    }

    /// List available audio input devices.
    ///
    /// Delegates to the static [`AudioCapture::list_devices`] from within
    /// this `&self` method, which is legal in Rust and keeps the trait
    /// object-safe.
    fn list_devices(&self) -> Result<Vec<AudioDeviceInfo>, VoiceError> {
        AudioCapture::list_devices()
    }
}

// ── LocalAudioSink ─────────────────────────────────────────────────

/// Local audio output adapter — delegates to rodio via [`AudioThreadHandle`].
///
/// Created by [`new_pair`].  Shares the underlying handle with the paired
/// [`LocalAudioSource`] — both operate on the same audio OS thread.
pub struct LocalAudioSink {
    handle: Arc<AudioThreadHandle>,
}

impl AudioSink for LocalAudioSink {
    fn start_streaming(&self) -> Result<(), VoiceError> {
        self.handle.start_streaming()
    }

    fn append(&self, samples: Vec<f32>, sample_rate: u32) -> Result<(), VoiceError> {
        self.handle.append(samples, sample_rate)
    }

    /// Stop playback immediately.
    ///
    /// [`AudioThreadHandle::stop_playback`] is fire-and-forget (returns `()`);
    /// we wrap it in `Ok(())` to satisfy the `Result`-returning trait signature.
    fn stop(&self) -> Result<(), VoiceError> {
        self.handle.stop_playback();
        Ok(())
    }

    fn is_playing(&self) -> bool {
        self.handle.is_playing()
    }

    fn on_playback_complete(&self, callback: Box<dyn FnOnce() + Send + 'static>) {
        self.handle.spawn_completion_watcher(Some(callback));
    }
}

// ── Constructor ────────────────────────────────────────────────────

/// Spawn one [`AudioThreadHandle`] and return a matched source/sink adapter
/// pair that share it.
///
/// Exactly one OS thread is created for the combined capture + playback
/// pipeline, matching the pre-abstraction architecture.
///
/// # Errors
///
/// Returns [`VoiceError`] if the audio thread fails to start (e.g. no audio
/// device present).
pub fn new_pair(echo_gate: &EchoGate) -> Result<(LocalAudioSource, LocalAudioSink), VoiceError> {
    let handle = Arc::new(AudioThreadHandle::spawn(echo_gate)?);
    let source = LocalAudioSource {
        handle: Arc::clone(&handle),
    };
    let sink = LocalAudioSink { handle };
    Ok((source, sink))
}
