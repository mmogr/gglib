//! The dependency report, as a pure function of the OS, the kind of gglib
//! asking and what was found on the machine.
//!
//! `gglib config check-deps` prints it and the desktop app's diagnostics show
//! it; both read the same groups, titles and sentences from here. The probe
//! that fills [`Found`] is `gglib_runtime::system::DefaultSystemProbe`.

use std::collections::BTreeMap;

use super::needs::{Need, Purpose, Source, needs_on};
use super::packages::{LinuxDistro, install_hint, packages_for};
use super::types::{Dependency, DependencyStatus, GpuInfo};
use crate::host::HostOs;
use crate::paths::{LLAMA_INSTALL_COMMAND, SD_INSTALL_COMMAND};

/// What kind of gglib is asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildOrigin {
    /// A copy someone installed: a package, the installer, a download.
    Installed,
    /// A build from a source checkout that is still on this machine.
    Checkout,
}

/// How llama.cpp's next install here would be made, as the engine's own
/// install plan decides it with nobody asking for a build.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EngineInstall {
    /// Downloaded ready-built.
    #[default]
    Download,
    /// Built from source: this machine has no ready-built llama.cpp.
    NoDownload,
    /// Built from source: what is installed was built from source, and an
    /// update builds it again rather than turning it into a download.
    Rebuild,
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
    /// How llama.cpp's next install would be made.
    pub llama_install: EngineInstall,
}

/// The dependencies that serve one purpose.
#[derive(Debug, Clone)]
pub struct DependencyGroup {
    pub purpose: Purpose,
    /// The group's heading.
    pub title: String,
    /// What the group is for, and whether this gglib needs it.
    pub explanation: String,
    /// Whether this gglib needs the group. Only a needed group's members can
    /// be required, so only a needed group can fail the check.
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

    /// The required dependencies that are missing: what this gglib needs
    /// and does not have.
    #[must_use]
    pub fn missing(&self) -> Vec<&Dependency> {
        self.dependencies()
            .filter(|dep| dep.required && dep.status == DependencyStatus::Missing)
            .collect()
    }

    /// Whether the group for `purpose` is needed here.
    #[must_use]
    pub fn needs(&self, purpose: Purpose) -> bool {
        self.groups
            .iter()
            .any(|group| group.purpose == purpose && group.needed)
    }

    /// The report's conclusion, in one sentence.
    #[must_use]
    pub fn verdict(&self) -> String {
        match self.missing().len() {
            0 => "Nothing this gglib needs is missing.".to_owned(),
            1 => "1 dependency this gglib needs is missing.".to_owned(),
            n => format!("{n} dependencies this gglib needs are missing."),
        }
    }
}

/// The report for a gglib of `origin` on `os`, from what `found` holds.
/// `distro` names the install commands on Linux.
///
/// Building gglib is needed by a checkout build alone. Building an engine is
/// needed by a checkout build, and by an installed copy whose next llama.cpp
/// install is a source build: no download for the machine, or an engine
/// already built from source.
#[must_use]
pub fn dependency_report(
    os: HostOs,
    origin: BuildOrigin,
    distro: LinuxDistro,
    found: &Found,
) -> DependencyReport {
    let checkout = origin == BuildOrigin::Checkout;
    let builds_engine = checkout || found.llama_install != EngineInstall::Download;

    let groups = [
        (Purpose::Run, true),
        (Purpose::BuildGglib, checkout),
        (Purpose::BuildEngine, builds_engine),
    ]
    .into_iter()
    .map(|(purpose, needed)| {
        let mut dependencies: Vec<Dependency> = needs_on(os)
            .filter(|need| need.purpose == purpose)
            .map(|need| row(need, needed, distro, found))
            .collect();
        match purpose {
            Purpose::Run => {
                dependencies.splice(0..0, engine_rows(found));
                dependencies.extend(gpu_rows(os, distro, &found.gpu));
            }
            Purpose::BuildEngine => {
                dependencies.extend(vulkan_build_rows(needed, distro, &found.gpu));
            }
            Purpose::BuildGglib => {}
        }
        let (title, explanation) = words(purpose, origin, found.llama_install);
        DependencyGroup {
            purpose,
            title: title.to_owned(),
            explanation: explanation.to_owned(),
            needed,
            dependencies,
        }
    })
    .collect();

    DependencyReport { origin, groups }
}

/// A group's heading and what it says about this gglib.
fn words(
    purpose: Purpose,
    origin: BuildOrigin,
    llama_install: EngineInstall,
) -> (&'static str, &'static str) {
    match (purpose, origin, llama_install) {
        (Purpose::Run, ..) => (
            "Running gglib",
            "What gglib uses as it runs. It installs its engines itself; the rest is optional.",
        ),
        (Purpose::BuildGglib, BuildOrigin::Installed, _) => (
            "Building gglib from source",
            "These build gglib from its source code. An installed gglib needs none of them.",
        ),
        (Purpose::BuildGglib, BuildOrigin::Checkout, _) => (
            "Building gglib from source",
            "This gglib was built from a source checkout, and building it again needs these.",
        ),
        (Purpose::BuildEngine, BuildOrigin::Installed, EngineInstall::Download) => (
            "Building an engine from source",
            "These build llama.cpp or stable-diffusion.cpp from source, which gglib does only \
             when asked to with --build. An installed gglib needs none of them: it downloads \
             llama.cpp ready-built.",
        ),
        (Purpose::BuildEngine, BuildOrigin::Installed, EngineInstall::NoDownload) => (
            "Building an engine from source",
            "There is no ready-built llama.cpp for this machine, so gglib builds it from \
             source, and that needs these.",
        ),
        (Purpose::BuildEngine, BuildOrigin::Installed, EngineInstall::Rebuild) => (
            "Building an engine from source",
            "The llama.cpp here was built from source, so gglib builds its updates from source \
             too, and that needs these.",
        ),
        (Purpose::BuildEngine, BuildOrigin::Checkout, _) => (
            "Building an engine from source",
            "These build llama.cpp or stable-diffusion.cpp from source, which gglib does when \
             asked to with --build, when there is no ready-built engine for this machine, or \
             to update one built from source. A build from a checkout needs them.",
        ),
    }
}

/// One row of the table, as found. It is required only where its purpose is
/// `needed`.
fn row(need: &Need, needed: bool, distro: LinuxDistro, found: &Found) -> Dependency {
    let install_hint = match need.source {
        Source::Url(url) => url.to_owned(),
        Source::Package => hint_for(need.name, distro),
    };
    Dependency {
        name: need.name.to_owned(),
        status: found_or_missing(found.versions.get(need.name).cloned()),
        description: need.description.to_owned(),
        required: need.required && needed,
        install_hint: Some(install_hint),
    }
}

fn found_or_missing(version: Option<String>) -> DependencyStatus {
    version.map_or(DependencyStatus::Missing, |version| {
        DependencyStatus::Present { version }
    })
}

fn present_if(present: bool, word: &str) -> DependencyStatus {
    found_or_missing(present.then(|| word.to_owned()))
}

/// The engines, which gglib installs itself: never required here, so an
/// engine not yet installed does not fail a check that `make setup` runs
/// before it installs them.
fn engine_rows(found: &Found) -> [Dependency; 2] {
    [
        Dependency::optional(
            "llama.cpp",
            "Runs chat models. gglib installs it when a model is first served",
        )
        .with_hint(LLAMA_INSTALL_COMMAND)
        .with_status(present_if(found.llama_installed, "installed")),
        Dependency::optional("stable-diffusion.cpp", "Generates images, if you want that")
            .with_hint(SD_INSTALL_COMMAND)
            .with_status(present_if(found.sd_installed, "installed")),
    ]
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
    } else if !gpu.has_metal && os != HostOs::MacOs {
        rows.push(
            Dependency::optional("Vulkan", "Install Vulkan drivers for GPU acceleration")
                .with_hint(hint_for("Vulkan", distro))
                .with_status(DependencyStatus::Optional),
        );
    }

    rows
}

/// What a Vulkan build of an engine needs, where there is a Vulkan GPU: a
/// build from source picks Vulkan there, and fails without these.
fn vulkan_build_rows(needed: bool, distro: LinuxDistro, gpu: &GpuInfo) -> Vec<Dependency> {
    if !gpu.has_vulkan {
        return Vec::new();
    }
    [
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
    ]
    .into_iter()
    .map(|(name, description, present)| Dependency {
        name: name.to_owned(),
        status: present_if(present, "available"),
        description: description.to_owned(),
        required: needed,
        install_hint: Some(hint_for(name, distro)),
    })
    .collect()
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
