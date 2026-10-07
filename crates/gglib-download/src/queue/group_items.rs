//! The queue items of one download group.

use gglib_core::download::{CompletionKey, DownloadId, ShardInfo};
use gglib_core::ports::ResolvedFile;

use crate::executor::known_size;

use super::shard_group::ShardGroupId;
use super::types::QueuedItem;
use super::usize_to_u32_saturating;

/// One queue item per file of a download group, in the order given: the
/// weights, then the projector fetched with them.
///
/// Each item carries its place in the group. Shards are numbered among the
/// weights alone, so a projector never raises the shard total; the group's
/// size covers every file, so progress runs over the whole group.
pub(super) fn group_items(
    id: &DownloadId,
    completion_key: &CompletionKey,
    files: &[ResolvedFile],
) -> Vec<QueuedItem> {
    let group_id = ShardGroupId::generate(id);
    let total_shards =
        usize_to_u32_saturating(files.iter().filter(|f| !f.role.is_projector()).count());

    // The size of the whole group, but only when HuggingFace gave a size
    // for every file: a sum that leaves one out would be a total the bytes
    // run past. A size of 0 is one it did not give.
    let group_total: Option<u64> = files.iter().map(|f| known_size(f.size)).sum();
    let has_projector = files.iter().any(|f| f.role.is_projector());

    files
        .iter()
        .enumerate()
        .map(|(idx, file)| {
            let index = usize_to_u32_saturating(idx);
            let shard_info = known_size(file.size)
                .map_or_else(
                    || ShardInfo::new(index, total_shards, &file.path),
                    |size| ShardInfo::with_size(index, total_shards, &file.path, size),
                )
                .with_role(file.role)
                .in_group(group_total, has_projector);
            QueuedItem::new_shard(
                id.clone(),
                group_id.clone(),
                shard_info,
                completion_key.clone(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::download::{FilePlace, GgufFileRole};

    fn items(files: &[ResolvedFile]) -> Vec<ShardInfo> {
        let id = DownloadId::new("owner/zeta-GGUF", Some("Q8_0"));
        let key = CompletionKey::HfFile {
            repo_id: "owner/zeta-GGUF".to_string(),
            revision: "main".to_string(),
            filename_canon: "zeta.Q8_0.gguf".to_string(),
            quantization: Some("Q8_0".to_string()),
        };
        group_items(&id, &key, files)
            .into_iter()
            .map(|item| item.shard_info.expect("every group item has its place"))
            .collect()
    }

    /// One weights file and a projector: a model of one shard, and a group
    /// of two files.
    #[test]
    fn a_projector_is_an_item_of_the_group_and_not_a_shard() {
        let group = items(&[
            ResolvedFile::with_size("zeta.Q8_0.gguf", 1_000),
            ResolvedFile::projector("mmproj-F16.gguf", 300, None),
        ]);

        assert_eq!(group.len(), 2);
        assert_eq!(group[0].role, GgufFileRole::Weights);
        assert_eq!((group[0].shard_index, group[0].total_shards), (0, 1));
        assert_eq!(group[1].role, GgufFileRole::Projector);
        assert_eq!(group[1].filename, "mmproj-F16.gguf");
        assert_eq!(group[1].total_shards, 1, "the projector is not a shard");
        assert_eq!(group[0].place(), Some(FilePlace::Weights));
        assert_eq!(group[1].place(), Some(FilePlace::Projector));
        assert!(group.iter().all(|file| file.has_projector));
    }

    #[test]
    fn three_shards_and_a_projector_are_three_shards() {
        let group = items(&[
            ResolvedFile::with_size("z-00001-of-00003.gguf", 1_000),
            ResolvedFile::with_size("z-00002-of-00003.gguf", 1_000),
            ResolvedFile::with_size("z-00003-of-00003.gguf", 500),
            ResolvedFile::projector("mmproj-F16.gguf", 300, None),
        ]);

        let numbering: Vec<_> = group
            .iter()
            .map(|file| (file.shard_index, file.total_shards))
            .collect();
        assert_eq!(numbering, [(0, 3), (1, 3), (2, 3), (3, 3)]);
        assert_eq!(group[2].place(), Some(FilePlace::Part { number: 3, of: 3 }));
        assert_eq!(group[3].place(), Some(FilePlace::Projector));
        assert_eq!(group[0].waiting_place(), Some(FilePlace::Parts(3)));
    }

    /// Download progress covers every file: the projector's bytes are in the
    /// group total, which every file of the group carries.
    #[test]
    fn the_group_total_covers_the_projector() {
        let group = items(&[
            ResolvedFile::with_size("z-00001-of-00002.gguf", 1_000),
            ResolvedFile::with_size("z-00002-of-00002.gguf", 500),
            ResolvedFile::projector("mmproj-F16.gguf", 300, None),
        ]);

        assert!(group.iter().all(|f| f.group_total_bytes == Some(1_800)));
    }

    #[test]
    fn a_group_without_a_projector_is_numbered_as_before() {
        let group = items(&[
            ResolvedFile::with_size("z-00001-of-00002.gguf", 1_000),
            ResolvedFile::with_size("z-00002-of-00002.gguf", 500),
        ]);

        assert!(group.iter().all(|f| f.role == GgufFileRole::Weights));
        assert_eq!((group[1].shard_index, group[1].total_shards), (1, 2));
        assert_eq!(group[1].group_total_bytes, Some(1_500));
        assert!(group.iter().all(|f| !f.has_projector));
    }

    /// One unknown size and the group has no total.
    #[test]
    fn a_file_of_unknown_size_leaves_the_group_without_a_total() {
        let group = items(&[
            ResolvedFile::new("zeta.Q8_0.gguf"),
            ResolvedFile::projector("mmproj-F16.gguf", 300, None),
        ]);

        assert!(group.iter().all(|f| f.group_total_bytes.is_none()));
        assert_eq!(group[1].file_size, Some(300));
    }

    /// Metadata with no size arrives as a size of 0, which is no size.
    #[test]
    fn a_size_of_zero_is_an_unknown_size() {
        let group = items(&[
            ResolvedFile::with_size("zeta.Q8_0.gguf", 0),
            ResolvedFile::projector("mmproj-F16.gguf", 300, None),
        ]);

        assert_eq!(group[0].file_size, None);
        assert!(group.iter().all(|f| f.group_total_bytes.is_none()));
    }
}
