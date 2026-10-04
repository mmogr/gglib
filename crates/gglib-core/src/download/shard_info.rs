//! One file's place in the group of files a model is downloaded as.

use serde::{Deserialize, Serialize};

use super::file_role::GgufFileRole;

/// Information about one file within a model's download group.
///
/// A group is the model's weights, one file or several shards, followed by
/// the projector fetched with them when the repository has one. Shards are
/// numbered among the weights alone: [`total_shards`](Self::total_shards)
/// never counts a projector, while the byte offsets cover every file.
///
/// [`preceding_bytes`](Self::preceding_bytes) and
/// [`group_total_bytes`](Self::group_total_bytes) carry the exact byte offsets
/// of this shard within the whole model, so aggregate progress does not have to
/// assume every shard is the same size. GGUF shard sets almost always end with
/// a smaller final shard, and estimating the group total as
/// `this_shard_size * shard_count` made the percentage both wrong and
/// discontinuous at every shard boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
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
    /// Size of this shard file in bytes (if known).
    #[cfg_attr(feature = "ts-bindings", ts(type = "number", optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size: Option<u64>,
    /// Summed size of every shard before this one (if all sizes are known).
    #[cfg_attr(feature = "ts-bindings", ts(type = "number", optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preceding_bytes: Option<u64>,
    /// Summed size of every shard in the group (if all sizes are known).
    #[cfg_attr(feature = "ts-bindings", ts(type = "number", optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_total_bytes: Option<u64>,
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
            preceding_bytes: None,
            group_total_bytes: None,
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

    /// Attach the exact byte offsets of this shard within its group.
    ///
    /// Only call this when *every* shard size in the group is known; a partial
    /// offset is worse than none, because the fallback estimate at least stays
    /// self-consistent.
    #[must_use]
    pub const fn with_group_offsets(mut self, preceding: u64, group_total: u64) -> Self {
        self.preceding_bytes = Some(preceding);
        self.group_total_bytes = Some(group_total);
        self
    }

    /// Exact aggregate progress for the group, given this shard's own progress.
    ///
    /// Returns `None` when the group's byte layout is unknown, leaving the
    /// caller to fall back to an equal-shard-size estimate.
    #[must_use]
    pub fn aggregate(&self, shard_downloaded: u64) -> Option<(u64, u64)> {
        let preceding = self.preceding_bytes?;
        let group_total = self.group_total_bytes?;
        let downloaded = preceding.saturating_add(shard_downloaded).min(group_total);
        Some((downloaded, group_total))
    }

    /// Format as display string: "Part 1/3" for a shard, "Projector" for a
    /// projector.
    #[must_use]
    pub fn display(&self) -> String {
        match self.role {
            GgufFileRole::Weights => format!("Part {}/{}", self.shard_index + 1, self.total_shards),
            GgufFileRole::Projector => "Projector".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shard_info_display() {
        let shard = ShardInfo::new(1, 5, "model-00002-of-00005.gguf");
        assert_eq!(shard.display(), "Part 2/5");
    }

    /// A projector follows the shards and is not one of them, so it is not
    /// shown as "Part 4/3".
    #[test]
    fn a_projector_is_displayed_by_its_role() {
        let projector = ShardInfo::new(3, 3, "mmproj-F16.gguf").with_role(GgufFileRole::Projector);
        assert_eq!(projector.display(), "Projector");
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
