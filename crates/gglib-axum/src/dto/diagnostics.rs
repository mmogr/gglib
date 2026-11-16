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
use gglib_core::utils::system::{Dependency, DependencyGroup, DependencyReport, DependencyStatus};
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
    /// The state in the words `check-deps` prints: `v3.28.1`, `available`,
    /// `missing`, `not installed`, `optional`.
    pub status_word: String,
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
            status_word: dep.status_word(),
            description: dep.description.clone(),
            required: dep.required,
            install_hint: dep.install_hint.clone(),
        }
    }
}

/// The dependencies that serve one purpose, as `gglib config check-deps`
/// prints them: its heading, what it says about this gglib, and its rows.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub(crate) struct DependencyGroupDto {
    pub title: String,
    pub explanation: String,
    /// Whether this gglib needs the group. A member of a group it does not
    /// need is never `required`.
    pub needed: bool,
    pub dependencies: Vec<DependencyDto>,
}

impl From<&DependencyGroup> for DependencyGroupDto {
    fn from(group: &DependencyGroup) -> Self {
        Self {
            title: group.title.clone(),
            explanation: group.explanation.clone(),
            needed: group.needed,
            dependencies: group.dependencies.iter().map(Into::into).collect(),
        }
    }
}

/// The dependency report: running gglib, building it from source, building
/// an engine from source, and the conclusion `check-deps` prints.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub(crate) struct DependencyReportDto {
    pub groups: Vec<DependencyGroupDto>,
    /// The conclusion, in one sentence.
    pub verdict: String,
    /// Whether nothing this gglib needs is missing.
    pub complete: bool,
}

impl From<&DependencyReport> for DependencyReportDto {
    fn from(report: &DependencyReport) -> Self {
        Self {
            groups: report.groups.iter().map(Into::into).collect(),
            verdict: report.verdict(),
            complete: report.missing().is_empty(),
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
    pub sd_server_path: String,
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
            sd_server_path: paths.sd_server_path.display().to_string(),
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
    pub dependencies: DependencyReportDto,
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
    /// That source in the words `gglib up` prints it in: "VRAM", "unified
    /// memory", "system RAM".
    pub budget_label: String,
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
            rationale: rec.candidate.rationale.to_string(),
            required_bytes: rec.candidate.required_bytes(),
            budget_bytes: rec.budget_bytes,
            budget_source: match rec.budget_source {
                BudgetSource::Vram => "vram",
                BudgetSource::UnifiedMemory => "unifiedMemory",
                BudgetSource::SystemRam => "systemRam",
            }
            .to_string(),
            budget_label: rec.budget_source.label().to_string(),
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

    /// The app reads the report's groups and conclusion as `check-deps`
    /// prints them, and an installed copy's build tools go out unrequired.
    #[test]
    fn the_report_goes_out_with_the_words_check_deps_prints() {
        use gglib_core::host::HostOs;
        use gglib_core::utils::system::{
            BuildOrigin, EngineInstall, Found, LinuxDistro, dependency_report,
        };

        let report = dependency_report(
            HostOs::Windows,
            BuildOrigin::Installed,
            LinuxDistro::Unknown,
            &Found {
                llama_install: EngineInstall::Download,
                ..Found::default()
            },
        );
        let json = serde_json::to_value(DependencyReportDto::from(&report)).unwrap();

        assert_eq!(json["verdict"], report.verdict());
        assert_eq!(json["complete"], true);
        let groups = json["groups"].as_array().unwrap();
        assert_eq!(groups.len(), report.groups.len());
        for (wire, group) in groups.iter().zip(&report.groups) {
            assert_eq!(wire["title"], group.title.as_str());
            assert_eq!(wire["explanation"], group.explanation.as_str());
            assert_eq!(wire["needed"], group.needed);
        }
        let npm = groups
            .iter()
            .flat_map(|group| group["dependencies"].as_array().unwrap())
            .find(|dep| dep["name"] == "npm")
            .unwrap();
        assert_eq!(npm["required"], false);
        assert_eq!(npm["status"], "missing");
        assert_eq!(npm["statusWord"], "not installed");
    }

    #[test]
    fn recommendation_names_its_budget_as_the_cli_prints_it() {
        use gglib_core::domain::recommendation::{BudgetSource, recommend};
        use gglib_core::utils::system::SystemMemoryInfo;

        let machines = [
            (
                Some(48 << 30),
                true,
                BudgetSource::UnifiedMemory,
                "unifiedMemory",
            ),
            (Some(48 << 30), false, BudgetSource::Vram, "vram"),
            (None, false, BudgetSource::SystemRam, "systemRam"),
        ];
        for (gpu, unified, source, key) in machines {
            let mem = SystemMemoryInfo {
                total_ram_bytes: 64 << 30,
                gpu_memory_bytes: gpu,
                is_unified_memory: unified,
                has_nvidia_gpu: gpu.is_some() && !unified,
            };
            let rec = recommend(&mem).expect("48 GiB fits the shortlist");
            assert_eq!(rec.budget_source, source);

            let json = serde_json::to_value(RecommendationDto::from(rec)).unwrap();
            assert_eq!(json["budgetSource"], key);
            assert_eq!(json["budgetLabel"], source.label());
        }
    }
}
