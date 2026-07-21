//! Slot cache path helpers.
//!
//! Shared utilities for constructing per-model KV cache slot filenames,
//! used by both gglib-runtime (purge) and gglib-proxy (save/restore).
//!
//! ## Why a flat `{model_id}__{session}.bin` name, not a subdirectory
//!
//! llama-server's `/slots/{id}?action=save|restore` endpoint validates the
//! `filename` field with `fs_validate_filename`, which rejects any path
//! separator ("Invalid filename", HTTP 400) as a path-traversal defense. A
//! `{model_id}/{session}.bin` name (an earlier subdirectory-based layout) is
//! therefore rejected outright, silently breaking every save/restore. Encoding
//! the model id as a filename *prefix* instead keeps per-model scoping (purge
//! removes only one model's files, matched by prefix) while sending llama-server
//! a separator-free name it accepts.

use std::path::{Path, PathBuf};

/// Filename for a model+session slot cache file: `{model_id}__{session_id}.bin`.
///
/// This is both the on-disk name (directly under `slot_dir`) and the
/// `filename` value sent to llama-server's save/restore endpoint — they must
/// match, and both must be free of path separators (see the module docs).
pub fn slot_file_name(model_id: u32, session_id: &str) -> String {
    format!("{model_id}__{session_id}.bin")
}

/// Full on-disk path for a model+session slot cache file (flat under `slot_dir`).
pub fn slot_bin_path(slot_dir: &Path, model_id: u32, session_id: &str) -> PathBuf {
    slot_dir.join(slot_file_name(model_id, session_id))
}

/// Filename prefix identifying all of one model's slot files: `{model_id}__`.
///
/// Used by purge to remove only the swapped-out model's files. The trailing
/// `__` delimiter is load-bearing: it prevents model `1`'s prefix (`1__`) from
/// matching model `11`'s files (`11__…`).
pub fn slot_model_prefix(model_id: u32) -> String {
    format!("{model_id}__")
}

/// Filename for an in-flight save: `{model_id}__{session_id}.{nonce}.tmp`.
///
/// llama-server is asked to write here instead of directly to the final
/// `.bin` name, so a save that times out or is retried while the server is
/// still writing can never produce a torn file at the name restore/eviction
/// actually read — those only ever see `*.bin` (see [`slot_file_name`]). The
/// caller renames this to the final name only after a confirmed-complete
/// write; `nonce` (a per-attempt counter) keeps concurrent attempts for the
/// same session from writing the same temp file.
pub fn slot_tmp_file_name(model_id: u32, session_id: &str, nonce: u64) -> String {
    format!("{model_id}__{session_id}.{nonce}.tmp")
}

/// Recover the session id from a slot file stem (`{model_id}__{session}`).
///
/// Splits on the **first** `__`. Model ids are numeric and contain no `__`, so
/// the first `__` is always the model/session delimiter — this correctly
/// recovers the session even when the session id itself contains `__`.
/// Returns `None` for a stem with no `__` (e.g. a legacy pre-namespacing file),
/// which callers treat as "not one of our namespaced files".
pub fn slot_session_from_stem(stem: &str) -> Option<&str> {
    stem.split_once("__").map(|(_, session)| session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_file_name_encodes_model_and_session() {
        assert_eq!(slot_file_name(42, "planner"), "42__planner.bin");
    }

    #[test]
    fn slot_bin_path_is_flat_under_slot_dir() {
        let base = Path::new("/tmp/slots");
        assert_eq!(
            slot_bin_path(base, 7, "abc"),
            PathBuf::from("/tmp/slots/7__abc.bin")
        );
    }

    #[test]
    fn slot_model_prefix_includes_delimiter() {
        assert_eq!(slot_model_prefix(1), "1__");
        // The delimiter guards against 1__ matching 11__.
        assert!(!"11__x.bin".starts_with(&slot_model_prefix(1)));
        assert!("1__x.bin".starts_with(&slot_model_prefix(1)));
    }

    #[test]
    fn slot_session_from_stem_recovers_session() {
        assert_eq!(slot_session_from_stem("42__planner"), Some("planner"));
    }

    #[test]
    fn slot_session_from_stem_handles_session_with_double_underscore() {
        // Session ids may themselves contain `__`; only the first split counts.
        assert_eq!(slot_session_from_stem("3__a__b"), Some("a__b"));
    }

    #[test]
    fn slot_session_from_stem_none_for_legacy_flat_name() {
        assert_eq!(slot_session_from_stem("auto-deadbeef"), None);
    }

    #[test]
    fn slot_tmp_file_name_encodes_model_session_and_nonce() {
        assert_eq!(slot_tmp_file_name(42, "planner", 7), "42__planner.7.tmp");
    }

    #[test]
    fn slot_tmp_file_name_never_has_bin_extension() {
        let name = slot_tmp_file_name(1, "s", 0);
        assert_eq!(
            Path::new(&name).extension(),
            Some(std::ffi::OsStr::new("tmp"))
        );
    }

    #[test]
    fn slot_tmp_file_name_is_unique_per_nonce() {
        assert_ne!(slot_tmp_file_name(1, "s", 0), slot_tmp_file_name(1, "s", 1));
    }
}
