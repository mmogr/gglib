//! Embedding-mode flag resolution for llama.cpp launches.
//!
//! Unlike every other resolver in this module there is no caller override to
//! weigh: `--embeddings` is not a preference, it is a statement about what the
//! model *is*.  llama-server reads the flag as "restrict to only the embedding
//! use case", so forcing it on for a chat model would produce a server that
//! refuses chat completions, and forcing it off for an embedding model would
//! produce one that 501s on `/v1/embeddings`.  Neither is a choice worth
//! exposing, so this resolves from the model's tags and nothing else.

/// Whether to pass `--embeddings` for a model carrying these tags.
///
/// The `"embedding"` tag is written at import time by
/// `gglib_gguf::capabilities`, from the GGUF's pooling type or an encoder-only
/// architecture.
#[must_use]
pub fn resolve_embeddings_flag(tags: &[String]) -> bool {
    tags.iter().any(|tag| tag.eq_ignore_ascii_case("embedding"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn embedding_tag_enables_the_flag() {
        assert!(resolve_embeddings_flag(&tags(&["embedding"])));
    }

    #[test]
    fn tag_match_is_case_insensitive() {
        assert!(resolve_embeddings_flag(&tags(&["Embedding"])));
    }

    #[test]
    fn other_tags_do_not_enable_the_flag() {
        assert!(!resolve_embeddings_flag(&tags(&[
            "agent",
            "reasoning",
            "mtp",
            "format:qwen-xml",
        ])));
    }

    #[test]
    fn no_tags_leaves_the_flag_off() {
        assert!(!resolve_embeddings_flag(&[]));
    }

    #[test]
    fn the_tag_is_found_alongside_others() {
        assert!(resolve_embeddings_flag(&tags(&["code", "embedding"])));
    }
}
