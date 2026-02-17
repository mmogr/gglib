//! Voice model catalog — curated list of STT, TTS, and VAD models.
//!
//! Models are ONNX archives (`.tar.bz2`) from the
//! [`k2-fsa/sherpa-onnx`](https://github.com/k2-fsa/sherpa-onnx/releases)
//! releases.  Each archive extracts to a directory containing the model
//! files expected by `sherpa-rs`.

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

// ── STT model info ─────────────────────────────────────────────────

/// Information about a whisper STT model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SttModelInfo {
    /// Model identifier (e.g., "base.en").
    pub id: VoiceModelId,

    /// Human-readable name.
    pub name: String,

    /// URL of the `.tar.bz2` archive containing the ONNX model files.
    pub archive_url: String,

    /// Directory name inside the archive (also used as the on-disk folder name).
    pub dir_name: String,

    /// Approximate download size in bytes.
    pub size_bytes: u64,

    /// Approximate size as human-readable string.
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

// ── TTS model info ─────────────────────────────────────────────────

/// Information about a TTS model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsModelInfo {
    /// Model identifier.
    pub id: VoiceModelId,

    /// Human-readable name.
    pub name: String,

    /// URL of the `.tar.bz2` archive.
    pub archive_url: String,

    /// Directory name inside the archive.
    pub dir_name: String,

    /// Approximate total size in bytes.
    pub size_bytes: u64,

    /// Approximate size as human-readable string.
    pub size_display: String,

    /// Number of available voices.
    pub voice_count: u32,
}

// ── VAD model info ─────────────────────────────────────────────────

/// Information about a Silero VAD model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VadModelInfo {
    /// Model identifier.
    pub id: VoiceModelId,

    /// Human-readable name.
    pub name: String,

    /// Direct download URL (single `.onnx` file, no archive).
    pub url: String,

    /// Filename on disk.
    pub filename: String,

    /// Size in bytes.
    pub size_bytes: u64,

    /// Human-readable size.
    pub size_display: String,
}

// ── URL bases ──────────────────────────────────────────────────────

const SHERPA_ASR_BASE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models";

const SHERPA_TTS_BASE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models";

// ── Catalog ────────────────────────────────────────────────────────

/// The curated voice model catalog.
///
/// Provides fixed lists of known-good STT and TTS models with
/// deterministic download URLs.
pub struct VoiceModelCatalog;

impl VoiceModelCatalog {
    // ── STT models ─────────────────────────────────────────────

    /// Get all available STT (whisper) models.
    #[must_use]
    pub fn stt_models() -> Vec<SttModelInfo> {
        vec![
            stt_model(
                "tiny.en",
                "Tiny (English)",
                "sherpa-onnx-whisper-tiny.en",
                118_489_088, // ~113 MB
                "113 MB",
                true,
                1,
                1,
                false,
            ),
            stt_model(
                "base.en",
                "Base (English)",
                "sherpa-onnx-whisper-base.en",
                208_666_624, // ~199 MB
                "199 MB",
                true,
                3,
                2,
                true, // default
            ),
            stt_model(
                "small.en",
                "Small (English)",
                "sherpa-onnx-whisper-small.en",
                635_437_056, // ~606 MB
                "606 MB",
                true,
                4,
                3,
                false,
            ),
            stt_model(
                "tiny",
                "Tiny (Multilingual)",
                "sherpa-onnx-whisper-tiny",
                116_391_936, // ~111 MB
                "111 MB",
                false,
                1,
                1,
                false,
            ),
            stt_model(
                "base",
                "Base (Multilingual)",
                "sherpa-onnx-whisper-base",
                207_618_048, // ~198 MB
                "198 MB",
                false,
                3,
                2,
                false,
            ),
            stt_model(
                "small",
                "Small (Multilingual)",
                "sherpa-onnx-whisper-small",
                639_631_360, // ~610 MB
                "610 MB",
                false,
                4,
                3,
                false,
            ),
            stt_model(
                "turbo",
                "Large V3 Turbo (Multilingual)",
                "sherpa-onnx-whisper-turbo",
                564_133_888, // ~538 MB
                "538 MB",
                false,
                5,
                4,
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

    // ── TTS model ──────────────────────────────────────────────

    /// Get the TTS model info.
    #[must_use]
    pub fn tts_model() -> TtsModelInfo {
        TtsModelInfo {
            id: VoiceModelId("kokoro-en-v0_19".to_string()),
            name: "Kokoro v0.19 (English)".to_string(),
            archive_url: format!("{SHERPA_TTS_BASE}/kokoro-en-v0_19.tar.bz2"),
            dir_name: "kokoro-en-v0_19".to_string(),
            size_bytes: 319_815_680, // ~305 MB
            size_display: "305 MB".to_string(),
            voice_count: 11,
        }
    }

    // ── VAD model ──────────────────────────────────────────────

    /// Get the Silero VAD model info.
    #[must_use]
    pub fn vad_model() -> VadModelInfo {
        VadModelInfo {
            id: VoiceModelId("silero-vad".to_string()),
            name: "Silero VAD".to_string(),
            url: format!("{SHERPA_ASR_BASE}/silero_vad.onnx"),
            filename: "silero_vad.onnx".to_string(),
            size_bytes: 644_096, // ~629 KB
            size_display: "629 KB".to_string(),
        }
    }

    // ── Paths ──────────────────────────────────────────────────

    /// Get the directory where voice models are stored.
    ///
    /// Returns `{data_root}/voice_models/`.
    pub fn voice_models_dir() -> Result<PathBuf, crate::error::VoiceError> {
        let data_root = gglib_core::paths::data_root()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::NotFound, e.to_string()))?;
        Ok(data_root.join("voice_models"))
    }

    /// Get the path where an STT model directory is stored on disk.
    pub fn stt_model_path(model: &SttModelInfo) -> Result<PathBuf, crate::error::VoiceError> {
        Ok(Self::voice_models_dir()?.join("stt").join(&model.dir_name))
    }

    /// Get the path where the TTS model directory is stored on disk.
    pub fn tts_model_path() -> Result<PathBuf, crate::error::VoiceError> {
        let tts = Self::tts_model();
        Ok(Self::voice_models_dir()?.join("tts").join(&tts.dir_name))
    }

    /// Get the path where the Silero VAD model is stored.
    pub fn vad_model_path() -> Result<PathBuf, crate::error::VoiceError> {
        let vad = Self::vad_model();
        Ok(Self::voice_models_dir()?.join("vad").join(&vad.filename))
    }

    // ── Download status queries ────────────────────────────────

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
        let path = Self::tts_model_path()?;
        // The path is a directory; check it contains model.onnx.
        Ok(path.join("model.onnx").exists())
    }

    /// Check whether the Silero VAD model is downloaded.
    pub fn is_vad_downloaded() -> Result<bool, crate::error::VoiceError> {
        Ok(Self::vad_model_path()?.exists())
    }

    /// Check if a specific STT model is downloaded.
    pub fn is_stt_downloaded(model_id: &str) -> Result<bool, crate::error::VoiceError> {
        let model = Self::find_stt_model(model_id);
        match model {
            Some(m) => {
                let path = Self::stt_model_path(&m)?;
                // Check the directory contains an encoder ONNX file.
                // sherpa-onnx archives use prefixed names like `base.en-encoder.onnx`.
                Ok(path.exists() && has_file_ending(&path, "-encoder.onnx"))
            }
            None => Ok(false),
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────

/// Return `true` if `dir` contains at least one file whose name ends with `suffix`.
fn has_file_ending(dir: &Path, suffix: &str) -> bool {
    std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| e.file_name().to_str().is_some_and(|n| n.ends_with(suffix)))
}

// ── Download helpers ───────────────────────────────────────────────

/// Download a single file from a URL to a destination path.
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
            source: anyhow::anyhow!("HTTP {}", response.status()),
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

/// Download a `.tar.bz2` archive and extract it into `dest_dir`.
///
/// The archive is downloaded into memory, then extracted in a blocking
/// thread. Returns the path to the extracted directory.
pub async fn download_and_extract_archive(
    url: &str,
    dest_dir: &Path,
    dir_name: &str,
    on_progress: impl Fn(u64, u64),
) -> Result<PathBuf, crate::error::VoiceError> {
    let extract_path = dest_dir.join(dir_name);

    // Already extracted?
    if extract_path.exists() {
        tracing::debug!(path = %extract_path.display(), "Archive already extracted");
        return Ok(extract_path);
    }

    tokio::fs::create_dir_all(dest_dir).await?;

    tracing::info!(url, dest = %extract_path.display(), "Downloading archive");

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
            source: anyhow::anyhow!("HTTP {}", response.status()),
        });
    }

    let total_size = response.content_length().unwrap_or(0);
    let archive_bytes =
        response
            .bytes()
            .await
            .map_err(|e| crate::error::VoiceError::DownloadError {
                name: url.to_string(),
                source: e.into(),
            })?;

    on_progress(
        archive_bytes.len() as u64,
        total_size.max(archive_bytes.len() as u64),
    );

    tracing::info!(
        size_mb = archive_bytes.len() / 1_048_576,
        "Archive downloaded, extracting"
    );

    // Extract in a blocking thread to avoid blocking the async runtime.
    let dest_owned = dest_dir.to_path_buf();
    let bytes_vec = archive_bytes.to_vec();
    tokio::task::spawn_blocking(move || {
        let cursor = std::io::Cursor::new(bytes_vec);
        let decompressor = bzip2::read::BzDecoder::new(cursor);
        let mut archive = tar::Archive::new(decompressor);
        archive
            .unpack(&dest_owned)
            .map_err(|e| crate::error::VoiceError::DownloadError {
                name: "archive".to_string(),
                source: anyhow::anyhow!("Failed to extract archive: {e}"),
            })?;
        Ok::<(), crate::error::VoiceError>(())
    })
    .await
    .map_err(|e| crate::error::VoiceError::DownloadError {
        name: url.to_string(),
        source: anyhow::anyhow!("Join error: {e}"),
    })??;

    tracing::info!(path = %extract_path.display(), "Archive extracted successfully");
    Ok(extract_path)
}

// ── Ensure helpers ─────────────────────────────────────────────────

/// Download the specified STT model if not already present.
///
/// Returns the path to the extracted model directory.
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

    let stt_dir = VoiceModelCatalog::voice_models_dir()?.join("stt");
    download_and_extract_archive(&model.archive_url, &stt_dir, &model.dir_name, on_progress).await
}

/// Download the TTS model if not already present.
///
/// Returns the directory containing the extracted model files.
pub async fn ensure_tts_model(
    on_progress: impl Fn(u64, u64),
) -> Result<PathBuf, crate::error::VoiceError> {
    let tts = VoiceModelCatalog::tts_model();
    let tts_dir = VoiceModelCatalog::voice_models_dir()?.join("tts");
    download_and_extract_archive(&tts.archive_url, &tts_dir, &tts.dir_name, on_progress).await
}

/// Download the Silero VAD model if not already present.
pub async fn ensure_vad_model(
    on_progress: impl Fn(u64, u64),
) -> Result<PathBuf, crate::error::VoiceError> {
    let vad = VoiceModelCatalog::vad_model();
    let path = VoiceModelCatalog::vad_model_path()?;

    if path.exists() {
        tracing::debug!(path = %path.display(), "VAD model already downloaded");
        return Ok(path);
    }

    download_voice_model(&vad.url, &path, on_progress).await?;
    Ok(path)
}

// ── Internal constructors ──────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn stt_model(
    id: &str,
    name: &str,
    dir_name: &str,
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
        archive_url: format!("{SHERPA_ASR_BASE}/{dir_name}.tar.bz2"),
        dir_name: dir_name.to_string(),
        size_bytes,
        size_display: size_display.to_string(),
        english_only,
        quality,
        speed,
        is_default,
    }
}
