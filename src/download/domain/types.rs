//! Core domain types for the download module.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use strum_macros::{Display, EnumIter, EnumString};

/// Canonical identifier for a download.
///
/// Represents a unique download as `model_id:quantization` (or just `model_id` if no quantization).
/// This is the single identifier format used throughout the system.
///
/// # Examples
///
/// ```rust
/// use gglib::download::DownloadId;
///
/// let id = DownloadId::new("unsloth/Llama-3", Some("Q4_K_M"));
/// assert_eq!(id.to_string(), "unsloth/Llama-3:Q4_K_M");
///
/// let parsed: DownloadId = "owner/repo:Q8_0".parse().unwrap();
/// assert_eq!(parsed.model_id(), "owner/repo");
/// assert_eq!(parsed.quantization(), Some("Q8_0"));
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DownloadId {
    model_id: String,
    quantization: Option<String>,
}

impl DownloadId {
    /// Create a new download ID.
    pub fn new(model_id: impl Into<String>, quantization: Option<impl Into<String>>) -> Self {
        Self {
            model_id: model_id.into(),
            quantization: quantization.map(|q| q.into()),
        }
    }

    /// Create a download ID from model_id only (no quantization).
    pub fn from_model(model_id: impl Into<String>) -> Self {
        Self {
            model_id: model_id.into(),
            quantization: None,
        }
    }

    /// Get the model ID (e.g., "unsloth/Llama-3").
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Get the quantization type if specified (e.g., "Q4_K_M").
    pub fn quantization(&self) -> Option<&str> {
        self.quantization.as_deref()
    }

    /// Check if this ID has a quantization specified.
    pub fn has_quantization(&self) -> bool {
        self.quantization.is_some()
    }

    /// Convert to the canonical string format.
    pub fn as_canonical(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for DownloadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.quantization {
            Some(q) => write!(f, "{}:{}", self.model_id, q),
            None => write!(f, "{}", self.model_id),
        }
    }
}

impl FromStr for DownloadId {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Find the LAST colon that's not part of the model_id
        // Model IDs can contain colons in rare cases, but quantization never does
        if let Some(colon_pos) = s.rfind(':') {
            let (model, quant) = s.split_at(colon_pos);
            let quant = &quant[1..]; // Skip the colon

            // Only treat as quantization if it looks like one (contains letters/numbers, no slashes)
            if !quant.is_empty() && !quant.contains('/') {
                return Ok(Self {
                    model_id: model.to_string(),
                    quantization: Some(quant.to_string()),
                });
            }
        }

        Ok(Self {
            model_id: s.to_string(),
            quantization: None,
        })
    }
}

/// Request to start a download.
///
/// Contains all parameters needed to initiate a download from HuggingFace Hub.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DownloadRequest {
    /// The download identifier.
    pub id: DownloadId,
    /// Repository ID on HuggingFace (e.g., "unsloth/Llama-3.2-GGUF").
    pub repo_id: String,
    /// The resolved quantization type.
    pub quantization: Quantization,
    /// Specific files to download.
    pub files: Vec<String>,
    /// Destination directory for downloaded files.
    pub destination: std::path::PathBuf,
    /// Revision/commit SHA (defaults to "main").
    pub revision: Option<String>,
    /// Force re-download even if file exists locally.
    pub force: bool,
    /// Add to local model database after download.
    pub add_to_db: bool,
    /// HuggingFace authentication token (for private repos).
    pub token: Option<String>,
}

impl DownloadRequest {
    /// Create a builder for constructing a download request.
    pub fn builder() -> DownloadRequestBuilder {
        DownloadRequestBuilder::default()
    }

    /// Create a simple download request (for testing).
    pub fn new(id: DownloadId) -> Self {
        Self {
            repo_id: id.model_id().to_string(),
            quantization: Quantization::Unknown,
            files: Vec::new(),
            destination: std::path::PathBuf::new(),
            revision: None,
            id,
            force: false,
            add_to_db: true,
            token: None,
        }
    }
}

/// Builder for DownloadRequest.
#[derive(Default)]
pub struct DownloadRequestBuilder {
    id: Option<DownloadId>,
    repo_id: Option<String>,
    quantization: Option<Quantization>,
    files: Vec<String>,
    destination: Option<std::path::PathBuf>,
    revision: Option<String>,
    force: bool,
    add_to_db: bool,
    token: Option<String>,
}

impl DownloadRequestBuilder {
    /// Set the download ID.
    pub fn id(mut self, id: DownloadId) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the repository ID.
    pub fn repo_id(mut self, repo_id: impl Into<String>) -> Self {
        self.repo_id = Some(repo_id.into());
        self
    }

    /// Set the quantization type.
    pub fn quantization(mut self, quantization: Quantization) -> Self {
        self.quantization = Some(quantization);
        self
    }

    /// Set the files to download.
    pub fn files(mut self, files: Vec<String>) -> Self {
        self.files = files;
        self
    }

    /// Set the destination directory.
    pub fn destination(mut self, destination: impl Into<std::path::PathBuf>) -> Self {
        self.destination = Some(destination.into());
        self
    }

    /// Set the revision/commit.
    pub fn revision(mut self, revision: impl Into<String>) -> Self {
        self.revision = Some(revision.into());
        self
    }

    /// Set whether to force re-download.
    pub fn force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }

    /// Set whether to add to database.
    pub fn add_to_db(mut self, add_to_db: bool) -> Self {
        self.add_to_db = add_to_db;
        self
    }

    /// Set the authentication token.
    pub fn token(mut self, token: Option<String>) -> Self {
        self.token = token;
        self
    }

    /// Build the request.
    pub fn build(self) -> DownloadRequest {
        let id = self.id.expect("id is required");
        DownloadRequest {
            repo_id: self.repo_id.unwrap_or_else(|| id.model_id().to_string()),
            quantization: self.quantization.unwrap_or(Quantization::Unknown),
            files: self.files,
            destination: self.destination.unwrap_or_default(),
            revision: self.revision,
            id,
            force: self.force,
            add_to_db: self.add_to_db,
            token: self.token,
        }
    }
}

/// Represents the quantization type of a GGUF model file.
///
/// This enum provides type-safe handling of quantization types commonly used
/// in GGUF model naming conventions. Use [`Quantization::from_filename`] to
/// parse a filename into this enum.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumString, Display, EnumIter,
)]
pub enum Quantization {
    // 1-bit quantizations
    #[strum(serialize = "IQ1_S")]
    Iq1S,
    #[strum(serialize = "IQ1_M")]
    Iq1M,

    // 2-bit quantizations
    #[strum(serialize = "IQ2_XXS")]
    Iq2Xxs,
    #[strum(serialize = "IQ2_XS")]
    Iq2Xs,
    #[strum(serialize = "IQ2_S")]
    Iq2S,
    #[strum(serialize = "IQ2_M")]
    Iq2M,
    #[strum(serialize = "Q2_K_XL")]
    Q2KXl,
    #[strum(serialize = "Q2_K_L")]
    Q2KL,
    #[strum(serialize = "Q2_K")]
    Q2K,

    // 3-bit quantizations
    #[strum(serialize = "IQ3_XXS")]
    Iq3Xxs,
    #[strum(serialize = "IQ3_XS")]
    Iq3Xs,
    #[strum(serialize = "IQ3_M")]
    Iq3M,
    #[strum(serialize = "Q3_K_XL")]
    Q3KXl,
    #[strum(serialize = "Q3_K_L")]
    Q3KL,
    #[strum(serialize = "Q3_K_M")]
    Q3KM,
    #[strum(serialize = "Q3_K_S")]
    Q3KS,

    // 4-bit quantizations
    #[strum(serialize = "IQ4_XS")]
    Iq4Xs,
    #[strum(serialize = "IQ4_NL")]
    Iq4Nl,
    #[strum(serialize = "Q4_K_XL")]
    Q4KXl,
    #[strum(serialize = "Q4_K_L")]
    Q4KL,
    #[strum(serialize = "Q4_K_M")]
    Q4KM,
    #[strum(serialize = "Q4_K_S")]
    Q4KS,
    #[strum(serialize = "Q4_1")]
    Q4_1,
    #[strum(serialize = "Q4_0")]
    Q4_0,
    #[strum(serialize = "MXFP4")]
    Mxfp4,
    #[strum(serialize = "Q4")]
    Q4,

    // 5-bit quantizations
    #[strum(serialize = "Q5_K_XL")]
    Q5KXl,
    #[strum(serialize = "Q5_K_L")]
    Q5KL,
    #[strum(serialize = "Q5_K_M")]
    Q5KM,
    #[strum(serialize = "Q5_K_S")]
    Q5KS,
    #[strum(serialize = "Q5_0")]
    Q5_0,
    #[strum(serialize = "Q5_1")]
    Q5_1,
    #[strum(serialize = "Q5")]
    Q5,

    // 6-bit quantizations
    #[strum(serialize = "Q6_K_XL")]
    Q6KXl,
    #[strum(serialize = "Q6_K_L")]
    Q6KL,
    #[strum(serialize = "Q6_K")]
    Q6K,
    #[strum(serialize = "Q6")]
    Q6,

    // 8-bit quantizations
    #[strum(serialize = "Q8_K_XL")]
    Q8KXl,
    #[strum(serialize = "Q8_0")]
    Q8_0,
    #[strum(serialize = "Q8")]
    Q8,

    // 16-bit and higher precision
    #[strum(serialize = "BF16")]
    Bf16,
    #[strum(serialize = "F16")]
    F16,
    #[strum(serialize = "F32")]
    F32,

    // Special formats
    #[strum(serialize = "imatrix")]
    Imatrix,

    #[strum(serialize = "unknown")]
    Unknown,
}

/// Pattern table for quantization extraction, ordered by specificity.
/// More specific patterns (longer, more detailed) come before generic ones.
const QUANT_PATTERNS: &[(&str, Quantization)] = &[
    // 1-bit quantizations
    ("IQ1_S", Quantization::Iq1S),
    ("IQ1_M", Quantization::Iq1M),
    // 2-bit quantizations (most specific first)
    ("IQ2_XXS", Quantization::Iq2Xxs),
    ("IQ2_XS", Quantization::Iq2Xs),
    ("IQ2_S", Quantization::Iq2S),
    ("IQ2_M", Quantization::Iq2M),
    ("Q2_K_XL", Quantization::Q2KXl),
    ("Q2_K_L", Quantization::Q2KL),
    ("Q2_K", Quantization::Q2K),
    // 3-bit quantizations (most specific first)
    ("IQ3_XXS", Quantization::Iq3Xxs),
    ("IQ3_XS", Quantization::Iq3Xs),
    ("IQ3_M", Quantization::Iq3M),
    ("Q3_K_XL", Quantization::Q3KXl),
    ("Q3_K_L", Quantization::Q3KL),
    ("Q3_K_M", Quantization::Q3KM),
    ("Q3_K_S", Quantization::Q3KS),
    // 4-bit quantizations (most specific first)
    ("IQ4_XS", Quantization::Iq4Xs),
    ("IQ4_NL", Quantization::Iq4Nl),
    ("Q4_K_XL", Quantization::Q4KXl),
    ("Q4_K_L", Quantization::Q4KL),
    ("Q4_K_M", Quantization::Q4KM),
    ("Q4_K_S", Quantization::Q4KS),
    ("Q4_1", Quantization::Q4_1),
    ("Q4_0", Quantization::Q4_0),
    ("MXFP4", Quantization::Mxfp4),
    ("Q4", Quantization::Q4),
    // 5-bit quantizations (most specific first)
    ("Q5_K_XL", Quantization::Q5KXl),
    ("Q5_K_L", Quantization::Q5KL),
    ("Q5_K_M", Quantization::Q5KM),
    ("Q5_K_S", Quantization::Q5KS),
    ("Q5_0", Quantization::Q5_0),
    ("Q5_1", Quantization::Q5_1),
    ("Q5", Quantization::Q5),
    // 6-bit quantizations (most specific first)
    ("Q6_K_XL", Quantization::Q6KXl),
    ("Q6_K_L", Quantization::Q6KL),
    ("Q6_K", Quantization::Q6K),
    ("Q6", Quantization::Q6),
    // 8-bit quantizations (most specific first)
    ("Q8_K_XL", Quantization::Q8KXl),
    ("Q8_0", Quantization::Q8_0),
    ("Q8", Quantization::Q8),
    // 16-bit and higher precision
    ("BF16", Quantization::Bf16),
    ("FP16", Quantization::F16),
    ("F16", Quantization::F16),
    ("FP32", Quantization::F32),
    ("F32", Quantization::F32),
    // Special formats
    ("IMATRIX", Quantization::Imatrix),
];

impl Quantization {
    /// Returns true if this quantization type is unknown.
    pub fn is_unknown(&self) -> bool {
        matches!(self, Quantization::Unknown)
    }

    /// Extract quantization type from a filename.
    ///
    /// Analyzes a filename to determine the quantization type based on common
    /// patterns used in GGUF model naming conventions.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use gglib::download::Quantization;
    ///
    /// assert_eq!(Quantization::from_filename("model-Q4_K_M.gguf"), Quantization::Q4KM);
    /// assert_eq!(Quantization::from_filename("llama-F16.gguf"), Quantization::F16);
    /// assert_eq!(Quantization::from_filename("unknown.gguf"), Quantization::Unknown);
    /// ```
    pub fn from_filename(filename: &str) -> Self {
        let upper = filename.to_uppercase();
        QUANT_PATTERNS
            .iter()
            .find(|(pattern, _)| upper.contains(pattern))
            .map(|(_, q)| *q)
            .unwrap_or(Quantization::Unknown)
    }
}

/// Information about a shard within a sharded model download.
///
/// Used to track individual parts of a multi-file GGUF model.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardInfo {
    /// 0-based index of this shard (e.g., 0 for "Part 1/3").
    pub shard_index: u32,
    /// Total number of shards in this model.
    pub total_shards: u32,
    /// The specific filename for this shard (e.g., "model-00001-of-00003.gguf").
    pub filename: String,
    /// Size of this shard file in bytes (if known).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size: Option<u64>,
}

impl ShardInfo {
    /// Create a new ShardInfo instance.
    pub fn new(shard_index: u32, total_shards: u32, filename: impl Into<String>) -> Self {
        Self {
            shard_index,
            total_shards,
            filename: filename.into(),
            file_size: None,
        }
    }

    /// Create a new ShardInfo instance with file size.
    pub fn with_size(
        shard_index: u32,
        total_shards: u32,
        filename: impl Into<String>,
        file_size: u64,
    ) -> Self {
        Self {
            shard_index,
            total_shards,
            filename: filename.into(),
            file_size: Some(file_size),
        }
    }

    /// Format as display string (e.g., "Part 1/3").
    pub fn display(&self) -> String {
        format!("Part {}/{}", self.shard_index + 1, self.total_shards)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_id_display() {
        let id = DownloadId::new("unsloth/Llama-3", Some("Q4_K_M"));
        assert_eq!(id.to_string(), "unsloth/Llama-3:Q4_K_M");

        let id_no_quant = DownloadId::from_model("owner/repo");
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

    #[test]
    fn test_download_id_equality() {
        let id1 = DownloadId::new("model", Some("Q4_K_M"));
        let id2 = DownloadId::new("model", Some("Q4_K_M"));
        let id3 = DownloadId::new("model", Some("Q8_0"));

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_quantization_from_filename() {
        assert_eq!(
            Quantization::from_filename("model-Q4_K_M.gguf"),
            Quantization::Q4KM
        );
        assert_eq!(
            Quantization::from_filename("llama-F16.gguf"),
            Quantization::F16
        );
        assert_eq!(
            Quantization::from_filename("model-IQ2_XXS.gguf"),
            Quantization::Iq2Xxs
        );
        assert_eq!(
            Quantization::from_filename("unknown.gguf"),
            Quantization::Unknown
        );
    }

    #[test]
    fn test_shard_info_display() {
        let shard = ShardInfo::new(1, 5, "model-00002-of-00005.gguf");
        assert_eq!(shard.display(), "Part 2/5");
    }

    #[test]
    fn test_download_request_builder() {
        let id = DownloadId::new("model", Some("Q4_K_M"));
        let request = DownloadRequest::builder()
            .id(id.clone())
            .repo_id("model")
            .quantization(Quantization::Q4KM)
            .files(vec!["model.gguf".to_string()])
            .destination("/tmp/models")
            .force(true)
            .add_to_db(false)
            .token(Some("secret".to_string()))
            .build();

        assert_eq!(request.id, id);
        assert!(request.force);
        assert!(!request.add_to_db);
        assert_eq!(request.token, Some("secret".to_string()));
    }
}
