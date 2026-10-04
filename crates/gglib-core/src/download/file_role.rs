//! What a GGUF file is for: a model's weights, or a multimodal projector.
//!
//! A projector is the second file llama-server loads (`--mmproj`) to give a
//! model image input. It is a GGUF like any other, so a repository listing or
//! a model's own file list holds it beside the weights, and only its name or
//! its header tells the two apart. [`GgufFileRole::classify`] reads the name;
//! `gglib-gguf` reads the header and reports the same type on
//! [`GgufMetadata::role`](crate::domain::GgufMetadata::role).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::types::find_boundary_match;

/// The filename token that marks a projector.
const PROJECTOR_TOKEN: &[u8] = b"mmproj";

/// What a GGUF file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub enum GgufFileRole {
    /// A model's weights, whole or one shard of them.
    #[default]
    Weights,
    /// A multimodal projector, loaded beside the weights with `--mmproj`.
    Projector,
}

impl GgufFileRole {
    /// The role `path`'s file name states.
    ///
    /// A projector carries the token `mmproj`, in either case, flanked by
    /// non-alphanumeric characters or an end of the name: `mmproj-F16.gguf`,
    /// `Qwen3-VL.mmproj-Q8_0.gguf`. Only the file name is read, so a weights
    /// file inside a directory named `mmproj` stays weights.
    #[must_use]
    pub fn classify(path: &Path) -> Self {
        let named_projector = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| find_boundary_match(name.as_bytes(), PROJECTOR_TOKEN).is_some());
        if named_projector {
            Self::Projector
        } else {
            Self::Weights
        }
    }

    /// The files among a model's own that are named as a projector.
    ///
    /// `names` are the model's files as the library records them. A
    /// `model_files` name is relative to the directory `weights` is in and is
    /// answered as a path there; a name that is already an absolute path is
    /// answered as it is. The order is the order given. `weights` itself is
    /// never one, whatever it is named, and nothing is read from disk.
    pub fn projectors_among<S: AsRef<Path>>(
        weights: &Path,
        names: impl IntoIterator<Item = S>,
    ) -> impl Iterator<Item = PathBuf> {
        let dir = weights.parent().unwrap_or_else(|| Path::new(""));
        names
            .into_iter()
            .map(move |name| dir.join(name))
            .filter(move |file| file != weights && Self::classify(file).is_projector())
    }

    /// Whether this is a projector.
    #[must_use]
    pub const fn is_projector(self) -> bool {
        matches!(self, Self::Projector)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn role(name: &str) -> GgufFileRole {
        GgufFileRole::classify(Path::new(name))
    }

    #[test]
    fn a_name_that_starts_with_the_token_is_a_projector() {
        assert_eq!(role("mmproj-F16.gguf"), GgufFileRole::Projector);
    }

    #[test]
    fn a_name_that_holds_the_token_after_a_dot_is_a_projector() {
        assert_eq!(role("X.mmproj-Q8_0.gguf"), GgufFileRole::Projector);
    }

    #[test]
    fn a_weights_name_without_the_token_is_weights() {
        assert_eq!(role("X.Q8_0.gguf"), GgufFileRole::Weights);
        assert_eq!(
            role("Qwen3-27B-Q8_0-00001-of-00004.gguf"),
            GgufFileRole::Weights
        );
    }

    #[test]
    fn the_token_matches_in_either_case() {
        assert_eq!(role("MMPROJ-F16.gguf"), GgufFileRole::Projector);
        assert_eq!(role("model-MmProj-f16.gguf"), GgufFileRole::Projector);
        assert_eq!(role("model.mmproj.GGUF"), GgufFileRole::Projector);
    }

    /// The token is a whole word of the name: letters or digits on either
    /// side make it part of some other word.
    #[test]
    fn the_token_inside_a_longer_word_is_not_a_projector() {
        assert_eq!(role("summproj-Q8_0.gguf"), GgufFileRole::Weights);
        assert_eq!(role("mmprojector-Q8_0.gguf"), GgufFileRole::Weights);
        assert_eq!(role("x-mmproj2-Q8_0.gguf"), GgufFileRole::Weights);
    }

    #[test]
    fn only_the_file_name_is_read() {
        assert_eq!(role("/models/mmproj/X.Q8_0.gguf"), GgufFileRole::Weights);
        assert_eq!(role("/models/X/mmproj-F16.gguf"), GgufFileRole::Projector);
    }

    /// The model-3 shape: the weights and a projector as two names in one
    /// directory.
    #[test]
    fn a_models_projector_is_found_beside_its_weights() {
        let weights = Path::new("/models/X/X.Q8_0.gguf");
        let names = ["X.Q8_0.gguf", "X.mmproj-Q8_0.gguf", "X.Q4_K_M.gguf"];

        let found: Vec<_> = GgufFileRole::projectors_among(weights, names).collect();

        assert_eq!(found, [Path::new("/models/X/X.mmproj-Q8_0.gguf")]);
    }

    #[test]
    fn an_absolute_name_is_kept_and_the_given_order_holds() {
        let weights = Path::new("/models/X/X.Q8_0.gguf");
        let names = ["/elsewhere/mmproj-F16.gguf", "mmproj-Q8_0.gguf"];

        let found: Vec<_> = GgufFileRole::projectors_among(weights, names).collect();

        assert_eq!(
            found,
            [
                Path::new("/elsewhere/mmproj-F16.gguf"),
                Path::new("/models/X/mmproj-Q8_0.gguf")
            ]
        );
    }

    /// Weights named like a projector are still the weights, by name or by
    /// absolute path.
    #[test]
    fn the_weights_are_never_their_own_projector() {
        let weights = Path::new("/models/X/mmproj-as-weights.gguf");
        let names = ["mmproj-as-weights.gguf", "/models/X/mmproj-as-weights.gguf"];

        assert_eq!(GgufFileRole::projectors_among(weights, names).count(), 0);
    }

    #[test]
    fn weights_is_the_default() {
        assert_eq!(GgufFileRole::default(), GgufFileRole::Weights);
        assert!(!GgufFileRole::Weights.is_projector());
        assert!(GgufFileRole::Projector.is_projector());
    }
}
