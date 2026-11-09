//! A file check's verdict and the words it is read in.
//!
//! [`OverallHealth::of`] is the one rule turning each file's health into the
//! model's, and [`OverallHealth::word`] and [`ShardHealth::word`] the one
//! place their words are written: `gglib model verify` prints them, the
//! state-words test records them in `contracts/vocabulary/words.json`, and
//! the GUI's word tables are held to that file.

use super::model_verification::{OverallHealth, ShardHealth};

impl OverallHealth {
    /// Every verdict, in display order.
    pub const ALL: [Self; 3] = [Self::Healthy, Self::Unhealthy, Self::Unverifiable];

    /// The verdict on a model whose files are `files`: unhealthy when any
    /// is corrupt or missing; else healthy when at least one was compared
    /// with its hash and matched; else (no file has a hash, or there are no
    /// files) unverifiable, since nothing was compared.
    ///
    /// A mix of matching files and files with no hash is healthy: every file
    /// that could be compared matched, and the files with no hash are named
    /// as such in the per-file list. Calling it unverifiable would let one
    /// file uploaded without a hash hide that the rest were checked.
    #[must_use]
    pub fn of<'a>(files: impl IntoIterator<Item = &'a ShardHealth>) -> Self {
        let mut matched = false;
        for health in files {
            match health {
                ShardHealth::Corrupt { .. } | ShardHealth::Missing => return Self::Unhealthy,
                ShardHealth::Healthy => matched = true,
                ShardHealth::NoOid => {}
            }
        }
        if matched {
            Self::Healthy
        } else {
            Self::Unverifiable
        }
    }

    /// The word a person reads.
    #[must_use]
    pub const fn word(&self) -> &'static str {
        match self {
            Self::Healthy => "Healthy",
            Self::Unhealthy => "Unhealthy",
            Self::Unverifiable => "Unverifiable",
        }
    }
}

impl ShardHealth {
    /// One of each health, in display order; the corrupt one's hashes are
    /// empty.
    #[must_use]
    pub const fn all() -> [Self; 4] {
        [
            Self::Healthy,
            Self::Corrupt {
                expected: String::new(),
                actual: String::new(),
            },
            Self::Missing,
            Self::NoOid,
        ]
    }

    /// The word a person reads.
    #[must_use]
    pub const fn word(&self) -> &'static str {
        match self {
            Self::Healthy => "Healthy",
            Self::Corrupt { .. } => "Corrupt",
            Self::Missing => "Missing",
            Self::NoOid => "No OID available",
        }
    }
}
