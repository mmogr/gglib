//! Audio playback module — TTS output via `rodio`.
//!
//! Plays synthesized speech audio and coordinates with the echo gate to
//! prevent the microphone from picking up playback output.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rodio::{OutputStream, OutputStreamHandle, Sink};

use crate::error::VoiceError;
use crate::gate::EchoGate;

/// Callback invoked when playback finishes naturally (all queued audio drained).
pub type PlaybackDoneCallback = Box<dyn FnOnce() + Send + 'static>;

/// Audio playback handle for TTS output.
///
/// Wraps `rodio` for audio output. Automatically manages the echo gate —
/// sets `is_system_speaking` when playback starts and clears it when done.
pub struct AudioPlayback {
    /// rodio output stream (must be kept alive).
    _stream: OutputStream,

    /// Handle used to create sinks.
    stream_handle: OutputStreamHandle,

    /// Current playback sink (if any).
    sink: Option<Arc<Sink>>,

    /// Echo gate — set while playing to suppress mic capture.
    echo_gate: EchoGate,

    /// Whether playback is in progress.
    is_playing: Arc<AtomicBool>,
}

impl AudioPlayback {
    /// Create a new audio playback instance using the default output device.
    pub fn new(echo_gate: EchoGate) -> Result<Self, VoiceError> {
        let (stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| VoiceError::OutputStreamError(e.to_string()))?;

        tracing::info!("Audio playback initialized on default output device");

        Ok(Self {
            _stream: stream,
            stream_handle,
            sink: None,
            echo_gate,
            is_playing: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Play audio samples at the given sample rate.
    ///
    /// This sets the echo gate to suppress mic capture during playback.
    /// The caller can use [`stop`] to interrupt playback early.
    pub fn play(&mut self, samples: Vec<f32>, sample_rate: u32) -> Result<(), VoiceError> {
        // Stop any existing playback
        self.stop();

        let sink = Sink::try_new(&self.stream_handle)
            .map_err(|e| VoiceError::OutputStreamError(e.to_string()))?;

        let source = rodio::buffer::SamplesBuffer::new(1, sample_rate, samples);
        sink.append(source);

        // Activate echo gate
        self.echo_gate.start_speaking();
        self.is_playing.store(true, Ordering::SeqCst);

        self.sink = Some(Arc::new(sink));

        tracing::debug!(sample_rate, "Audio playback started");
        Ok(())
    }

    /// Play audio samples and spawn a background task to clear the echo gate
    /// when playback finishes naturally.
    pub fn play_with_gate_management(
        &mut self,
        samples: Vec<f32>,
        sample_rate: u32,
    ) -> Result<(), VoiceError> {
        self.play_with_completion(samples, sample_rate, None)
    }

    /// Play audio samples. When playback finishes naturally (sink drains),
    /// clears the echo gate and invokes `on_done` (if provided).
    pub fn play_with_completion(
        &mut self,
        samples: Vec<f32>,
        sample_rate: u32,
        on_done: Option<PlaybackDoneCallback>,
    ) -> Result<(), VoiceError> {
        self.play(samples, sample_rate)?;
        self.spawn_completion_watcher(on_done);
        Ok(())
    }

    /// Prepare a sink for streaming playback (used by incremental TTS).
    ///
    /// Creates a fresh sink and activates the echo gate. Subsequent audio
    /// chunks should be queued via [`append`]. Call
    /// [`spawn_completion_watcher`] after the last chunk has been appended
    /// so that the echo gate and `on_done` callback fire when audio drains.
    pub fn start_streaming(&mut self) -> Result<(), VoiceError> {
        // Stop any existing playback
        self.stop();

        let sink = Sink::try_new(&self.stream_handle)
            .map_err(|e| VoiceError::OutputStreamError(e.to_string()))?;
        self.sink = Some(Arc::new(sink));
        self.echo_gate.start_speaking();
        self.is_playing.store(true, Ordering::SeqCst);

        tracing::debug!("Streaming playback sink created");
        Ok(())
    }

    /// Spawn a background thread that blocks until the sink drains or
    /// playback is stopped externally. On natural completion, clears the
    /// echo gate and invokes `on_done`.
    ///
    /// This is public so the pipeline can call it after the last chunk has
    /// been appended during streaming synthesis.
    pub fn spawn_completion_watcher(&self, on_done: Option<PlaybackDoneCallback>) {
        let Some(sink) = self.sink.clone() else {
            return;
        };
        if sink.empty() {
            return;
        }

        let echo_gate = self.echo_gate.clone();
        let is_playing = Arc::clone(&self.is_playing);

        // `Sink` is Send in rodio 0.20+, so we can move it into a
        // blocking task. `sleep_until_end()` blocks until the queue
        // drains or `stop()` is called (which drops the internal
        // sources, causing sleep_until_end to return immediately).
        std::thread::spawn(move || {
            sink.sleep_until_end();

            // If stop() was called, is_playing is already false and
            // the gate was already cleared — nothing more to do.
            if !is_playing.swap(false, Ordering::SeqCst) {
                return;
            }

            // Natural completion — clear gate and fire callback.
            echo_gate.stop_speaking();
            tracing::debug!("Playback finished naturally");
            if let Some(cb) = on_done {
                cb();
            }
        });
    }

    /// Queue additional audio samples onto the current playback sink.
    ///
    /// If no sink is active, a new one is created. This enables streaming
    /// TTS — audio chunks are appended as they are synthesized.
    pub fn append(&mut self, samples: Vec<f32>, sample_rate: u32) -> Result<(), VoiceError> {
        let sink = match &self.sink {
            Some(sink) if !sink.empty() || self.is_playing.load(Ordering::SeqCst) => sink,
            _ => {
                // Create a new sink
                let new_sink = Sink::try_new(&self.stream_handle)
                    .map_err(|e| VoiceError::OutputStreamError(e.to_string()))?;
                self.sink = Some(Arc::new(new_sink));
                self.echo_gate.start_speaking();
                self.is_playing.store(true, Ordering::SeqCst);
                self.sink.as_ref().expect("just created")
            }
        };

        let source = rodio::buffer::SamplesBuffer::new(1, sample_rate, samples);
        sink.append(source);

        Ok(())
    }

    /// Stop any active playback immediately.
    ///
    /// Clears the echo gate so the microphone can resume capturing.
    pub fn stop(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        self.is_playing.store(false, Ordering::SeqCst);
        self.echo_gate.stop_speaking();
        tracing::debug!("Audio playback stopped");
    }

    /// Check whether audio is currently playing.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.sink.as_ref().is_some_and(|sink| !sink.empty())
    }

    /// Set playback volume (0.0 = muted, 1.0 = full).
    pub fn set_volume(&self, volume: f32) {
        if let Some(sink) = &self.sink {
            sink.set_volume(volume.clamp(0.0, 1.0));
        }
    }

    /// Set playback speed multiplier (1.0 = normal).
    pub fn set_speed(&self, speed: f32) {
        if let Some(sink) = &self.sink {
            sink.set_speed(speed.max(0.1));
        }
    }
}
