//! Command arguments for the update command.

/// Arguments for the update command
pub struct UpdateArgs {
    pub id: u32,
    pub name: Option<String>,
    pub param_count: Option<f64>,
    pub architecture: Option<String>,
    pub quantization: Option<String>,
    pub context_length: Option<u64>,
    pub metadata: Vec<String>,
    pub remove_metadata: Option<String>,
    pub replace_metadata: bool,
    pub dry_run: bool,
    pub force: bool,
}
