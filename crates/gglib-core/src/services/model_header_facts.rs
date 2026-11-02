//! What a model's header states about its type: the type its first file
//! names, and the bits per weight of its tensor data over every shard.
//!
//! One rule for the four places that read them: an import, a download's
//! registration or upgrade, the start-up pass that fills a model added
//! before they were kept, and a retag of a model that still has neither.

use std::path::{Path, PathBuf};

use crate::domain::{FileType, GgufMetadata, Model, TensorPayload};
use crate::ports::GgufParserPort;

/// A model's type and bits per weight, as its files' headers give them.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HeaderFacts {
    /// The type the first file's header states.
    pub file_type: Option<FileType>,
    /// The bits per weight of every shard's tensor data together.
    pub bits_per_weight: Option<f64>,
}

impl HeaderFacts {
    /// Whether the header gave neither fact.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.file_type.is_none() && self.bits_per_weight.is_none()
    }
}

/// The facts of the model whose first file is `file_path`, which `gguf`
/// holds the parse of when it could be read.
///
/// `shards` is every file of a sharded model, `file_path` among them, when
/// the caller knows them; otherwise a first file whose header counts more
/// than one split (`split.count`) has its others found by llama.cpp's own
/// naming, `<prefix>-00002-of-00003.gguf`, which is how llama.cpp finds them
/// to load. The bits per weight are of every shard or none: a shard that
/// cannot be named or read, or whose tensor data cannot be measured, leaves
/// them unknown rather than measuring part of the model.
#[must_use]
pub fn header_facts(
    file_path: &Path,
    gguf: Option<&GgufMetadata>,
    shards: Option<&[PathBuf]>,
    parser: &dyn GgufParserPort,
) -> HeaderFacts {
    let found;
    let shards = match shards {
        Some(shards) => Some(shards),
        None => match gguf.and_then(split_count) {
            // A split whose others cannot be named is not measured in part.
            Some(count) => match split_siblings(file_path, count) {
                Some(siblings) => {
                    found = siblings;
                    Some(found.as_slice())
                }
                None => {
                    return HeaderFacts {
                        file_type: gguf.and_then(|g| g.file_type),
                        bits_per_weight: None,
                    };
                }
            },
            None => None,
        },
    };
    let payload = match shards {
        Some(shards) if shards.len() > 1 => {
            shards
                .iter()
                .try_fold(TensorPayload::default(), |sum, shard| {
                    let payload = if shard == file_path {
                        gguf?.payload?
                    } else {
                        parser.parse(shard).ok()?.payload?
                    };
                    Some(sum + payload)
                })
        }
        _ => gguf.and_then(|g| g.payload),
    };
    HeaderFacts {
        file_type: gguf.and_then(|g| g.file_type),
        bits_per_weight: payload.and_then(TensorPayload::bits_per_weight),
    }
}

/// The facts the file at `path` states, when it is there and states either:
/// [`header_facts`] over its parse, its shards found by llama.cpp's naming.
#[must_use]
pub fn read_header_facts(path: &Path, parser: &dyn GgufParserPort) -> Option<HeaderFacts> {
    if !path.is_file() {
        return None;
    }
    let gguf = parser.parse(path).ok()?;
    let facts = header_facts(path, Some(&gguf), None, parser);
    (!facts.is_empty()).then_some(facts)
}

/// Fill whichever of `model`'s type and bits per weight it lacks from its
/// file's header, as [`read_header_facts`] reads them, and answer whether
/// either was filled.
///
/// A fact already stored is never changed, and a file that is missing or
/// unreadable leaves the model as it was: a model the start-up pass missed,
/// its drive unmounted then, is filled by the first retag that finds it.
pub(super) fn fill_missing_facts(model: &mut Model, parser: &dyn GgufParserPort) -> bool {
    if model.file_type.is_some() && model.bits_per_weight.is_some() {
        return false;
    }
    let Some(facts) = read_header_facts(&model.file_path, parser) else {
        return false;
    };
    let type_read = model.file_type.is_none() && facts.file_type.is_some();
    if type_read {
        model.file_type = facts.file_type;
    }
    let bits_read = model.bits_per_weight.is_none() && facts.bits_per_weight.is_some();
    if bits_read {
        model.bits_per_weight = facts.bits_per_weight;
    }
    type_read || bits_read
}

/// How many files the header says its model is split into, when more than
/// one.
fn split_count(gguf: &GgufMetadata) -> Option<u32> {
    gguf.metadata
        .get("split.count")?
        .parse()
        .ok()
        .filter(|&count| count > 1)
}

/// Every shard of the `count`-way split whose first file is `file_path`, by
/// llama.cpp's naming, when its name is the first of them.
fn split_siblings(file_path: &Path, count: u32) -> Option<Vec<PathBuf>> {
    let name = file_path.file_name()?.to_str()?;
    let prefix = name.strip_suffix(&format!("-00001-of-{count:05}.gguf"))?;
    let dir = file_path.parent()?;
    Some(
        (1..=count)
            .map(|index| dir.join(format!("{prefix}-{index:05}-of-{count:05}.gguf")))
            .collect(),
    )
}

#[cfg(test)]
#[path = "model_header_facts_tests.rs"]
mod tests;
