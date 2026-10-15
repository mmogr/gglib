//! The files a model is made of, as the library records them.
//!
//! One row per file: a single-file model has one, a sharded model one per
//! shard, and a model fetched with a projector has the projector's too. Each
//! carries the OID and size verification and the update check compare against.
//!
//! A row names its file by its path in the repository, and [`ModelFolder`]
//! says where that file is on disk.

use std::path::{Path, PathBuf};

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

/// The folder a model's download wrote its files in, which every one of its
/// rows is read in.
///
/// A download writes each file at its path in the repository under one
/// folder, and the model's file is its first weights file, the first row. So
/// the folder is that file with the first row's path taken off its end:
/// `…/owner_repo` for `…/owner_repo/BF16/X-00001-of-00002.gguf` and the row
/// `BF16/X-00001-of-00002.gguf`, where a projector at the repository's root
/// is. A file that does not end with that row, or a model with no rows, is
/// read in the directory the file is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFolder(PathBuf);

impl ModelFolder {
    /// The folder of the model whose file is `weights` and whose first row
    /// names `first_row`.
    #[must_use]
    pub fn new(weights: &Path, first_row: Option<&str>) -> Self {
        let in_repository = first_row
            .map(Path::new)
            .filter(|row| row.is_relative() && weights.ends_with(row));
        let folder = in_repository
            .and_then(|row| weights.ancestors().nth(row.components().count()))
            .or_else(|| weights.parent())
            .unwrap_or_else(|| Path::new(""));
        Self(folder.to_path_buf())
    }

    /// The folder of the model whose file is `weights` and whose rows are
    /// `rows`, in file order.
    #[must_use]
    pub fn of(weights: &Path, rows: &[ModelFile]) -> Self {
        Self::new(weights, rows.first().map(|row| row.file_path.as_str()))
    }

    /// Where the file `row` names is on disk. A row that holds an absolute
    /// path, as an image model's companion's does, is that path.
    #[must_use]
    pub fn file(&self, row: impl AsRef<Path>) -> PathBuf {
        self.0.join(row)
    }

    /// The folder itself.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(weights: &str, first_row: Option<&str>) -> PathBuf {
        ModelFolder::new(Path::new(weights), first_row)
            .path()
            .to_path_buf()
    }

    #[test]
    fn weights_at_the_repository_root_are_read_beside_the_file() {
        assert_eq!(
            folder("/models/o_r/X.Q8_0.gguf", Some("X.Q8_0.gguf")),
            Path::new("/models/o_r")
        );
    }

    #[test]
    fn weights_in_a_subdirectory_are_read_from_the_download_folder() {
        let weights = "/models/o_r/BF16/X-BF16-00001-of-00002.gguf";
        let folder = ModelFolder::new(Path::new(weights), Some("BF16/X-BF16-00001-of-00002.gguf"));

        assert_eq!(folder.path(), Path::new("/models/o_r"));
        assert_eq!(
            folder.file("BF16/X-BF16-00002-of-00002.gguf"),
            Path::new("/models/o_r/BF16/X-BF16-00002-of-00002.gguf")
        );
        assert_eq!(
            folder.file("mmproj-BF16.gguf"),
            Path::new("/models/o_r/mmproj-BF16.gguf")
        );
    }

    #[test]
    fn an_absolute_row_is_kept_as_it_is() {
        let folder = ModelFolder::new(Path::new("/models/o_r/BF16/X.gguf"), Some("BF16/X.gguf"));

        assert_eq!(
            folder.file("/models/other/vae.gguf"),
            Path::new("/models/other/vae.gguf")
        );
    }

    /// A file that does not end with the first row, an absolute first row,
    /// and no rows at all: the directory the file is in, as before.
    #[test]
    fn a_first_row_the_file_does_not_end_with_reads_beside_the_file() {
        assert_eq!(
            folder("/models/o_r/X.gguf", Some("BF16/X.gguf")),
            Path::new("/models/o_r")
        );
        assert_eq!(
            folder("/models/o_r/X.gguf", Some("/models/o_r/X.gguf")),
            Path::new("/models/o_r")
        );
        assert_eq!(folder("/models/o_r/X.gguf", None), Path::new("/models/o_r"));
        assert_eq!(
            folder("/models/o_r/BF16/X.gguf", Some("X.gguf")),
            Path::new("/models/o_r/BF16")
        );
    }

    #[test]
    fn rows_are_read_by_the_first_of_them() {
        let row = |index: i32, path: &str| ModelFile {
            id: i64::from(index),
            model_id: 1,
            file_path: path.to_owned(),
            file_index: index,
            expected_size: 1,
            hf_oid: None,
            last_verified_at: None,
        };
        let rows = [
            row(0, "BF16/X-00001-of-00002.gguf"),
            row(1, "mmproj-BF16.gguf"),
        ];

        let folder = ModelFolder::of(Path::new("/m/o_r/BF16/X-00001-of-00002.gguf"), &rows);

        assert_eq!(folder.path(), Path::new("/m/o_r"));
    }
}
