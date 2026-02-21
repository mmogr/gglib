#![allow(clippy::doc_markdown)] // Generated README contains unbackticked identifiers
#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unused_crate_dependencies)]

#[cfg(test)]
use tempfile as _;
#[cfg(test)]
use tokio_test as _;

pub mod audio_io;
pub mod audio_local;
pub mod audio_thread;
pub mod backend;
pub mod capture;
pub mod error;
pub mod gate;
pub mod models;
pub mod pipeline;
pub mod playback;
pub mod service;
pub mod stt;
pub mod text_utils;
pub mod tts;
pub mod vad;

// Re-export key types for convenience
pub use error::VoiceError;
pub use gate::EchoGate;
pub use models::VadModelInfo;
pub use models::{SttModelInfo, TtsModelInfo, VoiceModelCatalog, VoiceModelId};
pub use pipeline::{VoiceEvent, VoicePipeline, VoicePipelineConfig, VoiceState};
pub use service::{RemoteAudioRegistry, VoiceService};

// Re-export backend trait types at crate root for ergonomic imports
pub use backend::{SttBackend, SttConfig, TtsAudio, TtsBackend, TtsConfig, VoiceGender, VoiceInfo};
