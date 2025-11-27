//! Shared GUI model types used by both Tauri and Web interfaces.
//!
//! This module provides a unified representation of models optimized for
//! frontend consumption, eliminating duplication between the Tauri desktop
//! app and the web GUI.

use crate::models::Gguf;
use serde::{Deserialize, Serialize};

/// Frontend-friendly model structure shared by both Tauri and Web GUIs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiModel {
    pub id: Option<u32>,
    pub name: String,
    pub file_path: String,
    pub param_count_b: f64,
    pub architecture: Option<String>,
    pub quantization: Option<String>,
    pub context_length: Option<u64>,
    pub added_at: String,
    pub hf_repo_id: Option<String>,
    /// User-defined tags
    #[serde(default)]
    pub tags: Vec<String>,
    /// Whether this model is currently being served
    #[serde(default)]
    pub is_serving: bool,
    /// Port number if being served
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

impl GuiModel {
    /// Convert a Gguf model to GuiModel format
    pub fn from_model(model: Gguf, is_serving: bool, port: Option<u16>) -> Self {
        Self {
            id: model.id,
            name: model.name,
            file_path: model.file_path.to_string_lossy().to_string(),
            param_count_b: model.param_count_b,
            architecture: model.architecture,
            quantization: model.quantization,
            context_length: model.context_length,
            added_at: model.added_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            hf_repo_id: model.hf_repo_id,
            tags: model.tags,
            is_serving,
            port,
        }
    }

    /// Convert from Gguf with default serving status (not serving)
    pub fn from_gguf(model: Gguf) -> Self {
        Self::from_model(model, false, None)
    }
}

impl From<Gguf> for GuiModel {
    fn from(model: Gguf) -> Self {
        Self::from_gguf(model)
    }
}

/// Request body for starting a server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartServerRequest {
    /// Optional context length override
    pub context_length: Option<u64>,
    /// Optional port specification
    pub port: Option<u16>,
    /// Enable memory lock
    #[serde(default)]
    pub mlock: bool,
    /// Optional override for forcing Jinja templates
    #[serde(default)]
    pub jinja: Option<bool>,
}

/// Response for starting a server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartServerResponse {
    pub port: u16,
    pub message: String,
}

/// Request body for adding a model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddModelRequest {
    pub file_path: String,
}

/// Request body for removing a model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveModelRequest {
    #[serde(default)]
    pub force: bool,
}

/// Request body for updating a model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateModelRequest {
    pub name: Option<String>,
    pub quantization: Option<String>,
    pub file_path: Option<String>,
}

/// Request body for downloading a model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadModelRequest {
    pub model_id: String,
    pub quantization: Option<String>,
}

/// Request body for cancelling an in-flight download
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelDownloadRequest {
    pub model_id: String,
}

/// Current configuration for the models directory shown in settings UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsDirectoryInfo {
    pub path: String,
    pub source: String,
    pub default_path: String,
    pub exists: bool,
    pub writable: bool,
}

/// Payload for updating the models directory via the settings UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateModelsDirectoryRequest {
    pub path: String,
}

/// Application settings for the settings UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub default_download_path: Option<String>,
    pub default_context_size: Option<u64>,
    pub proxy_port: Option<u16>,
    pub server_port: Option<u16>,
    pub max_download_queue_size: Option<u32>,
}

/// Request body for updating application settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSettingsRequest {
    pub default_download_path: Option<Option<String>>,
    pub default_context_size: Option<Option<u64>>,
    pub proxy_port: Option<Option<u16>>,
    pub server_port: Option<Option<u16>>,
    pub max_download_queue_size: Option<Option<u32>>,
}

/// Standard API response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::path::PathBuf;

    #[test]
    fn test_gui_model_from_gguf() {
        let gguf_model = Gguf {
            id: Some(1),
            name: "Test Model".to_string(),
            file_path: PathBuf::from("/test/model.gguf"),
            param_count_b: 7.0,
            architecture: Some("llama".to_string()),
            quantization: Some("Q4_0".to_string()),
            context_length: Some(4096),
            metadata: HashMap::new(),
            added_at: Utc::now(),
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags: Vec::new(),
        };

        let gui_model = GuiModel::from_gguf(gguf_model.clone());
        assert_eq!(gui_model.id, Some(1));
        assert_eq!(gui_model.name, "Test Model");
        assert_eq!(gui_model.param_count_b, 7.0);
        assert!(!gui_model.is_serving);
        assert_eq!(gui_model.port, None);

        let gui_model_serving = GuiModel::from_model(gguf_model, true, Some(8080));
        assert!(gui_model_serving.is_serving);
        assert_eq!(gui_model_serving.port, Some(8080));
    }

    #[test]
    fn test_api_response() {
        let success: ApiResponse<String> = ApiResponse::success("OK".to_string());
        assert!(success.success);
        assert_eq!(success.data, Some("OK".to_string()));
        assert_eq!(success.error, None);

        let error: ApiResponse<String> = ApiResponse::error("Failed");
        assert!(!error.success);
        assert_eq!(error.data, None);
        assert_eq!(error.error, Some("Failed".to_string()));
    }
}
