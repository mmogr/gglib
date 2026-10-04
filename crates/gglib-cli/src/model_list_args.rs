//! The sort and filter flags of `gglib model list`.

use clap::Args;

use crate::model_sort::{CliModelSortBy, CliSortOrder};

/// How `gglib model list` sorts and filters this machine's catalogue.
#[derive(Args, Debug, Clone)]
pub struct ListArgs {
    /// Field to sort by.
    #[arg(long, value_enum, default_value = "added")]
    pub sort: CliModelSortBy,
    /// Sort direction.
    #[arg(long, value_enum, default_value = "desc")]
    pub order: CliSortOrder,
    /// Only show models with at least this many parameters (in billions).
    #[arg(long)]
    pub min_params: Option<f64>,
    /// Only show models with at most this many parameters (in billions).
    #[arg(long)]
    pub max_params: Option<f64>,
    /// Only include models whose `latest_tg_tps` >= this value (t/s).
    /// Models with no benchmark data are excluded.
    #[arg(long)]
    pub min_speed: Option<f64>,
    /// Only include models whose `latest_tg_tps` <= this value (t/s).
    /// Models with no benchmark data are excluded.
    #[arg(long)]
    pub max_speed: Option<f64>,
    /// Only show models that have this tag (repeatable: AND semantics).
    #[arg(long = "tag", action = clap::ArgAction::Append)]
    pub tags: Vec<String>,
}
