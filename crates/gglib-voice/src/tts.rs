//! Text-to-Speech module — re-exports from [`crate::backend`].
//!
//! This module exists for backward compatibility. The canonical types are
//! in [`crate::backend`] (traits) and [`crate::backend::kokoro`] (Kokoro
//! implementation).
//!
//! ## Migration guide
//!
//! | Old import                          | New import                                    |
//! |-------------------------------------|-----------------------------------------------|
//! | `gglib_voice::tts::TtsEngine`       | `gglib_voice::backend::kokoro::KokoroBackend` |
//! | `gglib_voice::tts::TtsConfig`       | `gglib_voice::backend::TtsConfig`             |
//! | `gglib_voice::tts::VoiceInfo`       | `gglib_voice::backend::VoiceInfo`             |
//! | `gglib_voice::tts::VoiceGender`     | `gglib_voice::backend::VoiceGender`           |
//! | `gglib_voice::tts::KOKORO_SAMPLE_RATE` | `gglib_voice::backend::kokoro::KOKORO_SAMPLE_RATE` |

// Re-export backend types under their old names for compatibility.
pub use crate::backend::{TtsConfig, VoiceGender, VoiceInfo};

#[cfg(feature = "kokoro")]
pub use crate::backend::kokoro::{
    KOKORO_SAMPLE_RATE, KokoroBackend as TtsEngine, kokoro_voices,
};

#[cfg(feature = "kokoro")]
impl TtsEngine {
    /// List all available voices with metadata.
    ///
    /// Static method for backward compatibility (settings UI can call this
    /// without a loaded engine instance).
    #[must_use]
    pub fn available_voices() -> Vec<VoiceInfo> {
        kokoro_voices()
    }
}
