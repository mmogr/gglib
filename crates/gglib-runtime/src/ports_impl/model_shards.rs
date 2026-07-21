//! Multi-part GGUF shard naming and on-disk weight sizing.
//!
//! Split out from [`super::model_catalog`] so that module stays a pure
//! repository→port mapping adapter: this is filesystem arithmetic, and it has
//! consumers (cache-RAM auto-sizing) that have nothing to do with the catalog.

/// Split a GGUF shard filename into its `(prefix, total_shards)` parts.
///
/// Multi-part GGUFs follow the upstream convention
/// `{prefix}-{index:05}-of-{total:05}.gguf`. Returns `None` for single-file
/// models (the overwhelmingly common case).
fn parse_shard_name(file_name: &str) -> Option<(&str, u32)> {
    let stem = file_name.strip_suffix(".gguf")?;
    // …-00001-of-00004  →  split off "00004", then "of", then "00001".
    let (rest, total) = stem.rsplit_once("-of-")?;
    let total: u32 = total.parse().ok()?;
    let (prefix, index) = rest.rsplit_once('-')?;
    // Index must be numeric for this to be a shard name rather than a model
    // whose own name happens to contain "-of-".
    index.parse::<u32>().ok()?;
    (total > 0).then_some((prefix, total))
}

/// Total on-disk size of a model's weights in bytes.
///
/// For a multi-part GGUF this sums every shard, since llama-server loads them
/// all — counting only the first shard would badly under-report the memory the
/// weights occupy and inflate any budget derived from it. Returns `0` when the
/// size can't be read, which callers treat as "unknown".
///
/// Public so other launch surfaces that need the same figure for cache-RAM
/// auto-sizing (e.g. `gglib-app-services`' direct model-serve path) don't
/// have to reimplement multi-shard summing.
pub fn total_model_bytes(file_path: &std::path::Path) -> u64 {
    let single = || file_path.metadata().map(|md| md.len()).unwrap_or(0);

    let (Some(dir), Some(file_name)) = (
        file_path.parent(),
        file_path.file_name().and_then(|n| n.to_str()),
    ) else {
        return single();
    };
    let Some((prefix, total)) = parse_shard_name(file_name) else {
        return single();
    };

    (1..=total)
        .map(|i| {
            let shard = dir.join(format!("{prefix}-{i:05}-of-{total:05}.gguf"));
            shard.metadata().map(|md| md.len()).unwrap_or(0)
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Shard-name parsing ───────────────────────────────────────────────

    #[test]
    fn parses_a_multipart_shard_name() {
        assert_eq!(
            parse_shard_name("Qwen3-27B-Q8_0-00001-of-00004.gguf"),
            Some(("Qwen3-27B-Q8_0", 4))
        );
    }

    #[test]
    fn single_file_model_is_not_a_shard() {
        assert_eq!(parse_shard_name("Qwen3.6-27B-Q8_0.gguf"), None);
    }

    /// A model whose own name contains "-of-" must not be mistaken for a shard.
    #[test]
    fn non_numeric_shard_index_is_rejected() {
        assert_eq!(parse_shard_name("tale-of-two-models.gguf"), None);
    }

    #[test]
    fn non_gguf_extension_is_rejected() {
        assert_eq!(parse_shard_name("model-00001-of-00002.bin"), None);
    }

    // ── Size summing ─────────────────────────────────────────────────────

    #[test]
    fn total_model_bytes_reads_a_single_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.gguf");
        std::fs::write(&path, vec![0u8; 2048]).unwrap();
        assert_eq!(total_model_bytes(&path), 2048);
    }

    /// The whole point of the shard logic: all parts count toward the weight
    /// footprint, not just the one we were pointed at.
    #[test]
    fn total_model_bytes_sums_every_shard() {
        let dir = tempfile::tempdir().unwrap();
        for i in 1..=3u32 {
            let p = dir.path().join(format!("m-{i:05}-of-00003.gguf"));
            std::fs::write(&p, vec![0u8; 1000]).unwrap();
        }
        let first = dir.path().join("m-00001-of-00003.gguf");
        assert_eq!(total_model_bytes(&first), 3000);
    }

    /// A missing sibling shard contributes 0 rather than aborting the sum.
    #[test]
    fn total_model_bytes_tolerates_a_missing_shard() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("m-00001-of-00002.gguf");
        std::fs::write(&first, vec![0u8; 1500]).unwrap();
        assert_eq!(total_model_bytes(&first), 1500);
    }

    #[test]
    fn total_model_bytes_is_zero_for_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(total_model_bytes(&dir.path().join("nope.gguf")), 0);
    }
}
