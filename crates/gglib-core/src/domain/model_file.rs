//! The files a model is made of, as the library records them.
//!
//! One row per file: a single-file model has one, a sharded model one per
//! shard, and a model fetched with a projector has the projector's too. Each
//! carries the OID and size verification and the update check compare against.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents a single file (shard) belonging to a model.
///
/// This tracks per-file metadata including OIDs for verification and update detection.
/// Models can have multiple files (sharded models) or a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelFile {
    /// Database ID of this model file entry.
    pub id: i64,
    /// ID of the parent model.
    pub model_id: i64,
    /// Relative path to the file within the model directory.
    pub file_path: String,
    /// Index of this file in the shard sequence (0 for single-file models).
    pub file_index: i32,
    /// Expected file size in bytes (from `HuggingFace` API).
    pub expected_size: i64,
    /// `HuggingFace` OID (Git LFS SHA256 hash) for this file.
    pub hf_oid: Option<String>,
    /// UTC timestamp of when this file was last verified.
    pub last_verified_at: Option<DateTime<Utc>>,
}

/// A model file entry to be inserted into the system (no ID yet).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewModelFile {
    /// ID of the parent model.
    pub model_id: i64,
    /// Relative path to the file within the model directory.
    pub file_path: String,
    /// Index of this file in the shard sequence (0 for single-file models).
    pub file_index: i32,
    /// Expected file size in bytes (from `HuggingFace` API).
    pub expected_size: i64,
    /// `HuggingFace` OID (Git LFS SHA256 hash) for this file.
    pub hf_oid: Option<String>,
}

impl NewModelFile {
    /// Create a new model file entry with minimal required fields.
    #[must_use]
    pub const fn new(
        model_id: i64,
        file_path: String,
        file_index: i32,
        expected_size: i64,
        hf_oid: Option<String>,
    ) -> Self {
        Self {
            model_id,
            file_path,
            file_index,
            expected_size,
            hf_oid,
        }
    }
}
