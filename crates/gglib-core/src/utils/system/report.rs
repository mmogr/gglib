//! The dependency report, as a pure function of the OS, the kind of gglib
//! asking and what was found on the machine.
//!
//! `gglib config check-deps` prints it and the desktop app's diagnostics show
//! it; both read the same groups, titles and sentences from here. The probe
//! that fills [`Found`] is `gglib_runtime::system::DefaultSystemProbe`.

use std::collections::BTreeMap;

use super::needs::{HostOs, Need, Purpose, Source, needs_on};
use super::packages::{LinuxDistro, install_hint, packages_for};
use super::types::{Dependency, DependencyStatus, GpuInfo};

/// What kind of gglib is asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildOrigin {
    /// A copy someone installed: a package, the installer, a download.
    Installed,
    /// A build from a source checkout that is still on this machine.
    Checkout,
}

/// What a probe found on the machine.
#[derive(Debug, Clone, Default)]
pub struct Found {
    /// The version of each row of the table that was found, by its name.
    pub versions: BTreeMap<&'static str, String>,
    pub gpu: GpuInfo,
    /// Whether llama.cpp's `llama-server` is installed.
    pub llama_installed: bool,
    /// Whether stable-diffusion.cpp's `sd-server` is installed.
    pub sd_installed: bool,
    /// Whether this machine has a pre-built llama.cpp to download.
    pub llama_download: bool,
}

/// The dependencies that serve one purpose.
#[derive(Debug, Clone)]
pub struct DependencyGroup {
    pub purpose: Purpose,
    /// The group's heading.
    pub title: String,
    /// What the group is for, and whether this gglib needs it.
    pub explanation: String,
    /// Whether this gglib needs the group: a missing required member of a
    /// group it needs is a failed check.
    pub needed: bool,
    pub dependencies: Vec<Dependency>,
}

/// The whole report.
#[derive(Debug, Clone)]
pub struct DependencyReport {
    pub origin: BuildOrigin,
    pub groups: Vec<DependencyGroup>,
}

impl DependencyReport {
    /// Every dependency, group by group.
    pub fn dependencies(&self) -> impl Iterator<Item = &Dependency> {
        self.groups.iter().flat_map(|group| &group.dependencies)
    }

    /// The required dependencies that are missing.
    #[must_use]
    pub fn missing(&self) -> Vec<&Dependency> {
        self.dependencies()
            .filter(|dep| dep.required && dep.status == DependencyStatus::Missing)
            .collect()
    }

    /// The report's conclusion, in one sentence.
    #[must_use]
    pub fn verdict(&self) -> String {
        let missing = self.missing().len();
        if missing == 0 {
            "All required dependencies are installed!".to_owned()
        } else {
            format!("{missing} required dependencies are missing.")
        }
    }
}

/// The report for a gglib of `origin` on `os`, from what `found` holds.
/// `distro` names the install commands on Linux.
#[must_use]
pub fn dependency_report(
    os: HostOs,
    origin: BuildOrigin,
    distro: LinuxDistro,
    found: &Found,
) -> DependencyReport {
    let groups = [Purpose::Run, Purpose::BuildGglib, Purpose::BuildEngine]
        .into_iter()
        .map(|purpose| {
            let mut dependencies: Vec<Dependency> = needs_on(os)
                .filter(|need| need.purpose == purpose)
                .map(|need| row(need, distro, found))
                .collect();
            if purpose == Purpose::Run {
                dependencies.extend(gpu_rows(os, distro, &found.gpu));
            }
            DependencyGroup {
                purpose,
                title: String::new(),
                explanation: String::new(),
                needed: true,
                dependencies,
            }
        })
        .collect();

    DependencyReport { origin, groups }
}

/// One row of the table, as found.
fn row(need: &Need, distro: LinuxDistro, found: &Found) -> Dependency {
    let base = if need.required {
        Dependency::required(need.name, need.description)
    } else {
        Dependency::optional(need.name, need.description)
    };
    let hint = match need.source {
        Source::Url(url) => url.to_owned(),
        Source::Package => hint_for(need.name, distro),
    };
    base.with_hint(hint)
        .with_status(status(found.versions.get(need.name)))
}

fn status(version: Option<&String>) -> DependencyStatus {
    version.map_or(DependencyStatus::Missing, |version| {
        DependencyStatus::Present {
            version: version.clone(),
        }
    })
}

/// What the GPU found means for running gglib.
fn gpu_rows(os: HostOs, distro: LinuxDistro, gpu: &GpuInfo) -> Vec<Dependency> {
    let available = || DependencyStatus::Present {
        version: "available".to_owned(),
    };
    let mut rows = Vec::new();

    if gpu.has_metal {
        rows.push(
            Dependency::optional("Metal", "Apple GPU acceleration (built-in)")
                .with_status(available()),
        );
    } else if let Some(cuda_version) = gpu.cuda_version.clone() {
        rows.push(
            Dependency::optional("CUDA", "NVIDIA GPU acceleration for faster inference")
                .with_hint("https://developer.nvidia.com/cuda-downloads")
                .with_status(DependencyStatus::Present {
                    version: cuda_version,
                }),
        );
    } else if gpu.has_nvidia_gpu {
        rows.push(
            Dependency::optional(
                "CUDA",
                "NVIDIA GPU detected - install CUDA for GPU acceleration",
            )
            .with_hint("https://developer.nvidia.com/cuda-downloads")
            .with_status(DependencyStatus::Optional),
        );
    }

    if gpu.has_vulkan {
        rows.push(
            Dependency::optional(
                "Vulkan runtime",
                "GPU acceleration via Vulkan (AMD, Intel, NVIDIA)",
            )
            .with_status(available()),
        );
        for (name, description, present) in [
            (
                "Vulkan headers",
                "Development headers required to build with -DGGML_VULKAN=ON",
                gpu.vulkan_headers,
            ),
            (
                "glslc",
                "SPIR-V shader compiler required for Vulkan builds",
                gpu.vulkan_glslc,
            ),
            (
                "SPIR-V headers",
                "spirv/unified1/spirv.hpp required to build with -DGGML_VULKAN=ON",
                gpu.vulkan_spirv_headers,
            ),
        ] {
            rows.push(
                Dependency::required(name, description)
                    .with_hint(hint_for(name, distro))
                    .with_status(if present {
                        available()
                    } else {
                        DependencyStatus::Missing
                    }),
            );
        }
    } else if !gpu.has_metal && os != HostOs::MacOs {
        rows.push(
            Dependency::optional("Vulkan", "Install Vulkan drivers for GPU acceleration")
                .with_hint(hint_for("Vulkan", distro))
                .with_status(DependencyStatus::Optional),
        );
    }

    rows
}

/// The install hint for a dependency on `distro`.
///
/// Falls back to prose when there is no command to give: an unidentified
/// distribution, or a dependency that does not come from the system package
/// manager at all. Prose is preferred over a plausible-looking guess: a wrong
/// command gets run and fails, whereas "OpenSSL development headers" gets
/// looked up.
#[must_use]
pub fn hint_for(dependency: &str, distro: LinuxDistro) -> String {
    install_hint(dependency, distro).unwrap_or_else(|| {
        packages_for(dependency).map_or_else(
            || format!("install {dependency}"),
            |names| format!("install {}", names.generic),
        )
    })
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
