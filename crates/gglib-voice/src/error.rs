//! Voice mode error types.

use std::path::PathBuf;

/// Errors that can occur in the voice pipeline.
#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    /// No audio input device found.
    #[error("No audio input device found")]
    NoInputDevice,

    /// Failed to open audio input stream.
    #[error("Failed to open audio input stream: {0}")]
    InputStreamError(String),

    /// Failed to open audio output stream.
    #[error("Failed to open audio output stream: {0}")]
    OutputStreamError(String),

    /// Microphone permission denied.
    #[error("Microphone permission denied")]
    MicrophonePermissionDenied,

    /// STT model not loaded.
    #[error("STT model not loaded — download a whisper model first")]
    SttModelNotLoaded,

    /// TTS model not loaded.
    #[error("TTS model not loaded — download Kokoro TTS model first")]
    TtsModelNotLoaded,

    /// Model file not found at expected path.
    #[error("Voice model not found at {0}")]
    ModelNotFound(PathBuf),

    /// Failed to load whisper model.
    #[error("Failed to load whisper model: {0}")]
    WhisperLoadError(String),

    /// Failed to transcribe audio.
    #[error("Transcription failed: {0}")]
    TranscriptionError(String),

    /// Failed to synthesize speech.
    #[error("Speech synthesis failed: {0}")]
    SynthesisError(String),

    /// Failed to download voice model.
    #[error("Failed to download voice model '{name}': {source}")]
    DownloadError { name: String, source: anyhow::Error },

    /// Audio resampling error.
    #[error("Audio resampling failed: {0}")]
    ResampleError(String),

    /// Voice pipeline is already active.
    #[error("Voice pipeline is already active")]
    AlreadyActive,

    /// Voice pipeline is not active.
    #[error("Voice pipeline is not active")]
    NotActive,

    /// IO error (model files, data directory).
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Pipeline was cancelled.
    #[error("Voice operation cancelled")]
    Cancelled,
}
