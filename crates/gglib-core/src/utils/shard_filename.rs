//! Utilities for normalizing sharded filenames to a stable base name.

/// Strip shard suffix from a filename to get the stable base name.
///
/// This ensures all shards in a group compute the same identity:
/// - `model-00001-of-00005.gguf` → `model.gguf`
/// - `llama-3-8b-q4_k_m-00003-of-00008.gguf` → `llama-3-8b-q4_k_m.gguf`
///
/// The pattern `-<digits>-of-<digits>` is stripped only when it appears
/// immediately before the file extension.
pub fn base_shard_filename(name: &str) -> String {
    // Find the extension
    let Some(dot) = name.rfind('.') else {
        return name.to_string();
    };
    let (stem, ext) = name.split_at(dot); // ext includes '.'

    // Look for the last "-<digits>-of-<digits>" at end of stem
    // Parse from right to left: ... - N - of - M
    let mut parts = stem.rsplitn(3, '-');
    let a = parts.next(); // last chunk (should be digits, e.g., "00005")
    let b = parts.next(); // should be "of"
    let c = parts.next(); // preceding chunk (e.g., "model-00001" or rest of name)

    match (a, b, c) {
        (Some(m), Some("of"), Some(prefix_and_n)) if m.chars().all(|ch| ch.is_ascii_digit()) => {
            // prefix_and_n is "<prefix>-<n>" where n should also be digits
            if let Some((prefix, n)) = prefix_and_n.rsplit_once('-') {
                if n.chars().all(|ch| ch.is_ascii_digit()) {
                    // Valid shard pattern found
                    return format!("{prefix}{ext}");
                }
            }
            // Doesn't match pattern, return original
            name.to_string()
        }
        _ => name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_shard_filename() {
        // Sharded filenames
        assert_eq!(
            base_shard_filename("model-00001-of-00005.gguf"),
            "model.gguf"
        );
        assert_eq!(
            base_shard_filename("llama-3-8b-q4_k_m-00003-of-00008.gguf"),
            "llama-3-8b-q4_k_m.gguf"
        );
        assert_eq!(
            base_shard_filename("model-00010-of-00100.gguf"),
            "model.gguf"
        );

        // Non-sharded filenames (should pass through unchanged)
        assert_eq!(base_shard_filename("model.gguf"), "model.gguf");
        assert_eq!(
            base_shard_filename("llama-3-8b-q4_k_m.gguf"),
            "llama-3-8b-q4_k_m.gguf"
        );

        // Edge cases
        assert_eq!(base_shard_filename("noextension"), "noextension");
        assert_eq!(
            base_shard_filename("has-numbers-123.gguf"),
            "has-numbers-123.gguf"
        );
        assert_eq!(
            base_shard_filename("model-of-something.gguf"),
            "model-of-something.gguf"
        );

        // Future-proofing: -of- pattern appears earlier but not at end
        assert_eq!(
            base_shard_filename("model-of-doom-00001-of-00005.gguf"),
            "model-of-doom.gguf"
        );
        assert_eq!(
            base_shard_filename("prefix-of-test.gguf"),
            "prefix-of-test.gguf" // No digits, should not strip
        );

        // No extension edge case
        assert_eq!(
            base_shard_filename("model-00001-of-00005"),
            "model-00001-of-00005" // No dot, return unchanged
        );
    }
}
