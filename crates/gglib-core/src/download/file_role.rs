//! What a GGUF file is for: a model's weights, a multimodal projector, or a
//! sidecar that ships beside them.
//!
//! A projector is the second file llama-server loads (`--mmproj`) to give a
//! model image input. It is a GGUF like any other, so a repository listing or
//! a model's own file list holds it beside the weights, and only its header
//! tells the two apart: `gglib-gguf` reads it and reports the role on
//! [`GgufMetadata::role`](crate::domain::GgufMetadata::role), or on a head's
//! [`HeadMetadata::role`](crate::domain::HeadMetadata::role) once the head
//! settles it. A file's name is never read for its role: an uploader or a
//! user may name a file anything.
//!
//! A sidecar is a GGUF a repository keeps beside the weights that is neither
//! weights nor a projector: an importance matrix, an adapter, or a draft
//! model's head. It is no quantization of the model and no shard of one.

use serde::{Deserialize, Serialize};

/// What a GGUF file holds, as its header says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub enum GgufFileRole {
    /// A model's weights, whole or one shard of them.
    #[default]
    Weights,
    /// A multimodal projector, loaded beside the weights with `--mmproj`.
    Projector,
    /// A file that ships beside the weights and is neither: an importance
    /// matrix, an adapter, or a draft model's head.
    Sidecar,
}

impl GgufFileRole {
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
