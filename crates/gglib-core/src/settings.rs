//! Settings domain types and validation.
//!
//! This module contains the core settings types used across the application.
//! These are pure domain types with no infrastructure dependencies.

use serde::{Deserialize, Serialize};

/// Application settings structure.
///
/// All fields are optional to support partial updates and graceful defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    /// Default directory for downloading models.
    pub default_download_path: Option<String>,

    /// Default context size for models (e.g., 4096, 8192).
    pub default_context_size: Option<u64>,

    /// Port for the OpenAI-compatible proxy server.
    pub proxy_port: Option<u16>,

    /// Base port for llama-server instances.
    pub server_port: Option<u16>,

    /// Maximum number of downloads that can be queued (1-50).
    pub max_download_queue_size: Option<u32>,

    /// Whether to show memory fit indicators in `HuggingFace` browser.
    pub show_memory_fit_indicators: Option<bool>,
}

impl Settings {
    /// Create settings with sensible defaults.
    #[must_use]
    pub const fn with_defaults() -> Self {
        Self {
            default_download_path: None,
            default_context_size: Some(4096),
            proxy_port: Some(8080),
            server_port: Some(9000),
            max_download_queue_size: Some(10),
            show_memory_fit_indicators: Some(true),
        }
    }

    /// Merge another settings into this one, only updating fields that are Some.
    pub fn merge(&mut self, other: &SettingsUpdate) {
        if let Some(ref path) = other.default_download_path {
            self.default_download_path.clone_from(path);
        }
        if let Some(ref ctx_size) = other.default_context_size {
            self.default_context_size = *ctx_size;
        }
        if let Some(ref port) = other.proxy_port {
            self.proxy_port = *port;
        }
        if let Some(ref port) = other.server_port {
            self.server_port = *port;
        }
        if let Some(ref queue_size) = other.max_download_queue_size {
            self.max_download_queue_size = *queue_size;
        }
        if let Some(ref show_fit) = other.show_memory_fit_indicators {
            self.show_memory_fit_indicators = *show_fit;
        }
    }
}

/// Partial settings update.
///
/// Each field is `Option<Option<T>>`:
/// - `None` = don't change this field
/// - `Some(None)` = set field to None/null
/// - `Some(Some(value))` = set field to value
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SettingsUpdate {
    pub default_download_path: Option<Option<String>>,
    pub default_context_size: Option<Option<u64>>,
    pub proxy_port: Option<Option<u16>>,
    pub server_port: Option<Option<u16>>,
    pub max_download_queue_size: Option<Option<u32>>,
    pub show_memory_fit_indicators: Option<Option<bool>>,
}

/// Settings validation error.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SettingsError {
    #[error("Context size must be between 512 and 1,000,000, got {0}")]
    InvalidContextSize(u64),

    #[error("Port should be >= 1024 (privileged ports require root), got {0}")]
    InvalidPort(u16),

    #[error("Max download queue size must be between 1 and 50, got {0}")]
    InvalidQueueSize(u32),

    #[error("Download path cannot be empty")]
    EmptyDownloadPath,
}

/// Validate settings values.
pub fn validate_settings(settings: &Settings) -> Result<(), SettingsError> {
    // Validate context size
    if let Some(ctx_size) = settings.default_context_size {
        if !(512..=1_000_000).contains(&ctx_size) {
            return Err(SettingsError::InvalidContextSize(ctx_size));
        }
    }

    // Validate proxy port
    if let Some(port) = settings.proxy_port {
        if port < 1024 {
            return Err(SettingsError::InvalidPort(port));
        }
    }

    // Validate server port
    if let Some(port) = settings.server_port {
        if port < 1024 {
            return Err(SettingsError::InvalidPort(port));
        }
    }

    // Validate max download queue size
    if let Some(queue_size) = settings.max_download_queue_size {
        if !(1..=50).contains(&queue_size) {
            return Err(SettingsError::InvalidQueueSize(queue_size));
        }
    }

    // Validate download path if specified
    if settings
        .default_download_path
        .as_ref()
        .is_some_and(|p| p.trim().is_empty())
    {
        return Err(SettingsError::EmptyDownloadPath);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = Settings::with_defaults();
        assert_eq!(settings.default_context_size, Some(4096));
        assert_eq!(settings.proxy_port, Some(8080));
        assert_eq!(settings.server_port, Some(9000));
        assert_eq!(settings.default_download_path, None);
        assert_eq!(settings.max_download_queue_size, Some(10));
        assert_eq!(settings.show_memory_fit_indicators, Some(true));
    }

    #[test]
    fn test_validate_settings_valid() {
        let settings = Settings::with_defaults();
        assert!(validate_settings(&settings).is_ok());
    }

    #[test]
    fn test_validate_context_size_too_small() {
        let settings = Settings {
            default_context_size: Some(100),
            ..Default::default()
        };
        assert!(matches!(
            validate_settings(&settings),
            Err(SettingsError::InvalidContextSize(100))
        ));
    }

    #[test]
    fn test_validate_context_size_too_large() {
        let settings = Settings {
            default_context_size: Some(2_000_000),
            ..Default::default()
        };
        assert!(matches!(
            validate_settings(&settings),
            Err(SettingsError::InvalidContextSize(2_000_000))
        ));
    }

    #[test]
    fn test_validate_port_too_low() {
        let settings = Settings {
            proxy_port: Some(80),
            ..Default::default()
        };
        assert!(matches!(
            validate_settings(&settings),
            Err(SettingsError::InvalidPort(80))
        ));
    }

    #[test]
    fn test_validate_empty_path() {
        let settings = Settings {
            default_download_path: Some(String::new()),
            ..Default::default()
        };
        assert!(matches!(
            validate_settings(&settings),
            Err(SettingsError::EmptyDownloadPath)
        ));
    }

    #[test]
    fn test_validate_queue_size_too_small() {
        let settings = Settings {
            max_download_queue_size: Some(0),
            ..Default::default()
        };
        assert!(matches!(
            validate_settings(&settings),
            Err(SettingsError::InvalidQueueSize(0))
        ));
    }

    #[test]
    fn test_validate_queue_size_too_large() {
        let settings = Settings {
            max_download_queue_size: Some(100),
            ..Default::default()
        };
        assert!(matches!(
            validate_settings(&settings),
            Err(SettingsError::InvalidQueueSize(100))
        ));
    }

    #[test]
    fn test_merge_settings() {
        let mut settings = Settings::with_defaults();
        let update = SettingsUpdate {
            default_context_size: Some(Some(8192)),
            proxy_port: Some(None), // Clear proxy port
            ..Default::default()
        };
        settings.merge(&update);

        assert_eq!(settings.default_context_size, Some(8192));
        assert_eq!(settings.proxy_port, None);
        assert_eq!(settings.server_port, Some(9000)); // Unchanged
    }
}
