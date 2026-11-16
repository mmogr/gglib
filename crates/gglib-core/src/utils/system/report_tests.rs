//! The report on every OS, run on every OS: an OS is a value here.

use super::*;
use crate::utils::system::needs::{need, needs_on};

const EVERY_OS: [HostOs; 3] = [HostOs::Linux, HostOs::MacOs, HostOs::Windows];

/// A machine with gglib installed and nothing else: no compiler, no Rust, no
/// Node, no git. It has a pre-built llama.cpp to download, as Linux, macOS
/// and Windows all do.
fn bare_machine() -> Found {
    Found {
        llama_install: EngineInstall::Download,
        ..Found::default()
    }
}

fn group(report: &DependencyReport, purpose: Purpose) -> &DependencyGroup {
    report
        .groups
        .iter()
        .find(|group| group.purpose == purpose)
        .unwrap_or_else(|| panic!("a {purpose:?} group in {report:#?}"))
}

fn names(deps: &[&Dependency]) -> Vec<String> {
    deps.iter().map(|dep| dep.name.clone()).collect()
}

/// C11: an installed gglib was told that cargo, rustc, node, npm, make, gcc
/// and more were required and missing. None of them is needed to run it.
#[test]
fn an_installed_gglib_with_no_build_tools_is_missing_nothing() {
    for os in EVERY_OS {
        let report = dependency_report(
            os,
            BuildOrigin::Installed,
            LinuxDistro::Debian,
            &bare_machine(),
        );

        assert!(
            report.missing().is_empty(),
            "{os:?}: {:?} called required and missing",
            names(&report.missing())
        );
        for build in [Purpose::BuildGglib, Purpose::BuildEngine] {
            let group = group(&report, build);
            assert!(!group.needed, "{os:?}: {build:?} needed");
            assert!(
                group.dependencies.iter().all(|dep| !dep.required),
                "{os:?}: {build:?} has a required row"
            );
        }
    }
}

/// The report says in plain words what the build tools are for, and that an
/// installed gglib needs none of them.
#[test]
fn the_build_groups_say_what_they_are_for_and_that_an_installed_gglib_needs_none() {
    for os in EVERY_OS {
        let report = dependency_report(
            os,
            BuildOrigin::Installed,
            LinuxDistro::Debian,
            &bare_machine(),
        );

        let gglib = group(&report, Purpose::BuildGglib);
        assert_eq!(gglib.title, "Building gglib from source", "{os:?}");
        assert!(
            gglib
                .explanation
                .contains("An installed gglib needs none of them"),
            "{os:?}: {}",
            gglib.explanation
        );

        let engine = group(&report, Purpose::BuildEngine);
        assert_eq!(engine.title, "Building an engine from source", "{os:?}");
        assert!(
            engine
                .explanation
                .contains("An installed gglib needs none of them"),
            "{os:?}: {}",
            engine.explanation
        );

        assert_eq!(
            report.verdict(),
            "Nothing this gglib needs is missing.",
            "{os:?}"
        );
    }
}

/// What an installed gglib runs with is listed as such: the engines, and
/// the GPU runtime it found.
#[test]
fn what_an_installed_gglib_runs_with_is_listed_under_running_it() {
    let machines = [
        (
            HostOs::MacOs,
            GpuInfo {
                has_metal: true,
                ..GpuInfo::default()
            },
            "Metal",
        ),
        (
            HostOs::Windows,
            GpuInfo {
                has_vulkan: true,
                ..GpuInfo::default()
            },
            "Vulkan runtime",
        ),
        (
            HostOs::Linux,
            GpuInfo {
                has_nvidia_gpu: true,
                cuda_version: Some("12.4".to_owned()),
                ..GpuInfo::default()
            },
            "CUDA",
        ),
    ];
    for (os, gpu, runtime) in machines {
        let found = Found {
            gpu,
            llama_installed: true,
            ..bare_machine()
        };
        let report = dependency_report(os, BuildOrigin::Installed, LinuxDistro::Debian, &found);
        let run = group(&report, Purpose::Run);

        assert_eq!(run.title, "Running gglib", "{os:?}");
        assert!(run.needed, "{os:?}");
        let listed: Vec<&str> = run.dependencies.iter().map(|d| d.name.as_str()).collect();
        for name in ["llama.cpp", "stable-diffusion.cpp", "python3", runtime] {
            assert!(listed.contains(&name), "{os:?}: {name} in {listed:?}");
        }
        assert!(report.missing().is_empty(), "{os:?}");
    }
}

/// Vulkan's build packages build an engine; they are not what a Vulkan GPU
/// needs to run one that was downloaded.
#[test]
fn vulkan_build_packages_are_for_building_an_engine() {
    let found = Found {
        gpu: GpuInfo {
            has_vulkan: true,
            ..GpuInfo::default()
        },
        ..bare_machine()
    };
    let report = dependency_report(
        HostOs::Windows,
        BuildOrigin::Installed,
        LinuxDistro::Unknown,
        &found,
    );
    let engine = group(&report, Purpose::BuildEngine);
    for name in ["Vulkan headers", "glslc", "SPIR-V headers"] {
        assert!(
            engine.dependencies.iter().any(|dep| dep.name == name),
            "{name} under {engine:#?}"
        );
    }
    assert!(report.missing().is_empty());
}

/// A build from a checkout still needs what builds it.
#[test]
fn a_checkout_still_needs_its_build_tools() {
    for os in EVERY_OS {
        let report = dependency_report(
            os,
            BuildOrigin::Checkout,
            LinuxDistro::Debian,
            &bare_machine(),
        );
        let missing = names(&report.missing());
        for name in [
            "cargo",
            "rustc",
            "node",
            "npm",
            "git",
            "cmake",
            "C++ compiler",
        ] {
            assert!(
                missing.contains(&name.to_owned()),
                "{os:?}: {name} in {missing:?}"
            );
        }
        assert!(group(&report, Purpose::BuildGglib).needed, "{os:?}");
        assert!(
            !group(&report, Purpose::BuildGglib)
                .explanation
                .contains("An installed gglib"),
            "{os:?}"
        );
    }
}

/// Where there is no pre-built llama.cpp to download, an installed gglib
/// builds it, and needs the tools that do.
#[test]
fn an_installed_gglib_with_no_engine_download_needs_the_engine_build() {
    let found = Found {
        llama_install: EngineInstall::NoDownload,
        ..Found::default()
    };
    let report = dependency_report(
        HostOs::Linux,
        BuildOrigin::Installed,
        LinuxDistro::Debian,
        &found,
    );
    let missing = names(&report.missing());
    for name in ["git", "cmake", "C++ compiler"] {
        assert!(missing.contains(&name.to_owned()), "{name} in {missing:?}");
    }
    assert!(!missing.contains(&"cargo".to_owned()), "{missing:?}");
}

/// Each tool is looked for under the name it has on that OS. On Windows npm
/// is `npm.cmd`, which a program cannot be started as under its bare name,
/// and make, pkg-config and pkg-config's libraries are not part of a build.
#[test]
fn each_tool_is_looked_for_under_its_name_on_that_os() {
    assert_eq!(need("npm").unwrap().names(HostOs::Windows), ["npm.cmd"]);
    assert_eq!(need("npm").unwrap().names(HostOs::Linux), ["npm"]);

    let on_windows: Vec<&str> = needs_on(HostOs::Windows).map(|n| n.name).collect();
    for absent in ["make", "gcc", "g++", "pkg-config", "libssl-dev", "patchelf"] {
        assert!(!on_windows.contains(&absent), "{absent} on Windows");
    }
    assert_eq!(
        need("C++ compiler").unwrap().names(HostOs::Windows),
        ["cl", "g++", "clang++"]
    );

    assert!(!needs_on(HostOs::MacOs).any(|n| n.name == "patchelf"));
    assert!(needs_on(HostOs::Linux).any(|n| n.name == "patchelf"));
}

/// A row found is present with its version, and one not found is missing.
#[test]
fn a_row_found_carries_its_version() {
    let mut found = bare_machine();
    found.versions.insert("cargo", "1.97.1".to_owned());
    let report = dependency_report(
        HostOs::MacOs,
        BuildOrigin::Checkout,
        LinuxDistro::Unknown,
        &found,
    );
    let cargo = report.dependencies().find(|d| d.name == "cargo").unwrap();
    assert_eq!(
        cargo.status,
        DependencyStatus::Present {
            version: "1.97.1".to_owned()
        }
    );
    let rustc = report.dependencies().find(|d| d.name == "rustc").unwrap();
    assert_eq!(rustc.status, DependencyStatus::Missing);
}

/// An installed gglib whose llama.cpp was built from source (a CUDA build on
/// Linux, say) builds its updates from source too, download or not, so it
/// needs the engine build and is told why.
#[test]
fn an_installed_gglib_with_an_engine_built_from_source_needs_the_engine_build() {
    let found = Found {
        llama_installed: true,
        llama_install: EngineInstall::Rebuild,
        ..Found::default()
    };
    let report = dependency_report(
        HostOs::Linux,
        BuildOrigin::Installed,
        LinuxDistro::Debian,
        &found,
    );
    let engine = group(&report, Purpose::BuildEngine);
    assert!(engine.needed);
    assert!(
        engine.explanation.contains("built from source"),
        "{}",
        engine.explanation
    );
    let missing = names(&report.missing());
    assert!(missing.contains(&"cmake".to_owned()), "{missing:?}");
    assert!(!group(&report, Purpose::BuildGglib).needed);
}
