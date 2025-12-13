//! Internal error types for `HuggingFace` operations.
//!
//! These errors are internal to `gglib-hf` and are mapped to core port errors
//! at the boundary.

use thiserror::Error;

/// Result type alias for `HuggingFace` operations.
pub type HfResult<T> = Result<T, HfError>;

/// Errors related to `HuggingFace` API operations.
#[derive(Debug, Error)]
pub enum HfError {
    /// API request failed with an HTTP error status.
    #[error("HuggingFace API request failed with status {status}: {url}")]
    ApiRequestFailed {
        /// HTTP status code
        status: u16,
        /// The URL that was requested
        url: String,
    },

    /// API returned an invalid or unexpected response.
    #[error("Invalid response from HuggingFace API: {message}")]
    InvalidResponse {
        /// Description of what was invalid
        message: String,
    },

    /// The requested model was not found.
    #[error("Model '{model_id}' not found on HuggingFace")]
    ModelNotFound {
        /// The model ID that was not found
        model_id: String,
    },

    /// The requested quantization was not found for the model.
    #[error("Quantization '{quantization}' not found for model '{model_id}'")]
    QuantizationNotFound {
        /// The model ID
        model_id: String,
        /// The quantization that was not found
        quantization: String,
    },

    /// Network or HTTP client error.
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// URL parsing error.
    #[error("Invalid URL: {0}")]
    InvalidUrl(#[from] url::ParseError),

    /// JSON parsing error.
    #[error("JSON parsing error: {0}")]
    JsonParse(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_request_failed_error_message() {
        let error = HfError::ApiRequestFailed {
            status: 404,
            url: "https://huggingface.co/api/models/test".to_string(),
        };
        let msg = error.to_string();
        assert!(msg.contains("404"));
        assert!(msg.contains("huggingface.co"));
    }

    #[test]
    fn test_invalid_response_error_message() {
        let error = HfError::InvalidResponse {
            message: "Missing required field 'id'".to_string(),
        };
        assert!(error.to_string().contains("Missing required field"));
    }

    #[test]
    fn test_model_not_found_error_message() {
        let error = HfError::ModelNotFound {
            model_id: "TheBloke/NonExistent-GGUF".to_string(),
        };
        let msg = error.to_string();
        assert!(msg.contains("TheBloke/NonExistent-GGUF"));
        assert!(msg.contains("not found"));
    }

    #[test]
    fn test_quantization_not_found_error_message() {
        let error = HfError::QuantizationNotFound {
            model_id: "TheBloke/Llama-2-7B-GGUF".to_string(),
            quantization: "Q99_Z".to_string(),
        };
        let msg = error.to_string();
        assert!(msg.contains("Q99_Z"));
        assert!(msg.contains("TheBloke/Llama-2-7B-GGUF"));
    }

    #[test]
    fn test_hf_result_ok() {
        let result: HfResult<i32> = Ok(42);
        assert!(result.is_ok());
        assert!(matches!(result, Ok(42)));
    }

    #[test]
    fn test_hf_result_err() {
        let result: HfResult<i32> = Err(HfError::InvalidResponse {
            message: "test".to_string(),
        });
        assert!(result.is_err());
    }
}
