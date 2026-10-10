//! What a pre-built download records about itself.

use chrono::Utc;
use serde::{Deserialize, Serialize};

/// What a pre-built download records about itself.
///
/// The same four keys for every product, written into that product's own
/// record file (llama.cpp's is `llama-config.json`), where a source build
/// writes a shape the product defines for itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PrebuiltRecord {
    /// The release tag that was installed (e.g. `b10327`).
    pub(crate) version: String,
    /// The platform build that was chosen (e.g. `macOS ARM64 (Metal)`).
    pub(crate) platform: String,
    /// Always `prebuilt`.
    pub(crate) install_type: String,
    /// When it was installed, as RFC 3339.
    pub(crate) installed_at: String,
}

impl PrebuiltRecord {
    /// The record of a download made now.
    pub(crate) fn new(version: &str, platform: &str) -> Self {
        Self {
            version: version.to_string(),
            platform: platform.to_string(),
            install_type: "prebuilt".to_string(),
            installed_at: Utc::now().to_rfc3339(),
        }
    }
}
