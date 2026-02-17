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

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::audio_thread::AudioThreadHandle;
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
    /// Current state.
    state: VoiceState,

    /// Interaction mode.
    mode: VoiceInteractionMode,

    /// Shared echo gate.
    echo_gate: EchoGate,

    /// Audio I/O actor — owns capture + playback on a dedicated OS thread.
    audio: Option<AudioThreadHandle>,

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

    /// Pipeline configuration.
    config: VoicePipelineConfig,
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
            state: VoiceState::Idle,
            mode: config.mode,
            echo_gate,
            audio: None,
            stt: None,
            tts: None,
            vad: None,
            event_tx,
            is_active: Arc::new(AtomicBool::new(false)),
            config,
        };

        (pipeline, event_rx)
    }

    /// Get the current pipeline state.
    #[must_use]
    pub const fn state(&self) -> VoiceState {
        self.state
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

    // ── Lifecycle ──────────────────────────────────────────────────

    /// Start the voice pipeline.
    ///
    /// Initialises audio capture and playback. STT and TTS engines are
    /// loaded lazily on first use.
    pub fn start(&mut self) -> Result<(), VoiceError> {
        if self.is_active() {
            return Err(VoiceError::AlreadyActive);
        }

        tracing::info!(mode = ?self.mode, "Starting voice pipeline");

        // Initialise audio I/O on a dedicated OS thread.
        let audio = AudioThreadHandle::spawn(self.echo_gate.clone())?;
        self.audio = Some(audio);

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

    /// Stop the voice pipeline and release all resources.
    pub fn stop(&mut self) {
        tracing::info!("Stopping voice pipeline");

        // Stop any active recording / playback and join the audio thread.
        if let Some(ref audio) = self.audio {
            if audio.is_recording() {
                let _ = audio.stop_capture();
            }
            audio.stop_playback();
        }
        // Drop the handle — sends Shutdown and joins the thread.
        self.audio.take();

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
    pub fn load_stt(&mut self, model_path: &std::path::Path) -> Result<(), VoiceError> {
        use crate::backend::sherpa_stt::{SherpaSttBackend, SherpaSttConfig};

        tracing::info!(path = %model_path.display(), "Loading STT engine");

        let sherpa_config = SherpaSttConfig {
            language: self.config.stt.language.clone(),
            ..SherpaSttConfig::default()
        };
        let engine = SherpaSttBackend::load(model_path, &sherpa_config)?;
        self.stt = Some(Box::new(engine));
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
    pub fn ptt_start(&mut self) -> Result<(), VoiceError> {
        if !self.is_active() {
            return Err(VoiceError::NotActive);
        }

        // Stop any active playback first
        let audio = self.audio.as_ref().ok_or(VoiceError::NotActive)?;
        audio.stop_playback();
        audio.start_capture()?;
        self.set_state(VoiceState::Recording);

        Ok(())
    }

    /// Finish recording and transcribe (PTT mode: user released the talk button).
    ///
    /// Returns the transcribed text. Also emits a `VoiceEvent::Transcript`.
    pub fn ptt_stop(&mut self) -> Result<String, VoiceError> {
        if !self.is_active() {
            return Err(VoiceError::NotActive);
        }

        let audio_handle = self.audio.as_ref().ok_or(VoiceError::NotActive)?;
        let audio = audio_handle.stop_capture()?;

        if audio.is_empty() {
            self.set_state(VoiceState::Listening);
            return Ok(String::new());
        }

        self.set_state(VoiceState::Transcribing);

        let stt = self.stt.as_ref().ok_or(VoiceError::SttModelNotLoaded)?;
        let text = stt.transcribe(&audio)?;

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
    pub fn vad_process_frame(&mut self, frame: &[f32]) -> Result<Option<String>, VoiceError> {
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
                let text = stt.transcribe(&audio)?;

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
    pub async fn speak(&mut self, text: &str) -> Result<(), VoiceError> {
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
        let audio = self
            .audio
            .as_ref()
            .ok_or_else(|| VoiceError::OutputStreamError("Audio thread not running".to_string()))?;
        audio.start_streaming()?;

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
                        // Can't use self.set_state/emit here because `tts`
                        // holds an immutable borrow on self.tts.
                        self.state = VoiceState::Speaking;
                        let _ = self
                            .event_tx
                            .send(VoiceEvent::StateChanged(VoiceState::Speaking));
                        let _ = self.event_tx.send(VoiceEvent::SpeakingStarted);
                    }

                    // audio_thread methods take &self, so no re-borrow needed.
                    let a = self.audio.as_ref().expect("audio thread started above");
                    a.append(audio.samples, audio.sample_rate)?;
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
            if let Some(ref a) = self.audio {
                a.stop_playback();
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

        let audio = self.audio.as_ref().expect("audio thread started above");
        audio.spawn_completion_watcher(Some(on_done));

        Ok(())
    }

    /// Stop any active TTS playback immediately.
    pub fn stop_speaking(&mut self) {
        if let Some(ref audio) = self.audio {
            audio.stop_playback();
        }
        self.emit(VoiceEvent::SpeakingFinished);
        if self.is_active() {
            self.set_state(VoiceState::Listening);
        }
    }

    /// Check if TTS playback is currently active.
    #[must_use]
    pub fn is_speaking(&self) -> bool {
        self.audio
            .as_ref()
            .is_some_and(|a| a.is_playing())
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
    fn set_state(&mut self, new_state: VoiceState) {
        if self.state != new_state {
            tracing::debug!(old = ?self.state, new = ?new_state, "Voice state transition");
            self.state = new_state;
            self.emit(VoiceEvent::StateChanged(new_state));
        }
    }

    /// Emit a voice event (best-effort — if the receiver is dropped, we log and move on).
    fn emit(&self, event: VoiceEvent) {
        if self.event_tx.send(event).is_err() {
            tracing::warn!("Voice event receiver dropped");
        }
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
