//! Which pre-built llama.cpp each platform installs, against the asset names
//! `b11429` really published (`B11429_ASSETS`).

use std::collections::BTreeSet;

use gglib_core::utils::system::GpuInfo;

use super::tests::B11429_ASSETS;
use super::{
    CUDART_PATTERN, LLAMA_CPU_WARNING, PrebuiltAvailability, llama_matcher, llama_platform_asset,
};

fn gpu(has_nvidia_gpu: bool, cuda: bool, has_vulkan: bool) -> GpuInfo {
    GpuInfo {
        has_nvidia_gpu,
        cuda_version: cuda.then(|| "12.4".to_owned()),
        has_metal: false,
        has_vulkan,
        vulkan_headers: false,
        vulkan_glslc: false,
        vulkan_spirv_headers: false,
    }
}

/// What a platform installs: its archive's pattern, its description and its
/// warning, or why there is none.
fn picked(
    os: &str,
    arch: &str,
    gpu: &GpuInfo,
) -> Result<(String, String, Option<&'static str>), String> {
    match llama_platform_asset(os, arch, gpu) {
        PrebuiltAvailability::Available {
            asset_pattern,
            description,
            warning,
        } => Ok((asset_pattern, description, warning)),
        PrebuiltAvailability::NotAvailable { reason } => Err(reason),
    }
}

/// One row of the table: a platform and its GPU, and what it installs.
type Row<'a> = (&'a str, &'a str, &'a GpuInfo, &'a str, &'a str, bool);

fn assert_rows(rows: &[Row<'_>]) {
    for &(os, arch, gpu, pattern, description, warned) in rows {
        assert_eq!(
            picked(os, arch, gpu),
            Ok((
                pattern.to_owned(),
                description.to_owned(),
                warned.then_some(LLAMA_CPU_WARNING)
            )),
            "{os} {arch} {gpu:?}"
        );
    }
}

#[test]
fn macos_and_linux_take_the_build_their_gpu_runs() {
    let none = gpu(false, false, false);
    let vulkan = gpu(false, false, true);
    let cuda = gpu(true, true, true);
    assert_rows(&[
        (
            "macos",
            "aarch64",
            &none,
            "bin-macos-arm64.tar.gz",
            "macOS ARM64 (Metal)",
            false,
        ),
        (
            "macos",
            "x86_64",
            &none,
            "bin-macos-x64.tar.gz",
            "macOS x64 (Metal)",
            false,
        ),
        (
            "linux",
            "x86_64",
            &vulkan,
            "bin-ubuntu-vulkan-x64.tar.gz",
            "Linux x64 (Vulkan)",
            false,
        ),
        (
            "linux",
            "x86_64",
            &cuda,
            "bin-ubuntu-vulkan-x64.tar.gz",
            "Linux x64 (Vulkan)",
            false,
        ),
        (
            "linux",
            "x86_64",
            &none,
            "bin-ubuntu-x64.tar.gz",
            "Linux x64 (CPU)",
            true,
        ),
        (
            "linux",
            "aarch64",
            &vulkan,
            "bin-ubuntu-vulkan-arm64.tar.gz",
            "Linux ARM64 (Vulkan)",
            false,
        ),
        (
            "linux",
            "aarch64",
            &none,
            "bin-ubuntu-arm64.tar.gz",
            "Linux ARM64 (CPU)",
            true,
        ),
    ]);
}

#[test]
fn windows_takes_the_build_its_gpu_runs() {
    let none = gpu(false, false, false);
    let vulkan = gpu(false, false, true);
    let cuda = gpu(true, true, true);
    let nvidia_without_cuda = gpu(true, false, true);
    assert_rows(&[
        (
            "windows",
            "x86_64",
            &cuda,
            "bin-win-cuda-12.4-x64.zip",
            "Windows x64 (CUDA 12.4)",
            false,
        ),
        (
            "windows",
            "x86_64",
            &nvidia_without_cuda,
            "bin-win-vulkan-x64.zip",
            "Windows x64 (Vulkan)",
            false,
        ),
        (
            "windows",
            "x86_64",
            &none,
            "bin-win-cpu-x64.zip",
            "Windows x64 (CPU)",
            true,
        ),
        (
            "windows",
            "aarch64",
            &vulkan,
            "bin-win-vulkan-arm64.zip",
            "Windows ARM64 (Vulkan)",
            false,
        ),
        (
            "windows",
            "aarch64",
            &none,
            "bin-win-cpu-arm64.zip",
            "Windows ARM64 (CPU)",
            true,
        ),
    ]);
}

#[test]
fn a_platform_with_no_build_says_so() {
    assert_eq!(
        picked("freebsd", "x86_64", &gpu(false, false, false)),
        Err("llama.cpp publishes no pre-built build for freebsd on x86_64".to_owned())
    );
}

/// Every archive pattern the table gives, read from the table itself for
/// each os and arch listed here and every combination of the GPU fields it
/// reads today.
fn table_patterns() -> BTreeSet<String> {
    let gpus = [
        gpu(false, false, false),
        gpu(false, false, true),
        gpu(true, false, false),
        gpu(true, true, false),
        gpu(true, true, true),
    ];
    let mut patterns = BTreeSet::new();
    for os in ["macos", "linux", "windows"] {
        for arch in ["aarch64", "x86_64"] {
            for gpu in &gpus {
                if let Ok((pattern, _, _)) = picked(os, arch, gpu) {
                    patterns.insert(pattern);
                }
            }
        }
    }
    patterns
}

/// Each pattern matches exactly one of `assets`, release `tag`'s own archive.
fn assert_each_pattern_takes_its_own_archive(assets: &[&str], tag: &str) {
    let patterns = table_patterns();
    assert!(patterns.contains("bin-win-cpu-arm64.zip"), "{patterns:?}");
    let cuda: Vec<&String> = patterns.iter().filter(|p| p.contains("win-cuda")).collect();
    assert_eq!(cuda.len(), 1, "{patterns:?}");
    for pattern in &patterns {
        let matcher = llama_matcher(pattern);
        let matching: Vec<&str> = assets
            .iter()
            .copied()
            .filter(|name| matcher.matches(name))
            .collect();
        assert_eq!(
            matching,
            [format!("llama-{tag}-{pattern}")],
            "pattern {pattern}"
        );
    }
    // A CUDA build also fetches the runtime package, the first asset whose
    // name holds the pattern. A missing one is not fatal to an install, so
    // nothing else would notice it gone, or another CUDA's in its place.
    let runtime = assets.iter().find(|name| name.contains(CUDART_PATTERN));
    assert_eq!(
        runtime.map(|name| (*name).to_owned()),
        Some(format!("cudart-llama-{}", cuda[0])),
        "no {CUDART_PATTERN} package for the CUDA build"
    );
}

/// Each pattern the table can give matches exactly one archive of the real
/// release, the Windows CUDA one included: the CUDA runtime package listed
/// before it ends in the same pattern.
#[test]
fn every_pattern_takes_its_own_archive_from_a_real_release() {
    assert_each_pattern_takes_its_own_archive(B11429_ASSETS, "b11429");
}

/// A release that has lost its Windows CUDA runtime package fails the
/// check, though no archive pattern names it.
#[test]
#[should_panic(expected = "no cudart-llama-bin-win-cuda package")]
fn a_release_without_the_cuda_runtime_package_fails() {
    let assets: Vec<&str> = B11429_ASSETS
        .iter()
        .copied()
        .filter(|name| !name.starts_with("cudart-llama-bin-win"))
        .collect();
    assert_each_pattern_takes_its_own_archive(&assets, "b11429");
}

/// Nor does one whose first runtime package is another CUDA's, which the
/// install would unpack beside the build.
#[test]
#[should_panic(expected = "no cudart-llama-bin-win-cuda package")]
fn a_release_whose_cuda_runtime_is_another_cudas_fails() {
    let assets: Vec<&str> = B11429_ASSETS
        .iter()
        .copied()
        .filter(|name| *name != "cudart-llama-bin-win-cuda-12.4-x64.zip")
        .collect();
    assert_each_pattern_takes_its_own_archive(&assets, "b11429");
}

/// The same of the release pinned now, as published: CI lists its assets.
#[test]
#[ignore = "reads the pinned release's asset list; CI sets GGLIB_ENGINE_ASSETS_DIR"]
fn every_pattern_takes_its_own_archive_from_the_pinned_release() {
    let assets = crate::pinned_engines_tests::pinned_release_assets("llama.txt");
    let assets: Vec<&str> = assets.iter().map(String::as_str).collect();
    assert_each_pattern_takes_its_own_archive(&assets, super::PINNED_LLAMA_RELEASE);
}
