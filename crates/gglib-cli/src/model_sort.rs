//! CLI-friendly sort types for `gglib model list`, `search` and `browse`.
//!
//! `ValueEnum` mirrors of the domain's [`ModelSortBy`] and [`SortOrder`], and
//! of the Hub's [`HfSortField`], kept beside the commands they parameterise
//! rather than inside them: they are self-contained enums with their own
//! conversions, and [`model_commands`](super::model_commands) is about the
//! command surface.

use clap::ValueEnum;
use gglib_core::domain::{ModelSortBy, SortOrder};
use gglib_core::ports::HfSortField;

/// Sort field for `gglib model list`.
///
/// Each variant maps to the corresponding [`ModelSortBy`] domain value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum CliModelSortBy {
    /// Sort by date added (most recent first by default).
    #[default]
    Added,
    /// Sort alphabetically by model name.
    Name,
    /// Sort by parameter count in billions.
    Params,
    /// Sort by latest token-generation throughput (t/s) from benchmarks.
    /// Models without benchmark data sort last.
    Speed,
}

impl From<CliModelSortBy> for ModelSortBy {
    fn from(v: CliModelSortBy) -> Self {
        match v {
            CliModelSortBy::Added => Self::AddedAt,
            CliModelSortBy::Name => Self::Name,
            CliModelSortBy::Params => Self::ParamCount,
            CliModelSortBy::Speed => Self::LatestTgTps,
        }
    }
}

/// Sort direction for `gglib model list`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum CliSortOrder {
    /// Largest / most-recent first.
    #[default]
    Desc,
    /// Smallest / oldest first.
    Asc,
}

impl From<CliSortOrder> for SortOrder {
    fn from(v: CliSortOrder) -> Self {
        match v {
            CliSortOrder::Asc => Self::Asc,
            CliSortOrder::Desc => Self::Desc,
        }
    }
}

/// Sort order for `gglib model search`.
///
/// Each value is one order the Hub sorts by, and clap refuses any other, so
/// no value is accepted and then searched in another's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum CliHubSort {
    /// Most downloaded first.
    #[default]
    Downloads,
    /// Most liked first.
    Likes,
    /// Newest first.
    Created,
    /// Most recently changed first.
    #[value(alias = "modified")]
    Updated,
}

impl From<CliHubSort> for HfSortField {
    fn from(v: CliHubSort) -> Self {
        match v {
            CliHubSort::Downloads => Self::Downloads,
            CliHubSort::Likes => Self::Likes,
            CliHubSort::Created => Self::Created,
            CliHubSort::Updated => Self::Modified,
        }
    }
}

/// Category for `gglib model browse`: one order of the Hub's GGUF models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum CliBrowseCategory {
    /// The most downloaded models.
    #[default]
    Popular,
    /// The newest models.
    Recent,
}

impl CliBrowseCategory {
    /// The category as it is typed, for the lines that name it.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Popular => "popular",
            Self::Recent => "recent",
        }
    }
}

impl From<CliBrowseCategory> for HfSortField {
    fn from(v: CliBrowseCategory) -> Self {
        match v {
            CliBrowseCategory::Popular => Self::Downloads,
            CliBrowseCategory::Recent => Self::Created,
        }
    }
}
