//! The one error every failing call of this library answers with.
//!
//! Each case is one an app words differently. The Hub's errors arrive typed
//! ([`HfPortError`]) and map case for case; the download manager's
//! ([`DownloadError`]) map by their variant and HTTP status; a full disk is
//! read off the I/O error's own words, which are all the download manager
//! keeps of it.

use gglib_core::download::DownloadError;
use gglib_core::ports::{HfPortError, RepositoryError};

/// Why a call failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum GglibError {
    /// Hugging Face wants a token for this repository or file: it answered
    /// 401 or 403. The library asks with the token it was opened with, or
    /// else with the `HF_TOKEN` its process started with, which an app on a
    /// phone has none of.
    #[error("Hugging Face needs a token for this")]
    AuthRequired,
    /// No such repository, quantization, file or installed model: the Hub
    /// answered 404, or the library holds no model by that id.
    #[error("not found")]
    NotFound,
    /// Hugging Face is limiting this device's requests: it answered 429.
    #[error("Hugging Face is limiting requests")]
    RateLimited,
    /// The device has no room for what was being written.
    #[error("no space left on the device")]
    NoSpace,
    /// Hugging Face could not be reached, or its answer could not be read.
    #[error("network: {message}")]
    Network {
        /// What went wrong, in gglib's words.
        message: String,
    },
    /// A model needs a budget of `need` bytes and the device gives `have`.
    #[error("needs a budget of {need} bytes and the device gives {have}")]
    DoesNotFit {
        /// The smallest budget the model fits in, in bytes.
        need: u64,
        /// The budget the device gives, in bytes.
        have: u64,
    },
    /// Nothing in this build can run a model on this device.
    #[error("no engine on this device")]
    EngineUnavailable,
    /// Anything else, in gglib's own words.
    #[error("{message}")]
    Internal {
        /// What went wrong.
        message: String,
    },
}

impl GglibError {
    /// An [`Self::Internal`] carrying `error`'s words.
    pub(crate) fn internal(error: impl std::fmt::Display) -> Self {
        Self::Internal {
            message: error.to_string(),
        }
    }

    /// The error a failed file operation is: [`Self::NoSpace`] when the
    /// device is full, else [`Self::Internal`].
    pub(crate) fn from_io(error: &std::io::Error) -> Self {
        if error.kind() == std::io::ErrorKind::StorageFull || names_no_space(&error.to_string()) {
            Self::NoSpace
        } else {
            Self::internal(error)
        }
    }
}

impl From<HfPortError> for GglibError {
    fn from(error: HfPortError) -> Self {
        match error {
            HfPortError::AuthRequired { .. } => Self::AuthRequired,
            HfPortError::ModelNotFound { .. }
            | HfPortError::QuantizationNotFound { .. }
            | HfPortError::FileNotFound { .. } => Self::NotFound,
            HfPortError::RateLimited => Self::RateLimited,
            HfPortError::Network { message } | HfPortError::InvalidResponse { message } => {
                Self::Network { message }
            }
            HfPortError::Configuration { message } => Self::Internal { message },
        }
    }
}

impl From<DownloadError> for GglibError {
    fn from(error: DownloadError) -> Self {
        match &error {
            DownloadError::Network {
                status_code: Some(401 | 403),
                ..
            } => Self::AuthRequired,
            DownloadError::Network {
                status_code: Some(404),
                ..
            }
            | DownloadError::NotFound { .. }
            | DownloadError::InvalidQuantization { .. } => Self::NotFound,
            DownloadError::Network {
                status_code: Some(429),
                ..
            } => Self::RateLimited,
            DownloadError::Network { message, .. }
            | DownloadError::ResolutionFailed { message } => Self::Network {
                message: message.clone(),
            },
            DownloadError::Io { message, .. } if names_no_space(message) => Self::NoSpace,
            _ => Self::internal(&error),
        }
    }
}

impl From<RepositoryError> for GglibError {
    fn from(error: RepositoryError) -> Self {
        match error {
            RepositoryError::NotFound(_) => Self::NotFound,
            other => Self::internal(other),
        }
    }
}

/// The OS's code for a full disk: `ENOSPC` on Apple platforms and Linux,
/// `ERROR_DISK_FULL` on Windows.
#[cfg(not(windows))]
const DISK_FULL: i32 = 28;
#[cfg(windows)]
const DISK_FULL: i32 = 112;

/// Whether `text` holds the OS's words for a full disk, as an I/O error
/// turned into text by `to_string` carries them.
pub(crate) fn names_no_space(text: &str) -> bool {
    text.contains(&std::io::Error::from_raw_os_error(DISK_FULL).to_string())
}

#[cfg(test)]
#[path = "errors_tests.rs"]
mod tests;
