//! A file check's verdict and the words it is read in.
//!
//! [`OverallHealth::of`] is the one rule turning each file's health into the
//! model's, [`ShardHealth::is_damaged`] the one rule for which files a
//! repair fetches again, and [`OverallHealth::word`], [`ShardHealth::word`]
//! and [`damaged_line`] the one place their words are written: `gglib model
//! verify` prints them, the state-words test records them in
//! `contracts/vocabulary/words.json`, and the GUI's word tables are held to
//! that file. The damaged line is served on the report, in [`Damage`].

use super::model_verification::{Damage, OverallHealth, ShardHealth, ShardHealthReport};

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
            if health.is_damaged() {
                return Self::Unhealthy;
            }
            matched |= matches!(health, ShardHealth::Healthy);
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
    /// Whether the file is corrupt or missing: one a repair fetches again.
    /// A file with no hash is not.
    #[must_use]
    pub const fn is_damaged(&self) -> bool {
        matches!(self, Self::Corrupt { .. } | Self::Missing)
    }

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

impl Damage {
    /// The damaged files among `shards`, or `None` when none is.
    #[must_use]
    pub fn of(shards: &[ShardHealthReport]) -> Option<Self> {
        let files: Vec<usize> = shards
            .iter()
            .filter(|s| s.health.is_damaged())
            .map(|s| s.index)
            .collect();
        let line = damaged_line(files.len())?;
        Some(Self { files, line })
    }
}

/// How many files a check found damaged, as a person reads it: `1 file
/// damaged`, `2 files damaged`; `None` for none.
#[must_use]
pub fn damaged_line(count: usize) -> Option<String> {
    match count {
        0 => None,
        1 => Some("1 file damaged".to_owned()),
        n => Some(format!("{n} files damaged")),
    }
}
