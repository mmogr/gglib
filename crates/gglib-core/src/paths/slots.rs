//! Slot cache path helpers.
//!
//! Shared utilities for constructing per-model KV cache slot paths,
//! used by both gglib-runtime (purge) and gglib-proxy (save/restore).

use std::path::{Path, PathBuf};

/// Return the per-model subdirectory path `{slot_dir}/{model_id}/`
/// used by both gglib-runtime (purge) and gglib-proxy (save/restore).
pub fn slot_model_dir(slot_dir: &Path, model_id: u32) -> PathBuf {
    slot_dir.join(model_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_model_dir_appends_model_id() {
        let base = Path::new("/tmp/slots");
        let result = slot_model_dir(base, 42);
        assert_eq!(result, PathBuf::from("/tmp/slots/42"));
    }

    #[test]
    fn slot_model_dir_with_nested_path() {
        let base = Path::new("/home/user/.local/share/gglib/cache");
        let result = slot_model_dir(base, 7);
        assert_eq!(
            result,
            PathBuf::from("/home/user/.local/share/gglib/cache/7")
        );
    }
}
