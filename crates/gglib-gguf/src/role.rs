//! What a GGUF header says its file is: a model's weights, a projector, or
//! a sidecar that runs only beside a model.
//!
//! llama.cpp's converter writes `general.type = "mmproj"` into a projector.
//! Projectors converted before that key existed carry no `general.type`, and
//! are told by `general.architecture = "clip"`, which no model's weights use.
//!
//! A sidecar is any other GGUF that is not a model on its own. The header
//! says so in one of three ways, each seen in files on the Hub:
//!
//! - `general.type` names something other than a model: `imatrix` (an
//!   importance matrix), `adapter` (a `LoRA`).
//! - `{arch}.target_layers` names the layers of a target model whose hidden
//!   states the file drafts from: `EAGLE3`, `EAGLE4` and `DFlash` drafts.
//! - `{arch}.nextn_predict_layers` is set and the file cannot hold a model's
//!   trunk: every block it declares is a multi-token-prediction block (Gemma
//!   4's assistant heads), or it is one unsplit file with fewer tensors than
//!   blocks (an MTP head cut out of a Qwen 3.8 model, `blk.64` alone of 65).
//!   A model that keeps its own MTP head declares the same key with every
//!   block present, and stays weights.
//!
//! A shard holds fewer tensors than its model has blocks too, so the last
//! test holds only when `split.count` says the file is whole. A head that has
//! not reached `split.count` cannot tell, and leaves the role unsettled.

use gglib_core::GgufFileRole;
use gglib_core::domain::gguf::{GgufValue, RawMetadata};

/// The role a whole header states, `tensor_count` being the tensors the file
/// declares.
pub(crate) fn file_role(raw: &RawMetadata, tensor_count: u64) -> GgufFileRole {
    Header::new(raw, tensor_count)
        .role(true)
        .unwrap_or(GgufFileRole::Weights)
}

/// The role the pairs of a head state, when they settle it: `complete` when
/// the head holds every pair the file declares.
///
/// A head settles a projector or a sidecar on the first sign of one. It
/// settles weights only when it holds every pair: the sign of a draft can
/// sit after megabytes of tokenizer, and after the block count.
pub(crate) fn head_role(
    raw: &RawMetadata,
    tensor_count: u64,
    complete: bool,
) -> Option<GgufFileRole> {
    Header::new(raw, tensor_count).role(complete)
}

/// The keys a role is read from.
struct Header<'a> {
    raw: &'a RawMetadata,
    architecture: Option<&'a str>,
    tensor_count: u64,
}

impl<'a> Header<'a> {
    fn new(raw: &'a RawMetadata, tensor_count: u64) -> Self {
        Self {
            raw,
            architecture: text(raw, "general.architecture"),
            tensor_count,
        }
    }

    /// The architecture's own key `name`, as `{arch}.{name}`.
    fn own(&self, name: &str) -> Option<&'a GgufValue> {
        let architecture = self.architecture?;
        self.raw.get(&format!("{architecture}.{name}"))
    }

    fn own_count(&self, name: &str) -> Option<u64> {
        self.own(name).and_then(GgufValue::as_u64)
    }

    /// The role, or `None` when a pair past a head's end could change it.
    fn role(&self, complete: bool) -> Option<GgufFileRole> {
        let general_type = text(self.raw, "general.type");
        if general_type == Some("mmproj") || self.architecture == Some("clip") {
            return Some(GgufFileRole::Projector);
        }
        if general_type.is_some_and(|kind| kind != "model") || self.own("target_layers").is_some() {
            return Some(GgufFileRole::Sidecar);
        }
        let blocks = self.own_count("block_count");
        let mtp_blocks = self.own_count("nextn_predict_layers");
        if let (Some(blocks), Some(mtp_blocks)) = (blocks, mtp_blocks) {
            if mtp_blocks >= blocks {
                return Some(GgufFileRole::Sidecar);
            }
        }
        if mtp_blocks.is_some() && blocks.is_some_and(|blocks| self.tensor_count < blocks) {
            return match self.raw.get("split.count").and_then(GgufValue::as_u64) {
                Some(shards) if shards > 1 => Some(GgufFileRole::Weights),
                Some(_) => Some(GgufFileRole::Sidecar),
                None if complete => Some(GgufFileRole::Sidecar),
                None => None,
            };
        }
        complete.then_some(GgufFileRole::Weights)
    }
}

fn text<'a>(raw: &'a RawMetadata, key: &str) -> Option<&'a str> {
    raw.get(key).and_then(GgufValue::as_str)
}

#[cfg(test)]
#[path = "role_tests.rs"]
mod tests;
