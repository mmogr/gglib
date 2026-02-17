//! Speech-to-Text module — re-exports from [`crate::backend`].
//!
//! This module exists for backward compatibility. The canonical types are
//! in [`crate::backend`] (traits) and the backend-specific modules.

pub use crate::backend::SttConfig;

// ── sherpa backend ─────────────────────────────────────────────────

#[cfg(feature = "sherpa")]
pub use crate::backend::sherpa_stt::SherpaSttBackend as SttEngine;

// ── legacy whisper backend ─────────────────────────────────────────

#[cfg(feature = "whisper")]
pub use crate::backend::whisper::WhisperBackend as SttEngine;
