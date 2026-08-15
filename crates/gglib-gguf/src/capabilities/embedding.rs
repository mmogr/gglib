//! Embedding-model capability detection.
//!
//! Detects whether a GGUF file describes a model whose purpose is producing
//! embeddings rather than generating text, so the launch path can start
//! llama-server with `--embeddings`.
//!
//! # Why this has to be decided at import time
//!
//! `--embeddings` **restricts** llama-server to the embedding use case: a
//! server started with it refuses `/v1/chat/completions`, and a server started
//! without it answers `/v1/embeddings` with a 501.  The mode is therefore a
//! property of the launch, not of the request, and the only way a proxy can
//! pick it correctly is to know what kind of model it is about to load.
//!
//! # Detection strategy
//!
//! Two signals, in order, both read **exclusively** from the GGUF key-value
//! metadata:
//!
//! 1. **`{arch}.pooling_type` greater than zero.**  Zero is llama.cpp's
//!    `NONE`, and `/v1/embeddings` requires a pooling type other than none.
//!    This is the signal that catches decoder-architecture embedders —
//!    Qwen3-Embedding, `EmbeddingGemma` — whose `general.architecture` is
//!    indistinguishable from that of their chat siblings.
//! 2. **An encoder-only `general.architecture`.**  The backstop for a
//!    BERT-family file whose converter omitted the pooling key.  The list is
//!    deliberately confined to architectures llama.cpp supports for nothing
//!    but embeddings, so a match cannot be a generative model.
//!
//! No filename or model-name heuristics, for the same reason as
//! [`super::mtp`]: a file named `*-embed-*` that was converted without a
//! pooling type would be tagged on the strength of its name alone, and the
//! resulting launch would serve unpooled garbage.

use std::collections::HashMap;

/// `general.architecture` values llama.cpp only ever serves as embedding
/// models.  Used as a backstop when no pooling type is present.
const ENCODER_ONLY_ARCHITECTURES: &[&str] = &[
    "bert",
    "nomic-bert",
    "nomic-bert-moe",
    "neo-bert",
    "jina-bert-v2",
    "modern-bert",
    "t5encoder",
];

/// Detect embedding support from raw GGUF key-value metadata.
///
/// Returns `true` when the model should be launched with `--embeddings`.
#[must_use]
pub(crate) fn detect_embedding_support(metadata: &HashMap<String, String>) -> bool {
    let pooled = metadata.iter().any(|(key, value)| {
        key.ends_with(".pooling_type") && value.parse::<u32>().is_ok_and(|t| t > 0)
    });
    if pooled {
        return true;
    }

    metadata
        .get("general.architecture")
        .is_some_and(|arch| ENCODER_ONLY_ARCHITECTURES.contains(&arch.to_lowercase().as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn detects_mean_pooling_on_a_bert_encoder() {
        assert!(detect_embedding_support(&meta(&[
            ("general.architecture", "bert"),
            ("bert.pooling_type", "1"),
        ])));
    }

    /// The case the architecture backstop cannot cover: a decoder-architecture
    /// embedder that shares `general.architecture` with a chat model.
    #[test]
    fn detects_last_pooling_on_a_decoder_architecture() {
        assert!(detect_embedding_support(&meta(&[
            ("general.architecture", "qwen3"),
            ("qwen3.pooling_type", "3"),
        ])));
    }

    #[test]
    fn pooling_type_none_is_not_an_embedding_model() {
        assert!(!detect_embedding_support(&meta(&[
            ("general.architecture", "qwen3"),
            ("qwen3.pooling_type", "0"),
        ])));
    }

    #[test]
    fn non_numeric_pooling_type_is_ignored() {
        assert!(!detect_embedding_support(&meta(&[
            ("general.architecture", "qwen3"),
            ("qwen3.pooling_type", "mean"),
        ])));
    }

    #[test]
    fn every_encoder_only_architecture_is_detected_without_a_pooling_key() {
        for arch in ENCODER_ONLY_ARCHITECTURES {
            assert!(
                detect_embedding_support(&meta(&[("general.architecture", arch)])),
                "{arch} should be detected as an embedding architecture"
            );
        }
    }

    #[test]
    fn architecture_match_is_case_insensitive() {
        assert!(detect_embedding_support(&meta(&[(
            "general.architecture",
            "Nomic-BERT"
        )])));
    }

    #[test]
    fn a_generative_architecture_without_pooling_is_not_detected() {
        assert!(!detect_embedding_support(&meta(&[
            ("general.architecture", "llama"),
            ("llama.context_length", "8192"),
        ])));
    }

    #[test]
    fn empty_metadata_is_not_detected() {
        assert!(!detect_embedding_support(&HashMap::new()));
    }

    /// Same rule as MTP detection: the name proves nothing about what the file
    /// actually contains.
    #[test]
    fn model_name_heuristic_does_not_trigger_detection() {
        assert!(!detect_embedding_support(&meta(&[
            ("general.name", "nomic-embed-text-v1.5"),
            ("general.architecture", "llama"),
        ])));
    }
}
