//! Error types for `HuggingFace` port operations.

use std::time::Duration;

use thiserror::Error;

use crate::hub_limit::{HubBucket, clock};

/// Errors from `HuggingFace` port operations.
///
/// These are domain-level errors that consumers can handle.
/// Implementation-specific errors (HTTP, JSON) are mapped to these.
#[derive(Debug, Error)]
pub enum HfPortError {
    /// The requested model was not found.
    #[error("Model not found: {model_id}")]
    ModelNotFound {
        /// The model ID that wasn't found
        model_id: String,
    },

    /// The requested quantization wasn't found for the model.
    #[error("Quantization '{quantization}' not found for model '{model_id}'")]
    QuantizationNotFound {
        /// The model ID
        model_id: String,
        /// The quantization that wasn't found
        quantization: String,
    },

    /// The repository holds no file at the path asked for.
    #[error("No file {path} in {model_id}")]
    FileNotFound {
        /// The repository asked
        model_id: String,
        /// The path asked for
        path: String,
    },

    /// Hugging Face's rate limit refused the request, or would have: the
    /// allowance it counts against was spent when it was made.
    #[error("Hugging Face's rate limit lifts in {}", clock(*.resets_in))]
    RateLimited {
        /// The allowance that is spent.
        bucket: HubBucket,
        /// The time until it opens again.
        resets_in: Duration,
    },

    /// Authentication required or failed.
    #[error("Authentication required for private model: {model_id}")]
    AuthRequired {
        /// The model ID that requires auth
        model_id: String,
    },

    /// Network or connectivity error.
    #[error("Network error: {message}")]
    Network {
        /// Description of the network error
        message: String,
    },

    /// Invalid response from the API.
    #[error("Invalid API response: {message}")]
    InvalidResponse {
        /// What was invalid
        message: String,
    },

    /// Configuration error.
    #[error("Configuration error: {message}")]
    Configuration {
        /// What's wrong with the configuration
        message: String,
    },
}

/// Result type alias for `HuggingFace` port operations.
pub type HfPortResult<T> = Result<T, HfPortError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = HfPortError::ModelNotFound {
            model_id: "TheBloke/Test-GGUF".to_string(),
        };
        assert!(err.to_string().contains("TheBloke/Test-GGUF"));

        let err = HfPortError::QuantizationNotFound {
            model_id: "Org/Model".to_string(),
            quantization: "Q4_K_M".to_string(),
        };
        assert!(err.to_string().contains("Q4_K_M"));
        assert!(err.to_string().contains("Org/Model"));
    }

    /// A refusal says when the limit lifts, in the words a wait is shown in.
    #[test]
    fn a_rate_limit_says_when_it_lifts() {
        let err = HfPortError::RateLimited {
            bucket: HubBucket::Api,
            resets_in: Duration::from_millis(133_200),
        };
        assert_eq!(err.to_string(), "Hugging Face's rate limit lifts in 2:14");
    }
}
