//! Text-to-Speech module — re-exports from [`crate::backend`].
//!
//! This module exists for backward compatibility. The canonical types are
//! in [`crate::backend`] (traits) and the backend-specific modules.

// Re-export backend types under their old names for compatibility.
pub use crate::backend::{TtsConfig, VoiceGender, VoiceInfo};

pub use crate::backend::sherpa_tts::{
    SHERPA_TTS_SAMPLE_RATE, SherpaTtsBackend as TtsEngine, sherpa_kokoro_voices,
};

impl TtsEngine {
    /// List all available voices with metadata.
    ///
    /// Static method for backward compatibility (settings UI can call this
    /// without a loaded engine instance).
    #[must_use]
    pub fn available_voices() -> Vec<VoiceInfo> {
        sherpa_kokoro_voices()
    }
}
