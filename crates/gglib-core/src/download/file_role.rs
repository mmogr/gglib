//! What a GGUF file is for: a model's weights, a multimodal projector, or a
//! sidecar that ships beside them.
//!
//! A projector is the second file llama-server loads (`--mmproj`) to give a
//! model image input. It is a GGUF like any other, so a repository listing or
//! a model's own file list holds it beside the weights, and only its name or
//! its header tells the two apart. [`GgufFileRole::classify`] reads the name;
//! `gglib-gguf` reads the header and reports the same type on
//! [`GgufMetadata::role`](crate::domain::GgufMetadata::role).
//!
//! A sidecar is a GGUF a repository keeps beside the weights that is neither
//! weights nor a projector: an importance matrix, or a multi-token
//! prediction (MTP) head. It is no quantization of the model and no shard of
//! one.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::types::find_boundary_match;

/// The filename token that marks a projector.
const PROJECTOR_TOKEN: &[u8] = b"mmproj";

/// The filename tokens that mark a sidecar: an importance matrix, and an MTP
/// head.
const SIDECAR_TOKENS: [&[u8]; 2] = [b"imatrix", b"mtp"];

/// The directory a repository keeps its MTP heads in.
const MTP_DIRECTORY: &str = "MTP";

/// What a GGUF file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub enum GgufFileRole {
    /// A model's weights, whole or one shard of them.
    #[default]
    Weights,
    /// A multimodal projector, loaded beside the weights with `--mmproj`.
    Projector,
    /// A file that ships beside the weights and is neither: an importance
    /// matrix, or an MTP head.
    Sidecar,
}

impl GgufFileRole {
    /// The role `path`'s file name states.
    ///
    /// A projector carries the token `mmproj`, in either case, flanked by
    /// non-alphanumeric characters or an end of the name: `mmproj-F16.gguf`,
    /// `Qwen3-VL.mmproj-Q8_0.gguf`. Only the file name is read, so a weights
    /// file inside a directory named `mmproj` stays weights.
    ///
    /// A sidecar carries the token `imatrix` or `mtp` the same way
    /// (`imatrix_unsloth.gguf`, `mtp-X-Q4_0.gguf`), or sits in a directory
    /// named `MTP`, in either case. The projector's token wins over both.
    #[must_use]
    pub fn classify(path: &Path) -> Self {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let names = |token: &[u8]| find_boundary_match(name.as_bytes(), token).is_some();
        let in_mtp_directory = path
            .parent()
            .and_then(Path::file_name)
            .and_then(|dir| dir.to_str())
            .is_some_and(|dir| dir.eq_ignore_ascii_case(MTP_DIRECTORY));
        if names(PROJECTOR_TOKEN) {
            Self::Projector
        } else if in_mtp_directory || SIDECAR_TOKENS.into_iter().any(names) {
            Self::Sidecar
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

    /// Whether this is a model's weights: neither a projector nor a sidecar.
    #[must_use]
    pub const fn is_weights(self) -> bool {
        matches!(self, Self::Weights)
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
    fn an_importance_matrix_is_a_sidecar() {
        assert_eq!(role("imatrix_unsloth.gguf"), GgufFileRole::Sidecar);
        assert_eq!(role("X.imatrix.gguf"), GgufFileRole::Sidecar);
        assert_eq!(role("X-IMATRIX.gguf"), GgufFileRole::Sidecar);
    }

    #[test]
    fn an_mtp_head_is_a_sidecar_by_its_name_or_its_directory() {
        assert_eq!(role("MTP/mtp-Qwen3.8-27B-Q4_0.gguf"), GgufFileRole::Sidecar);
        assert_eq!(role("mtp-Qwen3.8-27B-Q4_0.gguf"), GgufFileRole::Sidecar);
        assert_eq!(role("MTP/Qwen3.8-27B-Q4_0.gguf"), GgufFileRole::Sidecar);
        assert_eq!(role("mtp/Qwen3.8-27B-Q4_0.gguf"), GgufFileRole::Sidecar);
    }

    /// The sidecar tokens are whole words too, and only the directory the
    /// file is in counts.
    #[test]
    fn a_sidecar_token_inside_a_longer_word_is_weights() {
        assert_eq!(role("smtp-Q4_0.gguf"), GgufFileRole::Weights);
        assert_eq!(role("mtpx-Q4_0.gguf"), GgufFileRole::Weights);
        assert_eq!(role("imatrixed-Q4_0.gguf"), GgufFileRole::Weights);
        assert_eq!(role("MTP/BF16/X-BF16.gguf"), GgufFileRole::Weights);
        assert_eq!(role("MTPX/X-Q4_0.gguf"), GgufFileRole::Weights);
    }

    #[test]
    fn a_projector_named_like_a_sidecar_is_a_projector() {
        assert_eq!(role("MTP/mmproj-F16.gguf"), GgufFileRole::Projector);
        assert_eq!(role("mmproj-imatrix.gguf"), GgufFileRole::Projector);
    }

    #[test]
    fn weights_is_the_default() {
        assert_eq!(GgufFileRole::default(), GgufFileRole::Weights);
        assert!(!GgufFileRole::Weights.is_projector());
        assert!(GgufFileRole::Projector.is_projector());
        assert!(GgufFileRole::Weights.is_weights());
        assert!(!GgufFileRole::Projector.is_weights());
        assert!(!GgufFileRole::Sidecar.is_weights());
        assert!(!GgufFileRole::Sidecar.is_projector());
    }
}
