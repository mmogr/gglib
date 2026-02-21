//! Voice pipeline orchestrator — coordinates STT, TTS, VAD, capture, and playback.
//!
//! The pipeline is a state machine that drives the full voice conversation loop:
//!
//! ```text
//!   Idle → Listening → Transcribing → (emit transcript) → Speaking → Idle
//!          ▲                                                    │
//!          └────────────────────────────────────────────────────┘
//! ```
//!
//! Two interaction modes are supported:
//! - **Push-to-Talk (PTT)**: User presses a button to start/stop recording.
//! - **Voice Activity Detection (VAD)**: Audio is continuously monitored;
//!   speech boundaries are detected automatically.

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::audio_io::{AudioSink, AudioSource};
use crate::backend::{SttBackend, SttConfig, TtsBackend, TtsConfig};
use crate::error::VoiceError;
use crate::gate::EchoGate;
use crate::text_utils;
use crate::vad::{VadConfig, VadEvent, VoiceActivityDetector};

// ── Voice state machine ────────────────────────────────────────────

/// Current state of the voice pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoiceState {
    /// Pipeline is idle — voice mode not active.
    Idle,

    /// Listening for speech (mic active, VAD running or waiting for PTT release).
    Listening,

    /// Recording audio (PTT mode — button held down).
    Recording,

    /// Transcribing captured audio via STT engine.
    Transcribing,

    /// Waiting for LLM response (pipeline doesn't own this, but tracks it).
    Thinking,

    /// Playing back TTS audio.
    Speaking,

    /// An error occurred — voice mode paused.
    Error,
}

// ── Interaction mode ───────────────────────────────────────────────

/// How the user triggers speech input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VoiceInteractionMode {
    /// User holds a button to record, releases to send.
    #[default]
    PushToTalk,

    /// Continuous listening with automatic speech boundary detection.
    VoiceActivityDetection,
}

// ── Events emitted by the pipeline ─────────────────────────────────

/// Events emitted by the voice pipeline to the UI / application layer.
#[derive(Debug, Clone)]
pub enum VoiceEvent {
    /// Pipeline state changed.
    StateChanged(VoiceState),

    /// A transcript was produced from speech.
    Transcript {
        /// The transcribed text.
        text: String,
        /// Whether this is a partial (streaming) or final result.
        is_final: bool,
    },

    /// TTS playback started.
    SpeakingStarted,

    /// TTS playback finished.
    SpeakingFinished,

    /// An error occurred in the pipeline.
    Error(String),

    /// Microphone audio level (0.0–1.0), for UI visualisation.
    AudioLevel(f32),
}

// ── Pipeline configuration ─────────────────────────────────────────

/// Configuration for the voice pipeline.
#[derive(Debug, Clone)]
pub struct VoicePipelineConfig {
    /// Interaction mode (PTT or VAD).
    pub mode: VoiceInteractionMode,

    /// STT engine configuration.
    pub stt: SttConfig,

    /// TTS engine configuration.
    pub tts: TtsConfig,

    /// VAD configuration (used only in VAD mode).
    pub vad: VadConfig,

    /// Whether to automatically speak LLM responses via TTS.
    pub auto_speak: bool,

    /// Optional path to a Silero VAD ONNX model.
    ///
    /// When set (and the `sherpa` feature is enabled), the pipeline will
    /// load this model into the VAD on [`start()`](VoicePipeline::start)
    /// for neural-network-based speech boundary detection.
    pub vad_model_path: Option<std::path::PathBuf>,
}

impl Default for VoicePipelineConfig {
    fn default() -> Self {
        Self {
            mode: VoiceInteractionMode::PushToTalk,
            stt: SttConfig::default(),
            tts: TtsConfig::default(),
            vad: VadConfig::default(),
            auto_speak: true,
            vad_model_path: None,
        }
    }
}

// ── Voice pipeline ─────────────────────────────────────────────────

/// The main voice pipeline orchestrator.
///
/// Coordinates audio capture, VAD, STT, TTS, and playback into a coherent
/// voice conversation loop. Emits [`VoiceEvent`]s via a channel for the
/// UI layer to consume.
pub struct VoicePipeline {
    /// Current state — behind a `Mutex` so that `ptt_start`, `ptt_stop`,
    /// `speak`, and `stop_speaking` can take `&self` (enabling concurrent
    /// readers on the `tokio::sync::RwLock` in `VoiceService`).
    state: Mutex<VoiceState>,

    /// Interaction mode.
    mode: VoiceInteractionMode,

    /// Shared echo gate.
    echo_gate: EchoGate,

    /// Audio input source — mic capture via cpal (local) or WebSocket stream (web).
    source: Option<Box<dyn AudioSource>>,

    /// Audio output sink — TTS playback via rodio (local) or WebSocket stream (web).
    sink: Option<Box<dyn AudioSink>>,

    /// Speech-to-text engine (loaded lazily).
    stt: Option<Box<dyn SttBackend>>,

    /// Text-to-speech engine (loaded lazily).
    tts: Option<Box<dyn TtsBackend>>,

    /// Voice activity detector (used in VAD mode).
    vad: Option<VoiceActivityDetector>,

    /// Event sender channel.
    event_tx: mpsc::UnboundedSender<VoiceEvent>,

    /// Whether the pipeline is active.
    is_active: Arc<AtomicBool>,

    /// Cancellation flag for in-progress `speak()` calls.
    ///
    /// `stop_speaking()` sets this to `true`; `speak()` checks it after
    /// each synthesised chunk and aborts early.  Reset to `false` at the
    /// start of every `speak()` call.
    speak_cancel: AtomicBool,

    /// Pipeline configuration.
    config: VoicePipelineConfig,

    /// The model ID of the currently loaded STT engine, if any.
    ///
    /// Set when [`load_stt`](VoicePipeline::load_stt) succeeds; cleared when
    /// the pipeline is dropped. Used by `voice_status` to report the active
    /// model to the frontend without requiring a round-trip to the catalog.
    loaded_stt_model_id: Option<String>,
}

impl VoicePipeline {
    /// Create a new voice pipeline.
    ///
    /// Returns the pipeline and a receiver for [`VoiceEvent`]s.
    #[must_use]
    pub fn new(config: VoicePipelineConfig) -> (Self, mpsc::UnboundedReceiver<VoiceEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let echo_gate = EchoGate::new();

        let pipeline = Self {
            state: Mutex::new(VoiceState::Idle),
            mode: config.mode,
            echo_gate,
            source: None,
            sink: None,
            stt: None,
            tts: None,
            vad: None,
            event_tx,
            is_active: Arc::new(AtomicBool::new(false)),
            speak_cancel: AtomicBool::new(false),
            config,
            loaded_stt_model_id: None,
        };

        (pipeline, event_rx)
    }

    /// Get the current pipeline state.
    #[must_use]
    pub fn state(&self) -> VoiceState {
        *self.state.lock().unwrap()
    }

    /// Get the interaction mode.
    #[must_use]
    pub const fn mode(&self) -> VoiceInteractionMode {
        self.mode
    }

    /// Check whether the pipeline is active (voice mode on).
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.is_active.load(Ordering::SeqCst)
    }

    /// Return the model ID of the currently loaded STT engine, if any.
    #[must_use]
    pub fn stt_model_id(&self) -> Option<&str> {
        self.loaded_stt_model_id.as_deref()
    }

    /// Return the currently configured TTS voice ID.
    #[must_use]
    pub fn tts_voice(&self) -> &str {
        &self.config.tts.voice
    }

    // ── Lifecycle ──────────────────────────────────────────────────

    /// Start the voice pipeline with explicitly provided audio I/O backends.
    ///
    /// This is the primary lifecycle entry-point. Separating backend
    /// construction from injection makes the pipeline testable with mock
    /// audio without real hardware, and enables the WebSocket audio path
    /// (Phase 3) to supply a different backend at runtime.
    ///
    /// STT and TTS engines are loaded lazily on first use.
    ///
    /// # Errors
    ///
    /// Returns [`VoiceError::AlreadyActive`] if the pipeline is already
    /// running.
    pub fn start_with_audio(
        &mut self,
        source: Box<dyn AudioSource>,
        sink: Box<dyn AudioSink>,
    ) -> Result<(), VoiceError> {
        if self.is_active() {
            return Err(VoiceError::AlreadyActive);
        }

        tracing::info!(mode = ?self.mode, "Starting voice pipeline");

        self.source = Some(source);
        self.sink = Some(sink);

        // In VAD mode, initialise the detector
        if self.mode == VoiceInteractionMode::VoiceActivityDetection {
            let mut vad = VoiceActivityDetector::new(
                self.config.vad.clone(),
                self.echo_gate.clone(),
                crate::capture::TARGET_SAMPLE_RATE,
            );

            // Load Silero VAD model if a path was configured.
            if let Some(ref model_path) = self.config.vad_model_path {
                if let Err(e) = vad.load_silero_model(model_path) {
                    tracing::warn!(
                        error = %e,
                        "Failed to load Silero VAD model — falling back to energy-based VAD"
                    );
                }
            }

            vad.start();
            self.vad = Some(vad);
        }

        self.is_active.store(true, Ordering::SeqCst);
        self.set_state(VoiceState::Listening);

        tracing::info!("Voice pipeline started");
        Ok(())
    }

    /// Start the voice pipeline using the local cpal/rodio audio backends.
    ///
    /// Convenience wrapper around
    /// [`start_with_audio`](VoicePipeline::start_with_audio) that creates a
    /// [`LocalAudioSource`](crate::audio_local::LocalAudioSource) /
    /// [`LocalAudioSink`](crate::audio_local::LocalAudioSink) pair backed by
    /// a single [`AudioThreadHandle`](crate::audio_thread::AudioThreadHandle).
    ///
    /// Preserved for backward compatibility with existing tests and the CLI.
    ///
    /// # Errors
    ///
    /// Returns [`VoiceError`] if the audio thread fails to start or the
    /// pipeline is already active.
    pub fn start(&mut self) -> Result<(), VoiceError> {
        let (source, sink) = crate::audio_local::new_pair(&self.echo_gate)?;
        self.start_with_audio(Box::new(source), Box::new(sink))
    }

    /// Stop the voice pipeline and release all resources.
    pub fn stop(&mut self) {
        tracing::info!("Stopping voice pipeline");

        // Stop any active recording.
        if let Some(ref source) = self.source {
            if source.is_capturing() {
                let _ = source.stop_capture();
            }
        }
        // Stop any active playback.
        if let Some(ref sink) = self.sink {
            let _ = sink.stop();
        }
        // Drop both handles — for LocalAudio* this joins the audio OS thread.
        self.source.take();
        self.sink.take();

        // Stop VAD
        if let Some(ref mut vad) = self.vad {
            vad.stop();
        }

        self.is_active.store(false, Ordering::SeqCst);
        self.set_state(VoiceState::Idle);

        tracing::info!("Voice pipeline stopped");
    }

    // ── STT/TTS engine management ──────────────────────────────────

    /// Load or replace the STT engine with a model at the given path.
    ///
    /// `model_path` should be a **directory** containing `encoder.onnx`,
    /// `decoder.onnx`, and `tokens.txt`.
    ///
    /// `model_id` is stored verbatim and returned by
    /// [`stt_model_id`](VoicePipeline::stt_model_id) so the frontend can
    /// display which model is currently active without querying the catalog.
    pub fn load_stt(
        &mut self,
        model_path: &std::path::Path,
        model_id: &str,
    ) -> Result<(), VoiceError> {
        use crate::backend::sherpa_stt::{SherpaSttBackend, SherpaSttConfig};

        tracing::info!(path = %model_path.display(), model_id, "Loading STT engine");

        let sherpa_config = SherpaSttConfig {
            language: self.config.stt.language.clone(),
            ..SherpaSttConfig::default()
        };
        let engine = SherpaSttBackend::load(model_path, &sherpa_config)?;
        self.stt = Some(Box::new(engine));
        self.loaded_stt_model_id = Some(model_id.to_owned());
        Ok(())
    }

    /// Load or replace the TTS engine from a model directory.
    ///
    /// `model_dir` should contain `model.onnx`, `voices.bin`, `tokens.txt`,
    /// and an `espeak-ng-data/` subdirectory.
    #[allow(clippy::unused_async)]
    pub async fn load_tts(&mut self, model_dir: &std::path::Path) -> Result<(), VoiceError> {
        use crate::backend::sherpa_tts::{SherpaTtsBackend, SherpaTtsConfig};

        tracing::info!(dir = %model_dir.display(), "Loading TTS engine");

        let sherpa_config = SherpaTtsConfig {
            voice: self.config.tts.voice.clone(),
            speed: self.config.tts.speed,
        };
        let engine = SherpaTtsBackend::load(model_dir, &sherpa_config)?;
        self.tts = Some(Box::new(engine));
        Ok(())
    }

    /// Check whether the STT engine is loaded and ready.
    #[must_use]
    pub const fn is_stt_loaded(&self) -> bool {
        self.stt.is_some()
    }

    /// Check whether the TTS engine is loaded and ready.
    #[must_use]
    pub const fn is_tts_loaded(&self) -> bool {
        self.tts.is_some()
    }

    // ── Push-to-Talk flow ──────────────────────────────────────────

    /// Begin recording (PTT mode: user pressed the talk button).
    pub fn ptt_start(&self) -> Result<(), VoiceError> {
        if !self.is_active() {
            return Err(VoiceError::NotActive);
        }

        // Stop any active playback first, then start capture.
        let sink = self.sink.as_ref().ok_or(VoiceError::NotActive)?;
        sink.stop()?;
        let source = self.source.as_ref().ok_or(VoiceError::NotActive)?;
        source.start_capture()?;
        self.set_state(VoiceState::Recording);

        Ok(())
    }

    /// Finish recording and transcribe (PTT mode: user released the talk button).
    ///
    /// Returns the transcribed text. Also emits a `VoiceEvent::Transcript`.
    pub async fn ptt_stop(&self) -> Result<String, VoiceError> {
        if !self.is_active() {
            return Err(VoiceError::NotActive);
        }

        let source = self.source.as_ref().ok_or(VoiceError::NotActive)?;
        let audio = source.stop_capture()?;

        if audio.is_empty() {
            self.set_state(VoiceState::Listening);
            return Ok(String::new());
        }

        self.set_state(VoiceState::Transcribing);

        let stt = self.stt.as_ref().ok_or(VoiceError::SttModelNotLoaded)?;
        let text = stt.transcribe(&audio).await?;

        self.emit(VoiceEvent::Transcript {
            text: text.clone(),
            is_final: true,
        });

        // Return to listening state (caller will transition to Thinking/Speaking)
        self.set_state(VoiceState::Listening);

        Ok(text)
    }

    // ── VAD flow ───────────────────────────────────────────────────

    /// Feed an audio frame to the VAD for processing.
    ///
    /// In VAD mode, this is called continuously with mic audio frames.
    /// When the VAD detects a complete utterance (speech followed by silence),
    /// the audio is automatically transcribed and a `VoiceEvent::Transcript`
    /// is emitted.
    pub async fn vad_process_frame(&mut self, frame: &[f32]) -> Result<Option<String>, VoiceError> {
        // Calculate audio level for UI visualization
        let level = calculate_audio_level(frame);
        self.emit(VoiceEvent::AudioLevel(level));

        let vad = self.vad.as_mut().ok_or(VoiceError::NotActive)?;

        match vad.process_frame(frame) {
            Some(VadEvent::SpeechStart) => {
                self.set_state(VoiceState::Recording);
                Ok(None)
            }

            Some(VadEvent::SpeechEnd { audio }) => {
                self.set_state(VoiceState::Transcribing);

                let stt = self.stt.as_ref().ok_or(VoiceError::SttModelNotLoaded)?;
                let text = stt.transcribe(&audio).await?;

                if !text.is_empty() {
                    self.emit(VoiceEvent::Transcript {
                        text: text.clone(),
                        is_final: true,
                    });
                }

                self.set_state(VoiceState::Listening);
                Ok(Some(text).filter(|t| !t.is_empty()))
            }

            Some(VadEvent::Listening) | None => Ok(None),
        }
    }

    // ── TTS playback ───────────────────────────────────────────────

    /// Speak text through TTS and play back the audio.
    ///
    /// Long text is automatically stripped of markdown formatting and split
    /// into sentence-sized chunks so that the TTS engine can synthesize each
    /// piece reliably (it struggles with very long inputs).
    ///
    /// Audio is **streamed incrementally**: the first chunk starts playing
    /// as soon as it is synthesized, and subsequent chunks are appended to
    /// the playback sink while synthesis continues in the foreground. This
    /// means the user hears speech almost immediately, even for very long
    /// messages.
    ///
    /// A background completion watcher is spawned after the last chunk is
    /// appended so that `SpeakingFinished` is emitted when playback
    /// naturally drains (rather than only on explicit `stop_speaking()`).
    #[allow(clippy::cognitive_complexity)]
    pub async fn speak(&self, text: &str) -> Result<(), VoiceError> {        // Reset cancellation flag from any previous stop_speaking() call.
        self.speak_cancel.store(false, Ordering::SeqCst);
        if text.trim().is_empty() {
            return Ok(());
        }

        // Preprocess: strip markdown → plain text → split into chunks
        let plain = text_utils::strip_markdown(text);
        let chunks = text_utils::split_into_chunks(&plain);

        if chunks.is_empty() {
            return Ok(());
        }

        tracing::debug!(
            original_len = text.len(),
            plain_len = plain.len(),
            num_chunks = chunks.len(),
            "Speaking text in chunks"
        );

        let tts = self.tts.as_ref().ok_or(VoiceError::TtsModelNotLoaded)?;

        // Prepare a streaming playback sink before synthesis begins.
        let sink = self
            .sink
            .as_ref()
            .ok_or_else(|| VoiceError::OutputStreamError("Audio thread not running".to_string()))?;
        sink.start_streaming()?;

        let mut any_audio = false;
        let mut total_duration = std::time::Duration::ZERO;
        let mut failed_chunks: usize = 0;
        let total_chunks = chunks.len();

        for (i, chunk) in chunks.iter().enumerate() {
            match tts.synthesize(chunk).await {
                Ok(audio) => {
                    tracing::debug!(
                        chunk = i + 1,
                        chunk_len = chunk.len(),
                        samples = audio.samples.len(),
                        duration_ms = audio.duration.as_millis(),
                        "Synthesised chunk"
                    );

                    // Emit SpeakingStarted on the first successful chunk
                    // so the frontend knows audio is about to play.
                    if !any_audio {
                        any_audio = true;
                        // `set_state` and `emit` both take `&self` — no borrow
                        // conflict with `tts` holding `&self.tts`.
                        self.set_state(VoiceState::Speaking);
                        self.emit(VoiceEvent::SpeakingStarted);
                    }

                    // Check for cancellation (stop_speaking called concurrently).
                    if self.speak_cancel.load(Ordering::SeqCst) {
                        tracing::debug!("speak() interrupted by stop_speaking");
                        break;
                    }

                    // AudioSink methods take &self, so no re-borrow needed.
                    let s = self.sink.as_ref().expect("audio sink started above");
                    s.append(audio.samples, audio.sample_rate)?;
                    total_duration += audio.duration;
                }
                Err(e) => {
                    failed_chunks += 1;
                    tracing::warn!(
                        chunk = i + 1,
                        chunk_text = &chunk[..chunk.len().min(80)],
                        error = %e,
                        "Failed to synthesise chunk, skipping"
                    );
                    // Continue with remaining chunks rather than failing entirely
                }
            }
        }

        if failed_chunks > 0 {
            tracing::warn!(
                failed = failed_chunks,
                total = total_chunks,
                "TTS synthesis completed with chunk failures — audio may be incomplete"
            );
        }

        if !any_audio {
            // Every chunk failed — tear down the empty sink and report error.
            if let Some(ref s) = self.sink {
                let _ = s.stop();
            }
            return Err(VoiceError::SynthesisError(
                "all chunks failed to synthesize".to_string(),
            ));
        }

        tracing::debug!(
            total_duration_ms = total_duration.as_millis(),
            "All chunks synthesised, audio is streaming"
        );

        // Spawn a background thread that fires SpeakingFinished when the
        // sink drains naturally (all appended audio has been played).
        let event_tx = self.event_tx.clone();
        let is_active = Arc::clone(&self.is_active);
        let on_done = Box::new(move || {
            let _ = event_tx.send(VoiceEvent::SpeakingFinished);
            let _ = event_tx.send(VoiceEvent::StateChanged(
                if is_active.load(Ordering::SeqCst) {
                    VoiceState::Listening
                } else {
                    VoiceState::Idle
                },
            ));
        });

        let sink = self.sink.as_ref().expect("audio sink started above");
        sink.on_playback_complete(on_done);

        Ok(())
    }

    /// Stop any active TTS playback immediately.
    pub fn stop_speaking(&self) {
        // Signal any in-progress speak() loop to abort after its current chunk.
        self.speak_cancel.store(true, Ordering::SeqCst);
        if let Some(ref sink) = self.sink {
            let _ = sink.stop();
        }
        self.emit(VoiceEvent::SpeakingFinished);
        if self.is_active() {
            self.set_state(VoiceState::Listening);
        }
    }

    /// Check if TTS playback is currently active.
    #[must_use]
    pub fn is_speaking(&self) -> bool {
        self.sink
            .as_ref()
            .is_some_and(|s| s.is_playing())
    }

    // ── Configuration ──────────────────────────────────────────────

    /// Update the interaction mode.
    pub fn set_mode(&mut self, mode: VoiceInteractionMode) {
        if self.mode == mode {
            return;
        }

        tracing::info!(old = ?self.mode, new = ?mode, "Voice mode changed");
        self.mode = mode;

        // Manage VAD state based on mode
        match mode {
            VoiceInteractionMode::VoiceActivityDetection => {
                if self.vad.is_none() && self.is_active() {
                    let mut vad = VoiceActivityDetector::new(
                        self.config.vad.clone(),
                        self.echo_gate.clone(),
                        crate::capture::TARGET_SAMPLE_RATE,
                    );

                    if let Some(ref model_path) = self.config.vad_model_path {
                        if let Err(e) = vad.load_silero_model(model_path) {
                            tracing::warn!(
                                error = %e,
                                "Failed to load Silero VAD — falling back to energy-based VAD"
                            );
                        }
                    }

                    vad.start();
                    self.vad = Some(vad);
                }
            }
            VoiceInteractionMode::PushToTalk => {
                if let Some(ref mut vad) = self.vad {
                    vad.stop();
                }
                self.vad = None;
            }
        }
    }

    /// Update the TTS voice.
    pub fn set_voice(&mut self, voice: impl Into<String>) {
        let voice_id = voice.into();
        if let Some(ref mut tts) = self.tts {
            tts.set_voice(&voice_id);
        }
        self.config.tts.voice = voice_id;
    }

    /// Update the TTS speed.
    pub fn set_speed(&mut self, speed: f32) {
        if let Some(ref mut tts) = self.tts {
            tts.set_speed(speed);
        }
        self.config.tts.speed = speed.clamp(0.5, 2.0);
    }

    /// Get whether auto-speak is enabled.
    #[must_use]
    pub const fn auto_speak(&self) -> bool {
        self.config.auto_speak
    }

    /// Set whether LLM responses should automatically be spoken.
    pub const fn set_auto_speak(&mut self, auto_speak: bool) {
        self.config.auto_speak = auto_speak;
    }

    /// Get a clone of the echo gate for external coordination.
    #[must_use]
    pub fn echo_gate(&self) -> EchoGate {
        self.echo_gate.clone()
    }

    // ── Internal helpers ───────────────────────────────────────────

    /// Transition to a new state and emit a state-change event.
    fn set_state(&self, new_state: VoiceState) {
        let old_state = *self.state.lock().unwrap();
        if old_state != new_state {
            tracing::debug!(old = ?old_state, new = ?new_state, "Voice state transition");
            *self.state.lock().unwrap() = new_state;
            self.emit(VoiceEvent::StateChanged(new_state));
        }
    }

    /// Emit a voice event (best-effort — if the receiver is dropped, we log and move on).
    fn emit(&self, event: VoiceEvent) {
        if self.event_tx.send(event).is_err() {
            tracing::warn!("Voice event receiver dropped");
        }
    }

    // ── Test helpers ───────────────────────────────────────────────

    /// Inject a mock STT backend for testing without loading real model files.
    ///
    /// Allows integration tests to drive the pipeline with a canned backend
    /// without loading real model files.
    ///
    /// # Test helper
    /// This method is intended for unit and integration tests only.
    /// It is available unconditionally because this crate is `publish = false`
    /// and integration tests in `tests/` cannot access `#[cfg(test)]`-gated items.
    #[doc(hidden)]
    pub fn inject_stt(&mut self, stt: Box<dyn SttBackend>) {
        self.stt = Some(stt);
    }

    /// Inject a mock TTS backend for testing without loading real model files.
    ///
    /// # Test helper
    #[doc(hidden)]
    pub fn inject_tts(&mut self, tts: Box<dyn TtsBackend>) {
        self.tts = Some(tts);
    }

    /// Mark the pipeline active without starting real audio hardware.
    ///
    /// Used by tests that need `is_active() == true` to exercise guards that
    /// require an active pipeline (e.g. `ptt_start`), but only test state
    /// transitions — not actual audio I/O.
    ///
    /// For tests that also need audio call sites to succeed, prefer calling
    /// [`start_with_audio`](VoicePipeline::start_with_audio) directly with
    /// `MockAudioSource`/`MockAudioSink` — that is the replacement for this
    /// helper.
    ///
    /// # Test helper
    #[doc(hidden)]
    pub fn set_active_for_test(&mut self) {
        self.is_active.store(true, Ordering::SeqCst);
        self.set_state(VoiceState::Listening);
    }

    /// Inject mock audio backends and activate the pipeline, bypassing real
    /// hardware initialisation.
    ///
    /// Equivalent to calling
    /// [`start_with_audio`](VoicePipeline::start_with_audio); provided as a
    /// named helper so test code reads intention-first.
    ///
    /// # Test helper
    #[doc(hidden)]
    pub fn inject_audio(
        &mut self,
        source: Box<dyn AudioSource>,
        sink: Box<dyn AudioSink>,
    ) -> Result<(), VoiceError> {
        self.start_with_audio(source, sink)
    }
}

impl Drop for VoicePipeline {
    fn drop(&mut self) {
        self.stop();
    }
}

// ── Helpers ────────────────────────────────────────────────────────

/// Calculate a normalised audio level (0.0–1.0) from PCM samples.
fn calculate_audio_level(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();

    #[allow(clippy::cast_precision_loss)]
    let rms = (sum_sq / samples.len() as f32).sqrt();

    // Map RMS to 0.0–1.0 (RMS of ~0.3 is very loud speech)
    (rms / 0.3).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_creates_in_idle_state() {
        let (pipeline, _rx) = VoicePipeline::new(VoicePipelineConfig::default());
        assert_eq!(pipeline.state(), VoiceState::Idle);
        assert!(!pipeline.is_active());
    }

    #[test]
    fn default_config_is_ptt() {
        let config = VoicePipelineConfig::default();
        assert_eq!(config.mode, VoiceInteractionMode::PushToTalk);
        assert!(config.auto_speak);
    }

    #[test]
    fn ptt_start_requires_active_pipeline() {
        let (mut pipeline, _rx) = VoicePipeline::new(VoicePipelineConfig::default());
        let result = pipeline.ptt_start();
        assert!(result.is_err());
    }

    #[test]
    fn audio_level_calculation() {
        #[allow(clippy::float_cmp)]
        {
            assert_eq!(calculate_audio_level(&[]), 0.0);
        }
        assert!(calculate_audio_level(&[0.1, 0.1, 0.1]) < 0.5);
        assert!(calculate_audio_level(&[0.3, 0.3, 0.3]) > 0.9);
    }
}
