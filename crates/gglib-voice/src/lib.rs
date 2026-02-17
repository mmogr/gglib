#![allow(clippy::doc_markdown)] // Generated README contains unbackticked identifiers
#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unused_crate_dependencies)]

// ort is used transitively by kokoro-tts features (coreml, cuda)
#[cfg(feature = "kokoro")]
use ort as _;

#[cfg(test)]
use tempfile as _;
#[cfg(test)]
use tokio_test as _;

pub mod backend;
pub mod capture;
pub mod error;
pub mod gate;
pub mod models;
pub mod pipeline;
pub mod playback;
pub mod stt;
pub mod text_utils;
pub mod tts;
pub mod vad;

// Re-export key types for convenience
pub use error::VoiceError;
pub use gate::EchoGate;
pub use models::{SttModelInfo, TtsModelInfo, VoiceModelCatalog, VoiceModelId};
#[cfg(feature = "sherpa")]
pub use models::VadModelInfo;
pub use pipeline::{VoiceEvent, VoicePipeline, VoicePipelineConfig, VoiceState};

// Re-export backend trait types at crate root for ergonomic imports
pub use backend::{SttBackend, SttConfig, TtsAudio, TtsBackend, TtsConfig, VoiceGender, VoiceInfo};
