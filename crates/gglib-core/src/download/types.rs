//! Core domain types for downloads.
//!
//! Pure data types with no I/O dependencies.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Canonical identifier for a download.
///
/// Represents a unique download as `model_id:label` (or just `model_id` when it
/// names no variant), the label being the variant's as the repository's listing
/// names it (`Q4_K_M`, `UD-Q4_K_XL`). This is the single identifier format used
/// throughout the system.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct DownloadId {
    model_id: String,
    /// The variant's label. No `skip_serializing_if`, so the key is always on
    /// the wire — `null` when the download names no variant.
    quantization: Option<String>,
}

impl DownloadId {
    /// Create a new download ID.
    pub fn new(model_id: impl Into<String>, quantization: Option<impl Into<String>>) -> Self {
        Self {
            model_id: model_id.into(),
            quantization: quantization.map(Into::into),
        }
    }

    /// Get the model ID (e.g., "unsloth/Llama-3").
    #[must_use]
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// The variant's label, when the download names one (e.g. "`UD-Q4_K_XL`").
    #[must_use]
    pub fn quantization(&self) -> Option<&str> {
        self.quantization.as_deref()
    }
}

impl fmt::Display for DownloadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.quantization {
            Some(q) => write!(f, "{}:{q}", self.model_id),
            None => write!(f, "{}", self.model_id),
        }
    }
}

/// The ID a canonical string names: `model_id:label`, or a model ID
/// alone. Every string is one, so this cannot fail.
impl From<&str> for DownloadId {
    fn from(s: &str) -> Self {
        // Find the LAST colon that's not part of the model_id
        if let Some(colon_pos) = s.rfind(':') {
            let (model, label) = s.split_at(colon_pos);
            let label = &label[1..]; // Skip the colon

            // A label holds no slash, so a colon followed by one is not a label's
            if !label.is_empty() && !label.contains('/') {
                return Self {
                    model_id: model.to_string(),
                    quantization: Some(label.to_string()),
                };
            }
        }

        Self {
            model_id: s.to_string(),
            quantization: None,
        }
    }
}

impl FromStr for DownloadId {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_id_display() {
        let id = DownloadId::new("unsloth/Llama-3", Some("Q4_K_M"));
        assert_eq!(id.to_string(), "unsloth/Llama-3:Q4_K_M");

        let id_no_quant = DownloadId::from("owner/repo");
        assert_eq!(id_no_quant.to_string(), "owner/repo");
    }

    #[test]
    fn test_download_id_parse() {
        let id: DownloadId = "unsloth/Llama-3:Q4_K_M".parse().unwrap();
        assert_eq!(id.model_id(), "unsloth/Llama-3");
        assert_eq!(id.quantization(), Some("Q4_K_M"));

        let id_no_quant: DownloadId = "owner/repo".parse().unwrap();
        assert_eq!(id_no_quant.model_id(), "owner/repo");
        assert_eq!(id_no_quant.quantization(), None);
    }
}
