#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unused_crate_dependencies)]

// ort is used transitively by kokoro-tts features (coreml, cuda)
use ort as _;

pub mod capture;
pub mod error;
pub mod gate;
pub mod models;
pub mod pipeline;
pub mod playback;
pub mod stt;
pub mod tts;
pub mod vad;

// Re-export key types for convenience
pub use error::VoiceError;
pub use gate::EchoGate;
pub use models::{SttModelInfo, TtsModelInfo, VoiceModelCatalog, VoiceModelId};
pub use pipeline::{VoiceEvent, VoicePipeline, VoicePipelineConfig, VoiceState};
