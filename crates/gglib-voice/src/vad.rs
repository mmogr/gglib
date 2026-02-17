//! Voice Activity Detection module — detects speech start/end in audio.
//!
//! Two detection strategies are supported:
//!
//! * **Silero VAD** (`sherpa` feature) — neural-network-based detection via
//!   `sherpa_rs::silero_vad::SileroVad`.  Produces high-quality utterance
//!   boundaries with minimal false positives.
//!
//! * **Energy-based** (fallback) — simple RMS energy thresholding.  Always
//!   available, used when no Silero model is loaded.
//!
//! In VAD mode the pipeline continuously monitors mic input and triggers
//! transcription when speech is detected followed by silence.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use sherpa_rs::silero_vad::{SileroVad, SileroVadConfig};
use std::path::Path;

use crate::error::VoiceError;
use crate::gate::EchoGate;

/// VAD configuration parameters.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VadConfig {
    /// Speech detection probability threshold (0.0–1.0, default 0.5).
    ///
    /// Higher values require more confidence before triggering speech detection.
    /// Lower values are more sensitive (may trigger on noise).
    pub threshold: f32,

    /// Minimum silence duration (ms) to consider speech ended (default 700).
    ///
    /// After speech is detected, this much continuous silence must pass
    /// before the utterance is considered complete and sent for transcription.
    pub min_silence_duration_ms: u32,

    /// Minimum speech duration (ms) to consider valid (default 250).
    ///
    /// Filters out very brief sounds (clicks, pops) that aren't real speech.
    pub min_speech_duration_ms: u32,

    /// Padding added around detected speech segments (ms, default 200).
    pub speech_pad_ms: u32,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            threshold: 0.5,
            min_silence_duration_ms: 700,
            min_speech_duration_ms: 250,
            speech_pad_ms: 200,
        }
    }
}

/// Current VAD state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VadState {
    /// Waiting for speech to start.
    Listening,

    /// Speech detected, accumulating audio.
    SpeechDetected,

    /// Silence detected after speech — utterance may be ending.
    SilenceAfterSpeech,
}

/// Voice Activity Detector.
///
/// Monitors an audio stream and emits events when speech starts/ends.
/// Integrates with the echo gate to avoid detecting TTS output as speech.
///
/// When a Silero VAD model is loaded (via [`load_silero_model`](Self::load_silero_model)),
/// neural-network-based detection is used.  Otherwise the detector falls
/// back to simple RMS energy thresholding.
pub struct VoiceActivityDetector {
    /// Current state of the VAD.
    state: VadState,

    /// Configuration parameters.
    config: VadConfig,

    /// Echo gate — skip detection when the system is speaking.
    echo_gate: EchoGate,

    /// Whether the VAD is actively monitoring.
    is_active: Arc<AtomicBool>,

    /// Audio buffer accumulating the current utterance.
    speech_buffer: Vec<f32>,

    /// Count of consecutive silent frames (for silence detection).
    silence_frame_count: u32,

    /// Count of consecutive speech frames (for speech detection).
    speech_frame_count: u32,

    /// Audio sample rate we're processing at.
    sample_rate: u32,

    /// Optional Silero VAD neural-network detector.
    ///
    /// When loaded, [`process_frame`](Self::process_frame) delegates to
    /// Silero instead of the energy-based detector.
    silero: Option<SileroVad>,
}

/// Events emitted by the VAD.
#[derive(Debug, Clone)]
pub enum VadEvent {
    /// Speech has started.
    SpeechStart,

    /// Speech has ended — contains the complete utterance audio (16 kHz mono).
    SpeechEnd {
        /// The captured speech audio (16 kHz mono f32 PCM).
        audio: Vec<f32>,
    },

    /// VAD is listening (periodic heartbeat).
    Listening,
}

impl VoiceActivityDetector {
    /// Create a new VAD with the given configuration.
    pub fn new(config: VadConfig, echo_gate: EchoGate, sample_rate: u32) -> Self {
        Self {
            state: VadState::Listening,
            config,
            echo_gate,
            is_active: Arc::new(AtomicBool::new(false)),
            speech_buffer: Vec::new(),
            silence_frame_count: 0,
            speech_frame_count: 0,
            sample_rate,
            silero: None,
        }
    }

    /// Load a Silero VAD model from disk.
    ///
    /// Once loaded, [`process_frame`](Self::process_frame) will use the
    /// neural-network detector instead of plain energy thresholding.
    ///
    /// The `model_path` should point to the Silero VAD ONNX model file
    /// (e.g. `silero_vad.onnx`).
    pub fn load_silero_model(&mut self, model_path: &Path) -> Result<(), VoiceError> {
        if !model_path.exists() {
            return Err(VoiceError::ModelNotFound(model_path.to_path_buf()));
        }

        let model_str = model_path
            .to_str()
            .ok_or_else(|| VoiceError::ModelNotFound(model_path.to_path_buf()))?;

        let silero_config = SileroVadConfig {
            model: model_str.to_string(),
            threshold: self.config.threshold,
            #[allow(clippy::cast_precision_loss)]
            min_silence_duration: self.config.min_silence_duration_ms as f32 / 1000.0,
            #[allow(clippy::cast_precision_loss)]
            min_speech_duration: self.config.min_speech_duration_ms as f32 / 1000.0,
            sample_rate: self.sample_rate,
            ..SileroVadConfig::default()
        };

        // Buffer up to 60 seconds of speech.
        let buffer_size_secs: f32 = 60.0;

        let vad = SileroVad::new(silero_config, buffer_size_secs)
            .map_err(|e| VoiceError::ModelLoadError(format!("Failed to load Silero VAD: {e}")))?;

        tracing::info!(path = %model_path.display(), "Silero VAD model loaded");
        self.silero = Some(vad);
        Ok(())
    }

    /// Start the VAD monitoring.
    pub fn start(&mut self) {
        self.is_active.store(true, Ordering::SeqCst);
        self.reset();
        tracing::debug!("VAD started");
    }

    /// Stop the VAD monitoring.
    pub fn stop(&mut self) {
        self.is_active.store(false, Ordering::SeqCst);
        self.reset();
        tracing::debug!("VAD stopped");
    }

    /// Whether the VAD is currently active.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.is_active.load(Ordering::SeqCst)
    }

    /// Process a frame of audio samples and return any VAD events.
    ///
    /// The audio should be at the configured sample rate (typically 16 kHz mono).
    ///
    /// When a Silero model is loaded the neural-network detector is used;
    /// otherwise falls back to simple RMS energy thresholding.
    pub fn process_frame(&mut self, frame: &[f32]) -> Option<VadEvent> {
        if !self.is_active.load(Ordering::Relaxed) {
            return None;
        }

        // Echo gate: ignore all audio while system is speaking
        if self.echo_gate.is_speaking() {
            // If we were accumulating speech, discard it
            if self.state != VadState::Listening {
                self.reset();
            }
            return None;
        }

        // Delegate to Silero when available.
        if self.silero.is_some() {
            return self.process_frame_silero(frame);
        }

        self.process_frame_energy(frame)
    }

    // ── Silero neural-network path ─────────────────────────────────

    /// Process a frame using the Silero VAD neural network.
    ///
    /// Silero internally tracks speech state and produces [`SpeechSegment`]s
    /// that contain the start offset and the captured audio samples.  We
    /// translate those into our [`VadEvent`] protocol.
    fn process_frame_silero(&mut self, frame: &[f32]) -> Option<VadEvent> {
        let silero = self.silero.as_mut().expect("checked by caller");

        // Feed samples to Silero.
        silero.accept_waveform(frame.to_vec());

        if silero.is_speech() && self.state == VadState::Listening {
            // Transition: speech just started.
            self.state = VadState::SpeechDetected;
            tracing::debug!("Silero VAD: speech started");
            return Some(VadEvent::SpeechStart);
        }

        if !silero.is_speech() && self.state == VadState::SpeechDetected {
            // Speech ended — drain all queued segments into a single buffer.
            self.state = VadState::Listening;

            // Flush any remaining samples so they appear as segments.
            silero.flush();

            let mut audio = Vec::new();
            while !silero.is_empty() {
                let seg = silero.front();
                audio.extend_from_slice(&seg.samples);
                silero.pop();
            }

            if audio.is_empty() {
                return None;
            }

            tracing::debug!(
                samples = audio.len(),
                duration_ms = audio.len() as u64 * 1000 / u64::from(self.sample_rate),
                "Silero VAD: speech ended"
            );

            return Some(VadEvent::SpeechEnd { audio });
        }

        None
    }

    // ── Energy-based fallback path ─────────────────────────────────

    /// Process a frame using simple RMS energy thresholding (fallback).
    fn process_frame_energy(&mut self, frame: &[f32]) -> Option<VadEvent> {
        let energy = calculate_rms_energy(frame);
        let is_speech = energy > energy_threshold_from_vad_threshold(self.config.threshold);

        // Frame duration in ms
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let frame_duration_ms = (frame.len() as f32 / self.sample_rate as f32 * 1000.0) as u32;

        match self.state {
            VadState::Listening => {
                if is_speech {
                    self.speech_frame_count += frame_duration_ms;
                    self.speech_buffer.extend_from_slice(frame);

                    if self.speech_frame_count >= self.config.min_speech_duration_ms {
                        self.state = VadState::SpeechDetected;
                        self.silence_frame_count = 0;
                        tracing::debug!(
                            energy,
                            "VAD: speech detected ({}ms)",
                            self.speech_frame_count
                        );
                        return Some(VadEvent::SpeechStart);
                    }
                } else if self.speech_frame_count > 0 {
                    // Reset speech counter if silence interrupts before min duration
                    self.speech_frame_count = 0;
                    self.speech_buffer.clear();
                }
            }

            VadState::SpeechDetected => {
                self.speech_buffer.extend_from_slice(frame);

                if is_speech {
                    // Reset silence counter — speech resumed
                    self.silence_frame_count = 0;
                } else {
                    self.silence_frame_count += frame_duration_ms;
                    if self.silence_frame_count >= self.config.min_silence_duration_ms {
                        self.state = VadState::SilenceAfterSpeech;
                    }
                }
            }

            VadState::SilenceAfterSpeech => {
                // Utterance is complete — emit the buffered audio
                let audio = std::mem::take(&mut self.speech_buffer);
                self.reset();

                tracing::debug!(
                    samples = audio.len(),
                    duration_ms = audio.len() as u64 * 1000 / u64::from(self.sample_rate),
                    "VAD: speech ended"
                );

                return Some(VadEvent::SpeechEnd { audio });
            }
        }

        None
    }

    /// Reset VAD state (clear buffers, go back to listening).
    fn reset(&mut self) {
        self.state = VadState::Listening;
        self.speech_buffer.clear();
        self.silence_frame_count = 0;
        self.speech_frame_count = 0;

        // Clear any buffered state in the Silero detector.
        if let Some(ref mut silero) = self.silero {
            silero.clear();
        }
    }

    /// Get the current VAD state.
    #[must_use]
    pub const fn state(&self) -> VadState {
        self.state
    }

    /// Update VAD configuration.
    pub const fn set_config(&mut self, config: VadConfig) {
        self.config = config;
    }
}

/// Calculate RMS (Root Mean Square) energy of an audio frame.
fn calculate_rms_energy(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_squares: f32 = samples.iter().map(|&s| s * s).sum();

    #[allow(clippy::cast_precision_loss)]
    let mean = sum_squares / samples.len() as f32;

    mean.sqrt()
}

/// Map VAD threshold (0.0–1.0) to an RMS energy threshold.
///
/// Lower VAD threshold → more sensitive (lower energy threshold).
/// Higher VAD threshold → less sensitive (higher energy threshold).
fn energy_threshold_from_vad_threshold(vad_threshold: f32) -> f32 {
    // Map [0.0, 1.0] → [0.001, 0.05] RMS energy range
    // 0.01 is a reasonable default for normal speech
    let min_energy: f32 = 0.001;
    let max_energy: f32 = 0.05;
    (max_energy - min_energy).mul_add(vad_threshold, min_energy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vad_starts_in_listening_state() {
        let gate = crate::gate::EchoGate::new();
        let vad = VoiceActivityDetector::new(VadConfig::default(), gate, 16_000);
        assert_eq!(vad.state(), VadState::Listening);
    }

    #[test]
    fn vad_ignores_audio_when_system_speaking() {
        let gate = crate::gate::EchoGate::new();
        let mut vad = VoiceActivityDetector::new(VadConfig::default(), gate.clone(), 16_000);
        vad.start();

        gate.start_speaking();

        // Send loud audio — should be ignored
        #[allow(clippy::cast_precision_loss)]
        let loud_frame: Vec<f32> = (0..1600).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
        let event = vad.process_frame(&loud_frame);
        assert!(event.is_none());
        assert_eq!(vad.state(), VadState::Listening);
    }

    #[test]
    fn rms_energy_calculation() {
        // Silence
        let silence = vec![0.0f32; 100];
        assert!((calculate_rms_energy(&silence) - 0.0).abs() < f32::EPSILON);

        // Full-scale signal
        let loud = vec![1.0f32; 100];
        assert!((calculate_rms_energy(&loud) - 1.0).abs() < f32::EPSILON);

        // Empty
        assert!((calculate_rms_energy(&[]) - 0.0).abs() < f32::EPSILON);
    }
}
