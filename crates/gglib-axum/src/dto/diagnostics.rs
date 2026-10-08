//! Wire types for the system diagnostics surface.
//!
//! `gglib config check-deps` and `paths` print directly from types that live
//! in `gglib-core` and carry no `Serialize`, with an enum or a path where the
//! wire has a string. Rather than derive serde onto domain types for the
//! benefit of one HTTP route, this reshapes them here. The acceleration and
//! fast-downloads halves need no reshaping: they go out as the service's own
//! types.

use gglib_app_services::setup::{AccelerationInfo, FastDownloadsInfo};
use gglib_core::paths::{ModelsDirSource, ResolvedPaths};
use gglib_core::utils::system::{Dependency, DependencyStatus};
use serde::Serialize;

/// One dependency's state.
///
/// `status` is flattened to a string plus an optional version rather than an
/// externally-tagged enum: the GUI switches on three cases and the extra
/// nesting buys nothing.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub(crate) struct DependencyDto {
    pub name: String,
    /// `"present" | "missing" | "optional"`.
    pub status: String,
    /// Present only when `status` is `"present"`.
    pub version: Option<String>,
    pub description: String,
    pub required: bool,
    pub install_hint: Option<String>,
}

impl From<&Dependency> for DependencyDto {
    fn from(dep: &Dependency) -> Self {
        let (status, version) = match &dep.status {
            DependencyStatus::Present { version } => ("present", Some(version.clone())),
            DependencyStatus::Missing => ("missing", None),
            DependencyStatus::Optional => ("optional", None),
        };

        Self {
            name: dep.name.clone(),
            status: status.to_string(),
            version,
            description: dep.description.clone(),
            required: dep.required,
            install_hint: dep.install_hint.clone(),
        }
    }
}

/// Where everything resolves to — `gglib config paths`, the golden-truth tool
/// for "which directory is it actually using?".
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResolvedPathsDto {
    pub data_root: String,
    pub resource_root: String,
    pub database_path: String,
    pub llama_server_path: String,
    pub models_dir: String,
    /// How the models directory was chosen: `"explicit" | "envVar" | "default"`.
    pub models_source: String,
}

impl From<ResolvedPaths> for ResolvedPathsDto {
    fn from(paths: ResolvedPaths) -> Self {
        Self {
            data_root: paths.data_root.display().to_string(),
            resource_root: paths.resource_root.display().to_string(),
            database_path: paths.database_path.display().to_string(),
            llama_server_path: paths.llama_server_path.display().to_string(),
            models_dir: paths.models_dir.display().to_string(),
            models_source: match paths.models_source {
                ModelsDirSource::Explicit => "explicit",
                ModelsDirSource::EnvVar => "envVar",
                ModelsDirSource::Default => "default",
            }
            .to_string(),
        }
    }
}

/// Everything the diagnostics panel shows, in one request.
///
/// One route rather than four because these are read together: the panel is
/// "why isn't this working", and a user comparing a missing dependency
/// against a resolved path should not be watching four spinners.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiagnosticsDto {
    pub dependencies: Vec<DependencyDto>,
    pub paths: ResolvedPathsDto,
    pub acceleration: AccelerationInfo,
    pub fast_downloads: FastDownloadsInfo,
}

/// A hardware-sized model suggestion — what `gglib up` picks on a first run.
///
/// `null` from the route rather than an empty object when nothing fits: "no
/// model in the shortlist fits this machine" is a real answer and the UI has
/// to say so, not show a recommendation card with blanks in it.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecommendationDto {
    pub repo: String,
    pub quantization: String,
    /// Why this model, in the user's terms.
    pub rationale: String,
    /// Weights plus KV cache at the candidate's context, at the quantization
    /// the runtime actually launches with.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub required_bytes: u64,
    /// The memory figure it was sized against.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub budget_bytes: u64,
    /// Where that figure came from: `"vram" | "unifiedMemory" | "systemRam"`.
    pub budget_source: String,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub headroom_bytes: u64,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub context: u64,
}

impl From<gglib_core::domain::recommendation::Recommendation> for RecommendationDto {
    fn from(rec: gglib_core::domain::recommendation::Recommendation) -> Self {
        use gglib_core::domain::recommendation::BudgetSource;

        Self {
            repo: rec.candidate.repo.to_string(),
            quantization: rec.candidate.quantization.to_string(),
            rationale: rec.candidate.rationale.to_string(),
            required_bytes: rec.candidate.required_bytes(),
            budget_bytes: rec.budget_bytes,
            budget_source: match rec.budget_source {
                BudgetSource::Vram => "vram",
                BudgetSource::UnifiedMemory => "unifiedMemory",
                BudgetSource::SystemRam => "systemRam",
            }
            .to_string(),
            headroom_bytes: rec.headroom_bytes,
            context: rec.candidate.context,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn present_dependency_carries_its_version() {
        let dep = Dependency {
            name: "cmake".into(),
            status: DependencyStatus::Present {
                version: "3.28.1".into(),
            },
            description: "Build system".into(),
            required: true,
            install_hint: Some("brew install cmake".into()),
        };

        let json = serde_json::to_value(DependencyDto::from(&dep)).unwrap();
        assert_eq!(json["status"], "present");
        assert_eq!(json["version"], "3.28.1");
        assert_eq!(json["installHint"], "brew install cmake");
    }

    #[test]
    fn missing_dependency_has_no_version() {
        let dep = Dependency {
            name: "git".into(),
            status: DependencyStatus::Missing,
            description: "Version control".into(),
            required: true,
            install_hint: None,
        };

        let json = serde_json::to_value(DependencyDto::from(&dep)).unwrap();
        assert_eq!(json["status"], "missing");
        assert!(json["version"].is_null());
    }
}
