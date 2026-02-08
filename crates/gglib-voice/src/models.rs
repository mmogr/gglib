//! Voice model catalog — curated list of STT and TTS models.
//!
//! Rather than extending the HuggingFace browser (which is GGUF-specific),
//! voice models use a curated catalog with deterministic download URLs.
//! All whisper models come from `ggerganov/whisper.cpp` on HuggingFace.
//! Kokoro TTS model files come from the `kokoro-tts` releases.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ── Model identifiers ──────────────────────────────────────────────

/// Unique identifier for a voice model.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceModelId(pub String);

impl std::fmt::Display for VoiceModelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── STT model catalog ──────────────────────────────────────────────

/// Information about a whisper STT model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SttModelInfo {
    /// Model identifier (e.g., "base.en").
    pub id: VoiceModelId,

    /// Human-readable name.
    pub name: String,

    /// Filename on HuggingFace (e.g., "ggml-base.en.bin").
    pub filename: String,

    /// Download URL.
    pub url: String,

    /// Approximate file size in bytes.
    pub size_bytes: u64,

    /// Approximate file size as human-readable string.
    pub size_display: String,

    /// Language support: true = English-only (faster), false = multilingual.
    pub english_only: bool,

    /// Quality rating (1–5 stars).
    pub quality: u8,

    /// Relative speed rating (1 = fastest, 5 = slowest).
    pub speed: u8,

    /// Whether this is the recommended default model.
    pub is_default: bool,
}

/// Information about a TTS model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsModelInfo {
    /// Model identifier.
    pub id: VoiceModelId,

    /// Human-readable name.
    pub name: String,

    /// Model ONNX filename.
    pub model_filename: String,

    /// Voice styles filename.
    pub voices_filename: String,

    /// Download URL for the model file.
    pub model_url: String,

    /// Download URL for the voices file.
    pub voices_url: String,

    /// Approximate total size in bytes (model + voices).
    pub size_bytes: u64,

    /// Approximate size as human-readable string.
    pub size_display: String,

    /// Number of available voices.
    pub voice_count: u32,
}

// ── Catalog ────────────────────────────────────────────────────────

const HF_WHISPER_BASE: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

const KOKORO_RELEASE_BASE: &str = "https://github.com/mzdk100/kokoro/releases/download";

/// The curated voice model catalog.
///
/// Provides fixed lists of known-good STT and TTS models with
/// deterministic download URLs.
pub struct VoiceModelCatalog;

impl VoiceModelCatalog {
    /// Get all available STT (whisper) models.
    #[must_use]
    pub fn stt_models() -> Vec<SttModelInfo> {
        vec![
            stt_model(
                "tiny.en",
                "Tiny (English)",
                "ggml-tiny.en.bin",
                77_691_713,
                "75 MB",
                true,
                1,
                1,
                false,
            ),
            stt_model(
                "base.en",
                "Base (English)",
                "ggml-base.en.bin",
                147_951_465,
                "142 MB",
                true,
                3,
                2,
                true, // default
            ),
            stt_model(
                "small.en",
                "Small (English)",
                "ggml-small.en.bin",
                487_601_817,
                "466 MB",
                true,
                4,
                3,
                false,
            ),
            stt_model(
                "medium.en",
                "Medium (English)",
                "ggml-medium.en.bin",
                1_533_774_781,
                "1.5 GB",
                true,
                5,
                4,
                false,
            ),
            stt_model(
                "tiny",
                "Tiny (Multilingual)",
                "ggml-tiny.bin",
                77_691_713,
                "75 MB",
                false,
                1,
                1,
                false,
            ),
            stt_model(
                "base",
                "Base (Multilingual)",
                "ggml-base.bin",
                147_964_211,
                "142 MB",
                false,
                3,
                2,
                false,
            ),
            stt_model(
                "small",
                "Small (Multilingual)",
                "ggml-small.bin",
                487_626_545,
                "466 MB",
                false,
                4,
                3,
                false,
            ),
            stt_model(
                "large-v3-turbo",
                "Large V3 Turbo (Multilingual)",
                "ggml-large-v3-turbo.bin",
                1_622_081_457,
                "1.5 GB",
                false,
                5,
                4,
                false,
            ),
            // Quantized variants
            stt_model(
                "base.en-q5_0",
                "Base Q5_0 (English)",
                "ggml-base.en-q5_0.bin",
                57_348_577,
                "55 MB",
                true,
                2,
                1,
                false,
            ),
            stt_model(
                "small.en-q5_1",
                "Small Q5_1 (English)",
                "ggml-small.en-q5_1.bin",
                190_852_577,
                "182 MB",
                true,
                3,
                2,
                false,
            ),
            stt_model(
                "medium.en-q5_0",
                "Medium Q5_0 (English)",
                "ggml-medium.en-q5_0.bin",
                539_212_577,
                "515 MB",
                true,
                4,
                3,
                false,
            ),
            stt_model(
                "large-v3-turbo-q5_0",
                "Large V3 Turbo Q5_0",
                "ggml-large-v3-turbo-q5_0.bin",
                574_041_889,
                "548 MB",
                false,
                4,
                3,
                false,
            ),
        ]
    }

    /// Get the default STT model info.
    #[must_use]
    pub fn default_stt_model() -> SttModelInfo {
        Self::stt_models()
            .into_iter()
            .find(|m| m.is_default)
            .expect("catalog must have a default STT model")
    }

    /// Find an STT model by ID.
    #[must_use]
    pub fn find_stt_model(id: &str) -> Option<SttModelInfo> {
        Self::stt_models().into_iter().find(|m| m.id.0 == id)
    }

    /// Get the TTS model info (Kokoro v1.0).
    #[must_use]
    pub fn tts_model() -> TtsModelInfo {
        TtsModelInfo {
            id: VoiceModelId("kokoro-v1.0".to_string()),
            name: "Kokoro v1.0 (82M)".to_string(),
            model_filename: "kokoro-v1.0.onnx".to_string(),
            voices_filename: "voices.bin".to_string(),
            model_url: format!("{KOKORO_RELEASE_BASE}/V1.0/kokoro-v1.0.onnx"),
            voices_url: format!("{KOKORO_RELEASE_BASE}/V1.0/voices.bin"),
            size_bytes: 330_000_000,
            size_display: "~330 MB".to_string(),
            voice_count: 27,
        }
    }

    /// Get the directory where voice models are stored.
    ///
    /// Returns `{data_root}/voice_models/` (e.g., `~/.local/share/gglib/voice_models/`).
    pub fn voice_models_dir() -> Result<PathBuf, crate::error::VoiceError> {
        let data_root = gglib_core::paths::data_root()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::NotFound, e.to_string()))?;
        Ok(data_root.join("voice_models"))
    }

    /// Get the path where an STT model would be stored on disk.
    pub fn stt_model_path(model: &SttModelInfo) -> Result<PathBuf, crate::error::VoiceError> {
        Ok(Self::voice_models_dir()?.join("stt").join(&model.filename))
    }

    /// Get the path where the TTS model ONNX file would be stored on disk.
    pub fn tts_model_path() -> Result<PathBuf, crate::error::VoiceError> {
        let tts = Self::tts_model();
        Ok(Self::voice_models_dir()?
            .join("tts")
            .join(&tts.model_filename))
    }

    /// Get the path where the TTS voices file would be stored on disk.
    pub fn tts_voices_path() -> Result<PathBuf, crate::error::VoiceError> {
        let tts = Self::tts_model();
        Ok(Self::voice_models_dir()?
            .join("tts")
            .join(&tts.voices_filename))
    }

    /// Check which STT models are already downloaded.
    pub fn downloaded_stt_models() -> Result<Vec<SttModelInfo>, crate::error::VoiceError> {
        let models = Self::stt_models();
        let mut downloaded = Vec::new();
        for model in models {
            if let Ok(path) = Self::stt_model_path(&model) {
                if path.exists() {
                    downloaded.push(model);
                }
            }
        }
        Ok(downloaded)
    }

    /// Check whether the TTS model is downloaded.
    pub fn is_tts_downloaded() -> Result<bool, crate::error::VoiceError> {
        let model_path = Self::tts_model_path()?;
        let voices_path = Self::tts_voices_path()?;
        Ok(model_path.exists() && voices_path.exists())
    }

    /// Check if a specific STT model is downloaded.
    pub fn is_stt_downloaded(model_id: &str) -> Result<bool, crate::error::VoiceError> {
        let model = Self::find_stt_model(model_id);
        match model {
            Some(m) => {
                let path = Self::stt_model_path(&m)?;
                Ok(path.exists())
            }
            None => Ok(false),
        }
    }
}

// ── Download helpers ───────────────────────────────────────────────

/// Download a voice model file from a URL to a destination path.
///
/// Creates parent directories as needed. Reports progress via the callback.
pub async fn download_voice_model(
    url: &str,
    dest: &Path,
    on_progress: impl Fn(u64, u64), // (bytes_downloaded, total_bytes)
) -> Result<(), crate::error::VoiceError> {
    // Create parent directories
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    tracing::info!(url, dest = %dest.display(), "Downloading voice model");

    let client = reqwest::Client::new();
    let response =
        client
            .get(url)
            .send()
            .await
            .map_err(|e| crate::error::VoiceError::DownloadError {
                name: url.to_string(),
                source: e.into(),
            })?;

    if !response.status().is_success() {
        return Err(crate::error::VoiceError::DownloadError {
            name: url.to_string(),
            source: anyhow::anyhow!("HTTP {}", response.status()).into(),
        });
    }

    let total_size = response.content_length().unwrap_or(0);

    let bytes = response
        .bytes()
        .await
        .map_err(|e| crate::error::VoiceError::DownloadError {
            name: url.to_string(),
            source: e.into(),
        })?;

    tokio::fs::write(dest, &bytes).await?;
    on_progress(bytes.len() as u64, total_size.max(bytes.len() as u64));

    tracing::info!(
        size_mb = bytes.len() / 1_048_576,
        dest = %dest.display(),
        "Voice model download complete"
    );

    Ok(())
}

/// Download the default STT model if not already present.
pub async fn ensure_stt_model(
    model_id: &str,
    on_progress: impl Fn(u64, u64),
) -> Result<PathBuf, crate::error::VoiceError> {
    let model = VoiceModelCatalog::find_stt_model(model_id)
        .ok_or_else(|| crate::error::VoiceError::ModelNotFound(PathBuf::from(model_id)))?;

    let path = VoiceModelCatalog::stt_model_path(&model)?;

    if path.exists() {
        tracing::debug!(path = %path.display(), "STT model already downloaded");
        return Ok(path);
    }

    download_voice_model(&model.url, &path, on_progress).await?;
    Ok(path)
}

/// Download the TTS model files if not already present.
pub async fn ensure_tts_model(
    on_progress: impl Fn(u64, u64) + Clone,
) -> Result<(PathBuf, PathBuf), crate::error::VoiceError> {
    let tts = VoiceModelCatalog::tts_model();
    let model_path = VoiceModelCatalog::tts_model_path()?;
    let voices_path = VoiceModelCatalog::tts_voices_path()?;

    if !model_path.exists() {
        download_voice_model(&tts.model_url, &model_path, on_progress.clone()).await?;
    } else {
        tracing::debug!(path = %model_path.display(), "TTS model already downloaded");
    }

    if !voices_path.exists() {
        download_voice_model(&tts.voices_url, &voices_path, on_progress).await?;
    } else {
        tracing::debug!(path = %voices_path.display(), "TTS voices already downloaded");
    }

    Ok((model_path, voices_path))
}

// ── Internal helpers ───────────────────────────────────────────────

fn stt_model(
    id: &str,
    name: &str,
    filename: &str,
    size_bytes: u64,
    size_display: &str,
    english_only: bool,
    quality: u8,
    speed: u8,
    is_default: bool,
) -> SttModelInfo {
    SttModelInfo {
        id: VoiceModelId(id.to_string()),
        name: name.to_string(),
        filename: filename.to_string(),
        url: format!("{HF_WHISPER_BASE}/{filename}"),
        size_bytes,
        size_display: size_display.to_string(),
        english_only,
        quality,
        speed,
        is_default,
    }
}
