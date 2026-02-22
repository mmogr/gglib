//! Channel-backed audio source and sink for the WebSocket audio data plane.
//!
//! [`WebSocketAudioSource`] receives raw f32 PCM frames from the browser via
//! an `mpsc` channel that is fed by the WS ingest task (which decodes the
//! incoming PCM16 LE binary frames). It exposes audio to the voice pipeline
//! through the [`AudioSource`] trait.
//!
//! [`WebSocketAudioSink`] accepts f32 samples from the voice pipeline (TTS
//! output), encodes them to PCM16 LE, and queues them in an `mpsc` channel
//! that the WS egress task drains and forwards to the browser as binary
//! WebSocket frames.
//!
//! ## Channel failure handling
//!
//! * **Source disconnect** — when the WebSocket closes, the ingest task drops
//!   the `Sender<Vec<f32>>`.  The next call to
//!   [`AudioSource::read_vad_frame`] returns
//!   [`VoiceError::AudioThreadDied`], which causes the pipeline's VAD loop
//!   to initiate a clean shutdown.  `stop_capture` is also panic-free: it
//!   drains whatever frames remain and returns the accumulated buffer.
//!
//! * **Sink disconnect** — when the WebSocket closes, the egress task drops
//!   the `Receiver<Vec<u8>>`, which causes [`AudioSink::append`] to return
//!   [`VoiceError::OutputStreamError`] on the next call.  The pipeline's
//!   `speak()` method propagates this error and exits the synthesis loop
//!   cleanly.  Overflow (buffer full) is silently dropped rather than
//!   back-pressuring the pipeline, to prevent stale audio buildup.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;
use tracing::warn;

use gglib_voice::VoiceError;
use gglib_voice::audio_io::{AudioSink, AudioSource};
use gglib_voice::capture::AudioDeviceInfo;

/// Shared slot for the sink's one-shot completion callback.
///
/// The `Arc` is cloned into the WS ingest task so it can fire the callback
/// when the browser sends `playback_drained`, and into the timeout task as a
/// fallback.  The `Option` is consumed by whichever fires first.
pub type PendingCompletion = Arc<Mutex<Option<Box<dyn FnOnce() + Send + 'static>>>>;

// ── WebSocketAudioSource ───────────────────────────────────────────────────────

/// Audio source backed by binary WebSocket frames from a browser.
///
/// The browser sends PCM16 LE at 16 kHz, mono.  Each frame is nominally
/// 960 bytes (30 ms × 16 000 Hz × 2 bytes/sample), but any frame size is
/// accepted.  The ingest task decodes to `f32` before sending here so that
/// the pipeline never has to deal with integer PCM.
///
/// **Factory:** [`WebSocketAudioSource::new`] returns the source and the
/// matching `Sender` that the WS ingest task uses to push decoded frames.
pub struct WebSocketAudioSource {
    /// Receives decoded f32 frames from the WS ingest task.
    ///
    /// Wrapped in a `Mutex` so `read_vad_frame` and `stop_capture` can
    /// take `&self` (required by the trait) while mutating the receiver.
    frame_rx: Mutex<mpsc::Receiver<Vec<f32>>>,
    /// Accumulates samples between `start_capture()` and `stop_capture()`.
    buffer: Mutex<Vec<f32>>,
    /// True while PTT capture is in progress.
    capturing: AtomicBool,
}

impl WebSocketAudioSource {
    /// Create a new source and the matching sender for the WS ingest task.
    ///
    /// The ingest task feeds decoded f32 frames via `source_tx`.  Dropping
    /// `source_tx` signals "WebSocket connection closed" — the source will
    /// subsequently return [`VoiceError::AudioThreadDied`] from
    /// `read_vad_frame`.
    ///
    /// # Channel capacity
    /// Up to 200 frames (~6 s at 30 ms/frame) are buffered.  Past this
    /// limit the ingest task applies back-pressure.
    #[must_use]
    pub fn new() -> (Self, mpsc::Sender<Vec<f32>>) {
        let (tx, rx) = mpsc::channel(200);
        let source = Self {
            frame_rx: Mutex::new(rx),
            buffer: Mutex::new(Vec::new()),
            capturing: AtomicBool::new(false),
        };
        (source, tx)
    }
}

impl AudioSource for WebSocketAudioSource {
    fn start_capture(&self) -> Result<(), VoiceError> {
        self.buffer.lock().unwrap().clear();
        self.capturing.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn stop_capture(&self) -> Result<Vec<f32>, VoiceError> {
        self.capturing.store(false, Ordering::SeqCst);
        // Drain any frames already queued in the channel.  If the channel is
        // disconnected we simply stop draining — whatever was accumulated is
        // still valid audio.
        let mut rx = self.frame_rx.lock().unwrap();
        while let Ok(frame) = rx.try_recv() {
            self.buffer.lock().unwrap().extend(frame);
        }
        Ok(std::mem::take(&mut *self.buffer.lock().unwrap()))
    }

    fn read_vad_frame(&self) -> Result<Option<Vec<f32>>, VoiceError> {
        match self.frame_rx.lock().unwrap().try_recv() {
            Ok(frame) => {
                // In VAD mode, also accumulate the frame in the capture buffer
                // so that stop_capture() returns the full utterance.
                if self.capturing.load(Ordering::SeqCst) {
                    self.buffer.lock().unwrap().extend_from_slice(&frame);
                }
                Ok(Some(frame))
            }
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            // Sender was dropped — WebSocket connection closed.
            Err(mpsc::error::TryRecvError::Disconnected) => Err(VoiceError::AudioThreadDied),
        }
    }

    fn is_capturing(&self) -> bool {
        self.capturing.load(Ordering::SeqCst)
    }

    /// Remote source has no device enumeration — returns an empty list.
    fn list_devices(&self) -> Result<Vec<AudioDeviceInfo>, VoiceError> {
        Ok(Vec::new())
    }
}

// ── WebSocketAudioSink ────────────────────────────────────────────────────────

/// Server-side timeout for the `playback_drained` acknowledgement.
///
/// If the browser does not send `playback_drained` within this many seconds
/// after the last TTS chunk has been queued, the completion callback is fired
/// unconditionally so the pipeline is never left waiting indefinitely.
/// 8 s is generous enough for long TTS utterances while still providing a
/// reliable safety net for disconnected or stalled browsers.
const DRAIN_TIMEOUT_SECS: u64 = 8;

/// Audio sink that delivers TTS output to a browser as binary WebSocket frames.
///
/// f32 samples (any sample rate) are encoded to PCM16 LE and queued in a
/// bounded channel.  The WS egress task drains the channel and sends each
/// chunk as a binary WebSocket message.
///
/// **Overflow policy:** when the send buffer is full the chunk is silently
/// dropped and a warning is logged.  This prevents stale audio build-up if
/// the egress task cannot keep up (e.g. a slow browser connection).
///
/// **Factory:** [`WebSocketAudioSink::new`] returns the sink and the matching
/// `Receiver` that the WS egress task drains.
pub struct WebSocketAudioSink {
    /// Sends PCM16 LE byte buffers to the WS egress task.
    frame_tx: mpsc::Sender<Vec<u8>>,
    /// True while the sink is actively streaming TTS audio.
    playing: AtomicBool,
    /// Completion callback registered by the pipeline after the last TTS
    /// chunk is appended.
    ///
    /// # Completion semantics for the WebSocket sink
    ///
    /// Unlike the local rodio sink — which fires this after the audio ring
    /// buffer drains — the WS sink fires it immediately from
    /// `on_playback_complete`, because there is no in-process notification
    /// boundary once bytes have left via the network channel.  The
    /// consequence is that `VoiceSpeakingFinished` reaches the frontend
    /// slightly before the browser finishes playing the last frame.
    ///
    /// ## Timing gap
    /// The gap has two components:
    ///   1. **Network latency**: propagation + TCP send-buffer delay for the
    ///      last WS frame (~1–20 ms on LAN, larger on WAN).
    ///   2. **Browser ring-buffer drain**: the playback `AudioWorklet` uses a
    ///      2-second ring buffer; if TTS output was long the buffer may hold
    ///      up to ~2 s of audio that the browser has not yet rendered.
    ///
    /// In practice the combined gap is small for short responses, but can
    /// reach several hundred milliseconds (or more) for long TTS utterances.
    ///
    /// ## Why this is acceptable for now
    ///   1. PTT flow: the user triggers the next action (press-to-talk),
    ///      which calls `sink.stop()` anyway — state is consistent.
    ///   2. Auto-speak flow: the pipeline transitions to `Listening` a few
    ///      hundred milliseconds early; browsers with echo-cancellation
    ///      on `getUserMedia` suppress TTS bleed-through regardless.
    ///
    /// ## Fix (GitHub issue #230)
    /// The callback is now deferred: `on_playback_complete` stores it and the
    /// WS ingest task fires it when the browser sends a `playback_drained`
    /// text frame confirming the AudioWorklet ring buffer has drained.  A
    /// server-side timeout fires it after [`DRAIN_TIMEOUT_SECS`] so a stalled
    /// or disconnected browser cannot freeze the pipeline.
    /// `stop()` fires it immediately as the fallback for PTT and explicit
    /// stop_speaking() paths.
    ///
    /// Shared with the WS ingest task — the third value returned by
    /// [`WebSocketAudioSink::new`] is a clone of this `Arc`.
    pending_completion: PendingCompletion,
}

impl WebSocketAudioSink {
    /// Create a new sink and the matching receiver for the WS egress task.
    ///
    /// The egress task drains `sink_rx` and forwards each `Vec<u8>` chunk
    /// as a binary WebSocket frame; when `sink_rx` is closed (all senders
    /// dropped) the egress task exits cleanly.
    ///
    /// # Channel capacity
    /// Up to 64 chunks are buffered, providing ~1–2 s of headroom
    /// for typical TTS synthesis bursts before overflow policy kicks in.
    ///
    /// # Return values
    /// 1. The sink (implements [`AudioSink`]).
    /// 2. The `Receiver` the WS egress task drains.
    /// 3. An `Arc` of the pending-completion slot — pass this to the WS
    ///    ingest task so it can fire the callback on `playback_drained`.
    #[must_use]
    pub fn new() -> (Self, mpsc::Receiver<Vec<u8>>, PendingCompletion) {
        let (tx, rx) = mpsc::channel(64);
        let pending = Arc::new(Mutex::new(None::<Box<dyn FnOnce() + Send + 'static>>));
        let sink = Self {
            frame_tx: tx,
            playing: AtomicBool::new(false),
            pending_completion: Arc::clone(&pending),
        };
        (sink, rx, pending)
    }

    /// Encode f32 samples (range −1.0 … 1.0) to PCM16 LE bytes.
    ///
    /// Values outside [−1, 1] are clamped before conversion.
    fn encode_pcm16(samples: &[f32]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(samples.len() * 2);
        for &s in samples {
            let clamped = s.clamp(-1.0, 1.0);
            #[allow(clippy::cast_possible_truncation)]
            let i16_val = (clamped * 32_767.0) as i16;
            buf.extend_from_slice(&i16_val.to_le_bytes());
        }
        buf
    }
}

impl AudioSink for WebSocketAudioSink {
    fn start_streaming(&self) -> Result<(), VoiceError> {
        self.playing.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn append(&self, samples: Vec<f32>, _sample_rate: u32) -> Result<(), VoiceError> {
        let pcm = Self::encode_pcm16(&samples);
        match self.frame_tx.try_send(pcm) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                // Overflow: drop this chunk to avoid stale audio build-up.
                warn!("WebSocketAudioSink: send buffer full — dropping audio chunk");
                Ok(()) // Intentional drop; not a fatal error.
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                // Receiver dropped — WS connection closed while TTS was active.
                Err(VoiceError::OutputStreamError(
                    "WebSocket connection closed during TTS playback".into(),
                ))
            }
        }
    }

    fn stop(&self) -> Result<(), VoiceError> {
        self.playing.store(false, Ordering::SeqCst);
        // Fire immediately.  Covers both the explicit stop_speaking() path and
        // the PTT ptt_start() path (which calls sink.stop() before starting
        // capture).  `.take()` ensures the timeout task is a no-op if it fires
        // after this.
        if let Some(cb) = self.pending_completion.lock().unwrap().take() {
            cb();
        }
        Ok(())
    }

    fn is_playing(&self) -> bool {
        self.playing.load(Ordering::SeqCst)
    }

    fn on_playback_complete(&self, callback: Box<dyn FnOnce() + Send + 'static>) {
        // Store the callback — it will be fired by whichever wins the race:
        //   1. The WS ingest task, when the browser sends `playback_drained`,
        //      confirming the AudioWorklet ring buffer has fully drained.
        //   2. `stop()`, for PTT and explicit stop_speaking() paths.
        //   3. The server-side timeout task below, so a stalled or disconnected
        //      browser cannot freeze the pipeline indefinitely.
        *self.pending_completion.lock().unwrap() = Some(callback);

        let pending = Arc::clone(&self.pending_completion);
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                tokio::time::sleep(Duration::from_secs(DRAIN_TIMEOUT_SECS)).await;
                if let Some(cb) = pending.lock().unwrap().take() {
                    warn!(
                        timeout_secs = DRAIN_TIMEOUT_SECS,
                        "WebSocketAudioSink: playback_drained not received within \
                         timeout — firing completion callback"
                    );
                    cb();
                }
            });
        }
    }
}
