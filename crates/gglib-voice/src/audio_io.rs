//! `AudioSource` and `AudioSink` trait abstractions for voice pipeline audio I/O.
//!
//! These traits decouple the [`VoicePipeline`](crate::pipeline::VoicePipeline)
//! from any specific audio backend, enabling different backends to be injected
//! at runtime:
//!
//! | Implementor | Where used |
//! |---|---|
//! | [`LocalAudioSource`](crate::audio_local::LocalAudioSource) / [`LocalAudioSink`](crate::audio_local::LocalAudioSink) | Desktop / CLI — cpal capture + rodio playback on the local machine |
//! | `WebSocketAudioSource` / `WebSocketAudioSink` | WebUI — PCM16 LE streaming via browser WebSocket |
//!
//! Both traits are **object-safe** (`Box<dyn AudioSource>` / `Box<dyn AudioSink>`).
//! All methods take `&self` to satisfy the object-safety requirement; interior
//! mutability (channels, atomic flags) handles state changes inside each
//! implementation.

use crate::capture::AudioDeviceInfo;
use crate::error::VoiceError;

// ── AudioSource ────────────────────────────────────────────────────

/// Abstraction over an audio input source (microphone capture).
///
/// # Object safety
/// All methods take `&self`, so the trait is object-safe and usable as
/// `Box<dyn AudioSource>` inside [`VoicePipeline`](crate::pipeline::VoicePipeline).
///
/// # Implementations
/// - [`LocalAudioSource`](crate::audio_local::LocalAudioSource) — wraps
///   [`AudioThreadHandle`](crate::audio_thread::AudioThreadHandle) → cpal
/// - `WebSocketAudioSource` — receives PCM from the browser via WS
pub trait AudioSource: Send + Sync {
    /// Begin capturing audio from the source.
    ///
    /// For local audio this activates the cpal stream; for the WebSocket source
    /// it arms the sample accumulation buffer.
    fn start_capture(&self) -> Result<(), VoiceError>;

    /// Stop capturing and return all accumulated 16 kHz mono f32 PCM samples.
    ///
    /// The returned buffer starts from just after the most recent call to
    /// [`start_capture`](AudioSource::start_capture).
    fn stop_capture(&self) -> Result<Vec<f32>, VoiceError>;

    /// Read a single VAD frame (continuous VAD mode).
    ///
    /// Returns `Ok(Some(frame))` if a new frame is available, `Ok(None)` if
    /// nothing is ready yet, or an `Err` if the source has died.
    ///
    /// The local adapter always returns `Ok(None)` because the cpal audio
    /// thread does not expose a frame-by-frame polling API.  The primary
    /// consumer of this method is `WebSocketAudioSource`.
    fn read_vad_frame(&self) -> Result<Option<Vec<f32>>, VoiceError>;

    /// Whether audio is currently being captured.
    fn is_capturing(&self) -> bool;

    /// List available audio input devices.
    ///
    /// Takes `&self` to preserve object safety.  The local implementation
    /// delegates to the static [`AudioCapture::list_devices`] from within the
    /// instance method body, which is legal in Rust.  The WebSocket
    /// implementation returns `Ok(vec![])` — the browser handles device
    /// enumeration.
    ///
    /// [`AudioCapture::list_devices`]: crate::capture::AudioCapture::list_devices
    fn list_devices(&self) -> Result<Vec<AudioDeviceInfo>, VoiceError>;
}

// ── AudioSink ──────────────────────────────────────────────────────

/// Abstraction over an audio output sink (TTS playback).
///
/// # Object safety
/// All methods take `&self`, so the trait is object-safe and usable as
/// `Box<dyn AudioSink>` inside [`VoicePipeline`](crate::pipeline::VoicePipeline).
///
/// # Implementations
/// - [`LocalAudioSink`](crate::audio_local::LocalAudioSink) — wraps
///   [`AudioThreadHandle`](crate::audio_thread::AudioThreadHandle) → rodio
/// - `WebSocketAudioSink` — encodes f32 → PCM16 LE and sends to
///   the browser via WebSocket
pub trait AudioSink: Send + Sync {
    /// Prepare the sink for streaming playback.
    ///
    /// Creates a fresh playback sink and activates the echo gate so that mic
    /// capture is suppressed during playback.
    fn start_streaming(&self) -> Result<(), VoiceError>;

    /// Append audio samples to the playback buffer.
    ///
    /// For the local sink this queues samples in the rodio sink; for the
    /// WebSocket sink it converts to PCM16 LE and sends over the wire.
    fn append(&self, samples: Vec<f32>, sample_rate: u32) -> Result<(), VoiceError>;

    /// Stop playback immediately.
    fn stop(&self) -> Result<(), VoiceError>;

    /// Whether audio is currently playing.
    fn is_playing(&self) -> bool;

    /// Register a one-shot callback that fires when all queued audio drains.
    ///
    /// `callback` must be `Send + 'static` because it is dispatched from a
    /// background watcher thread/task.
    fn on_playback_complete(&self, callback: Box<dyn FnOnce() + Send + 'static>);
}
