//! Finding a repository's projectors by name.
//!
//! A repository lists a projector as one more GGUF, and its name carries a
//! quantization like any weights file's (`mmproj-F16.gguf`). The projectors a
//! download may fetch beside a model's weights are the files
//! [`GgufFileRole::classify`] names as one. A projector is also a variant of
//! its own in the listing, never a shard of another: the variants are
//! grouped by name alone, and what each one is comes from its header.

use std::path::Path;

use gglib_core::download::GgufFileRole;
use gglib_core::ports::huggingface::HfFileInfo;

use crate::models::HfFileEntry;

impl HfFileEntry {
    /// Whether this is a GGUF file holding a projector.
    pub(crate) fn is_projector(&self) -> bool {
        self.is_gguf() && GgufFileRole::classify(Path::new(&self.path)).is_projector()
    }
}

/// The projectors among `files`, by path.
pub(crate) fn projectors_among(files: &[HfFileEntry]) -> Vec<HfFileEntry> {
    let mut projectors: Vec<HfFileEntry> = files
        .iter()
        .filter(|file| file.is_projector())
        .cloned()
        .collect();
    projectors.sort_by(|a, b| a.path.cmp(&b.path));
    projectors
}

/// The port's view of a repository file, with its OID.
pub(crate) fn to_file_info(entry: HfFileEntry) -> HfFileInfo {
    HfFileInfo {
        is_gguf: entry.is_gguf(),
        path: entry.path,
        size: entry.size,
        oid: entry.oid,
    }
}

#[cfg(test)]
#[path = "file_roles_tests.rs"]
mod tests;
