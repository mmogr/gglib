//! A repository file as the port sees it.
//!
//! A repository lists a projector as one more GGUF, a variant of its own,
//! never a shard of another: the variants are grouped by name alone, and
//! what each one is, a projector among them, comes from its header.

use gglib_core::ports::huggingface::HfFileInfo;

use crate::models::HfFileEntry;

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
