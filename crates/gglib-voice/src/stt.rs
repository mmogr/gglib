//! Speech-to-Text module — re-exports from [`crate::backend`].
//!
//! This module exists for backward compatibility. The canonical types are
//! in [`crate::backend`] (traits) and [`crate::backend::whisper`] (Whisper
//! implementation).
//!
//! ## Migration guide
//!
//! | Old import                        | New import                                      |
//! |-----------------------------------|-------------------------------------------------|
//! | `gglib_voice::stt::SttEngine`     | `gglib_voice::backend::whisper::WhisperBackend`  |
//! | `gglib_voice::stt::SttConfig`     | `gglib_voice::backend::SttConfig`                |

pub use crate::backend::SttConfig;

#[cfg(feature = "whisper")]
pub use crate::backend::whisper::WhisperBackend as SttEngine;
