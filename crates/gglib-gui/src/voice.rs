//! Voice data & config operations delegating to `VoicePipelinePort`.

use std::sync::Arc;

use gglib_core::ports::{AudioDeviceDto, VoiceModelsDto, VoicePipelinePort, VoiceStatusDto};

use crate::deps::GuiDeps;
use crate::error::GuiError;

/// Voice operations handler — thin delegates over `VoicePipelinePort`.
///
/// Follows the same lifetime-borrow pattern as `DownloadOps`, `ModelOps`, etc.
pub struct VoiceOps<'a> {
    voice: &'a Arc<dyn VoicePipelinePort>,
}

impl<'a> VoiceOps<'a> {
    pub fn new(deps: &'a GuiDeps) -> Self {
        Self { voice: &deps.voice }
    }

    pub async fn status(&self) -> Result<VoiceStatusDto, GuiError> {
        self.voice.status().await.map_err(GuiError::from)
    }

    pub async fn list_models(&self) -> Result<VoiceModelsDto, GuiError> {
        self.voice.list_models().await.map_err(GuiError::from)
    }

    pub async fn download_stt_model(&self, model_id: &str) -> Result<(), GuiError> {
        self.voice
            .download_stt_model(model_id)
            .await
            .map_err(GuiError::from)
    }

    pub async fn download_tts_model(&self) -> Result<(), GuiError> {
        self.voice
            .download_tts_model()
            .await
            .map_err(GuiError::from)
    }

    pub async fn download_vad_model(&self) -> Result<(), GuiError> {
        self.voice
            .download_vad_model()
            .await
            .map_err(GuiError::from)
    }

    pub async fn load_stt(&self, model_id: &str) -> Result<(), GuiError> {
        self.voice.load_stt(model_id).await.map_err(GuiError::from)
    }

    pub async fn load_tts(&self) -> Result<(), GuiError> {
        self.voice.load_tts().await.map_err(GuiError::from)
    }

    pub async fn set_mode(&self, mode: &str) -> Result<(), GuiError> {
        self.voice.set_mode(mode).await.map_err(GuiError::from)
    }

    pub async fn set_voice(&self, voice_id: &str) -> Result<(), GuiError> {
        self.voice.set_voice(voice_id).await.map_err(GuiError::from)
    }

    pub async fn set_speed(&self, speed: f32) -> Result<(), GuiError> {
        self.voice.set_speed(speed).await.map_err(GuiError::from)
    }

    pub async fn set_auto_speak(&self, enabled: bool) -> Result<(), GuiError> {
        self.voice
            .set_auto_speak(enabled)
            .await
            .map_err(GuiError::from)
    }

    pub async fn unload(&self) -> Result<(), GuiError> {
        self.voice.unload().await.map_err(GuiError::from)
    }

    pub async fn list_devices(&self) -> Result<Vec<AudioDeviceDto>, GuiError> {
        self.voice.list_devices().await.map_err(GuiError::from)
    }

    // ── Audio I/O ───────────────────────────────────────────────────────────

    pub async fn start(&self, mode: Option<String>) -> Result<(), GuiError> {
        self.voice.start(mode).await.map_err(GuiError::from)
    }

    pub async fn stop(&self) -> Result<(), GuiError> {
        self.voice.stop().await.map_err(GuiError::from)
    }

    pub async fn ptt_start(&self) -> Result<(), GuiError> {
        self.voice.ptt_start().await.map_err(GuiError::from)
    }

    pub async fn ptt_stop(&self) -> Result<String, GuiError> {
        self.voice.ptt_stop().await.map_err(GuiError::from)
    }

    pub async fn speak(&self, text: &str) -> Result<(), GuiError> {
        self.voice.speak(text).await.map_err(GuiError::from)
    }

    pub async fn stop_speaking(&self) -> Result<(), GuiError> {
        self.voice.stop_speaking().await.map_err(GuiError::from)
    }
}
