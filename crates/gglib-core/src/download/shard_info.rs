//! One file's place in the group of files a model is downloaded as.

use super::file_role::GgufFileRole;
use super::row::FilePlace;

/// Information about one file within a model's download group.
///
/// A group is the model's weights, one file or several shards, followed by
/// the projector fetched with them when the repository has one. Shards are
/// numbered among the weights alone: [`total_shards`](Self::total_shards)
/// never counts a projector, while
/// [`group_total_bytes`](Self::group_total_bytes) covers every file.
///
/// This is the queue's own record. What a client is shown of it is the
/// [`FilePlace`] in a row's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShardInfo {
    /// 0-based position of this file in its group. Weights come first, so a
    /// shard's position is its shard number; a projector follows the last
    /// shard.
    pub shard_index: u32,
    /// Number of weights shards in this model.
    pub total_shards: u32,
    /// The specific filename for this file.
    pub filename: String,
    /// What this file is: a shard of the weights, or the projector.
    pub role: GgufFileRole,
    /// Size of this file in bytes (if known).
    pub file_size: Option<u64>,
    /// Summed size of every file in the group (if all sizes are known).
    pub group_total_bytes: Option<u64>,
    /// Whether the group has a projector after its weights.
    pub has_projector: bool,
}

impl ShardInfo {
    /// Create a new `ShardInfo` instance for a weights shard.
    #[must_use]
    pub fn new(shard_index: u32, total_shards: u32, filename: impl Into<String>) -> Self {
        Self {
            shard_index,
            total_shards,
            filename: filename.into(),
            role: GgufFileRole::Weights,
            file_size: None,
            group_total_bytes: None,
            has_projector: false,
        }
    }

    /// Create a new `ShardInfo` instance with file size.
    #[must_use]
    pub fn with_size(
        shard_index: u32,
        total_shards: u32,
        filename: impl Into<String>,
        file_size: u64,
    ) -> Self {
        Self {
            file_size: Some(file_size),
            ..Self::new(shard_index, total_shards, filename)
        }
    }

    /// Mark this file as `role`.
    #[must_use]
    pub const fn with_role(mut self, role: GgufFileRole) -> Self {
        self.role = role;
        self
    }

    /// Record what is known of the whole group: the size of every file
    /// together, when every one is known, and whether a projector is among
    /// them.
    #[must_use]
    pub const fn in_group(mut self, group_total: Option<u64>, has_projector: bool) -> Self {
        self.group_total_bytes = group_total;
        self.has_projector = has_projector;
        self
    }

    /// Whether this is the only file of its group.
    #[must_use]
    pub const fn is_alone(&self) -> bool {
        self.total_shards <= 1 && !self.has_projector
    }

    /// How this file is named on the row of a running download: its number
    /// among the shards, `weights` when one weights file is fetched with a
    /// projector, `projector`, and nothing for a download of one file.
    #[must_use]
    pub const fn place(&self) -> Option<FilePlace> {
        match self.role {
            GgufFileRole::Projector => Some(FilePlace::Projector),
            GgufFileRole::Weights if self.total_shards > 1 => Some(FilePlace::Part {
                number: self.shard_index + 1,
                of: self.total_shards,
            }),
            GgufFileRole::Weights if self.has_projector => Some(FilePlace::Weights),
            GgufFileRole::Weights => None,
        }
    }

    /// How the group is named on the row of a waiting download: by its
    /// number of shards, when it has more than one.
    #[must_use]
    pub const fn waiting_place(&self) -> Option<FilePlace> {
        if self.total_shards > 1 {
            Some(FilePlace::Parts(self.total_shards))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shard_is_placed_by_its_number() {
        let shard = ShardInfo::new(1, 5, "model-00002-of-00005.gguf");

        assert_eq!(shard.place(), Some(FilePlace::Part { number: 2, of: 5 }));
        assert_eq!(shard.waiting_place(), Some(FilePlace::Parts(5)));
    }

    /// A projector follows the shards and is not one of them, so it is not
    /// "part 4/3".
    #[test]
    fn a_projector_is_placed_by_its_role() {
        let projector = ShardInfo::new(3, 3, "mmproj-F16.gguf")
            .with_role(GgufFileRole::Projector)
            .in_group(None, true);

        assert_eq!(projector.place(), Some(FilePlace::Projector));
    }

    /// One weights file is "weights" only beside a projector: alone, the row
    /// is about the whole download and names no file.
    #[test]
    fn a_single_weights_file_is_named_only_beside_a_projector() {
        let alone = ShardInfo::new(0, 1, "m.gguf");
        let beside = ShardInfo::new(0, 1, "m.gguf").in_group(None, true);

        assert_eq!(alone.place(), None);
        assert!(alone.is_alone());
        assert_eq!(beside.place(), Some(FilePlace::Weights));
        assert!(!beside.is_alone());
        assert_eq!(beside.waiting_place(), None);
    }

    #[test]
    fn a_new_shard_info_is_a_weights_shard() {
        assert_eq!(ShardInfo::new(0, 1, "m.gguf").role, GgufFileRole::Weights);
        assert_eq!(
            ShardInfo::with_size(0, 1, "m.gguf", 10).role,
            GgufFileRole::Weights
        );
    }
}
