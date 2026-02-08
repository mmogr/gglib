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

use crate::capture::AudioCapture;
use crate::error::VoiceError;
use crate::gate::EchoGate;
use crate::playback::AudioPlayback;
use crate::stt::{SttConfig, SttEngine};
use crate::tts::{KOKORO_SAMPLE_RATE, TtsConfig, TtsEngine};
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

    /// Transcribing captured audio via whisper.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoiceInteractionMode {
    /// User holds a button to record, releases to send.
    PushToTalk,

    /// Continuous listening with automatic speech boundary detection.
    VoiceActivityDetection,
}

impl Default for VoiceInteractionMode {
    fn default() -> Self {
        Self::PushToTalk
    }
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
}

impl Default for VoicePipelineConfig {
    fn default() -> Self {
        Self {
            mode: VoiceInteractionMode::PushToTalk,
            stt: SttConfig::default(),
            tts: TtsConfig::default(),
            vad: VadConfig::default(),
            auto_speak: true,
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

    /// Audio capture (microphone).
    capture: Option<AudioCapture>,

    /// Audio playback (speakers).
    playback: Option<AudioPlayback>,

    /// Speech-to-text engine (loaded lazily).
    stt: Option<SttEngine>,

    /// Text-to-speech engine (loaded lazily).
    tts: Option<TtsEngine>,

    /// Voice activity detector (used in VAD mode).
    vad: Option<VoiceActivityDetector>,

    /// Event sender channel.
    event_tx: mpsc::UnboundedSender<VoiceEvent>,

    /// Whether the pipeline is active.
    is_active: Arc<AtomicBool>,

    /// Pipeline configuration.
    config: VoicePipelineConfig,
}

// Safety: VoicePipeline is always accessed behind a tokio::sync::RwLock in AppState,
// ensuring exclusive mutable access. The only !Send field is cpal::Stream inside
// AudioCapture, which is only created/accessed through pipeline methods. On macOS
// (CoreAudio) the stream is actually thread-safe; the !Send marker is a conservative
// cross-platform constraint in cpal.
#[allow(unsafe_code, clippy::non_send_fields_in_send_ty)]
unsafe impl Send for VoicePipeline {}
#[allow(unsafe_code, clippy::non_send_fields_in_send_ty)]
unsafe impl Sync for VoicePipeline {}

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
            capture: None,
            playback: None,
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

        // Initialise audio I/O
        let capture = AudioCapture::new(self.echo_gate.clone())?;
        let playback = AudioPlayback::new(self.echo_gate.clone())?;

        self.capture = Some(capture);
        self.playback = Some(playback);

        // In VAD mode, initialise the detector
        if self.mode == VoiceInteractionMode::VoiceActivityDetection {
            let mut vad = VoiceActivityDetector::new(
                self.config.vad.clone(),
                self.echo_gate.clone(),
                crate::capture::WHISPER_SAMPLE_RATE,
            );
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

        // Stop any active recording
        if let Some(ref mut capture) = self.capture {
            if capture.is_recording() {
                let _ = capture.stop_recording();
            }
        }

        // Stop any active playback
        if let Some(ref mut playback) = self.playback {
            playback.stop();
        }

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
    pub fn load_stt(&mut self, model_path: &std::path::Path) -> Result<(), VoiceError> {
        tracing::info!(path = %model_path.display(), "Loading STT engine");
        let engine = SttEngine::load(model_path, &self.config.stt)?;
        self.stt = Some(engine);
        Ok(())
    }

    /// Load or replace the TTS engine with model files at the given paths.
    pub async fn load_tts(
        &mut self,
        model_path: &std::path::Path,
        voices_path: &std::path::Path,
    ) -> Result<(), VoiceError> {
        tracing::info!(
            model = %model_path.display(),
            voices = %voices_path.display(),
            "Loading TTS engine"
        );
        let engine = TtsEngine::load(model_path, voices_path, &self.config.tts).await?;
        self.tts = Some(engine);
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
        if let Some(ref mut playback) = self.playback {
            playback.stop();
        }

        let capture = self.capture.as_mut().ok_or(VoiceError::NoInputDevice)?;
        capture.start_recording()?;
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

        let capture = self.capture.as_mut().ok_or(VoiceError::NoInputDevice)?;
        let audio = capture.stop_recording()?;

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
    /// Sets the echo gate to suppress mic capture during playback.
    pub async fn speak(&mut self, text: &str) -> Result<(), VoiceError> {
        if text.trim().is_empty() {
            return Ok(());
        }

        let tts = self.tts.as_ref().ok_or(VoiceError::TtsModelNotLoaded)?;
        let (samples, duration) = tts.synthesize(text).await?;

        tracing::debug!(
            text_len = text.len(),
            duration_ms = duration.as_millis(),
            "Synthesised speech, starting playback"
        );

        self.set_state(VoiceState::Speaking);
        self.emit(VoiceEvent::SpeakingStarted);

        let playback = self
            .playback
            .as_mut()
            .ok_or_else(|| VoiceError::OutputStreamError("Playback not initialized".to_string()))?;

        playback.play_with_gate_management(samples, KOKORO_SAMPLE_RATE)?;

        Ok(())
    }

    /// Stop any active TTS playback immediately.
    pub fn stop_speaking(&mut self) {
        if let Some(ref mut playback) = self.playback {
            playback.stop();
        }
        self.emit(VoiceEvent::SpeakingFinished);
        if self.is_active() {
            self.set_state(VoiceState::Listening);
        }
    }

    /// Check if TTS playback is currently active.
    #[must_use]
    pub fn is_speaking(&self) -> bool {
        self.playback.as_ref().is_some_and(AudioPlayback::is_playing)
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
                        crate::capture::WHISPER_SAMPLE_RATE,
                    );
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
