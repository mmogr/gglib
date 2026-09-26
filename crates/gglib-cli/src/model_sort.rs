//! CLI-friendly sort types for `gglib model list`.
//!
//! `ValueEnum` mirrors of the domain's [`ModelSortBy`] and [`SortOrder`], kept
//! beside the command they parameterise rather than inside it: they are two
//! self-contained enums with their own conversions, and
//! [`model_commands`](super::model_commands) is about the command surface.

use clap::ValueEnum;
use gglib_core::domain::{ModelSortBy, SortOrder};

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

impl CliModelSortBy {
    /// The `snake_case` name expected by the HTTP query parameter `sort=`.
    pub fn api_value(self) -> &'static str {
        match self {
            Self::Added => "added_at",
            Self::Name => "name",
            Self::Params => "param_count",
            Self::Speed => "latest_tg_tps",
        }
    }
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

impl CliSortOrder {
    /// The value expected by the HTTP query parameter `order=`.
    pub fn api_value(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

impl From<CliSortOrder> for SortOrder {
    fn from(v: CliSortOrder) -> Self {
        match v {
            CliSortOrder::Asc => Self::Asc,
            CliSortOrder::Desc => Self::Desc,
        }
    }
}
