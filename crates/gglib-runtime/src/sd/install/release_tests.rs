//! The pin, its override, and which asset each platform installs, against
//! the asset names `master-948-228c707` really published (read with
//! `gh release view` on 2026-10-10).

use gglib_core::utils::system::GpuInfo;

use super::release::{SD_RELEASE, SdAsset, sd_download_dir, sd_platform_asset, sd_wanted};
use super::{PINNED_SD_RELEASE, SD_RELEASE_ENV};
use crate::binary_install::{AssetChoice, ReleaseSelector, selector_from_override};

/// Every asset of the pinned release, as GitHub lists them.
const ASSETS: [&str; 9] = [
    "cudart-sd-bin-win-cu12-x64.zip",
    "sd-master-228c707-bin-Darwin-macOS-26.6.2-arm64.zip",
    "sd-master-228c707-bin-Linux-Ubuntu-24.04-x86_64-rocm-7.14.0.zip",
    "sd-master-228c707-bin-Linux-Ubuntu-24.04-x86_64-vulkan.zip",
    "sd-master-228c707-bin-Linux-Ubuntu-24.04-x86_64.zip",
    "sd-master-228c707-bin-win-cpu-x64.zip",
    "sd-master-228c707-bin-win-cuda12-x64.zip",
    "sd-master-228c707-bin-win-rocm-7.14.0-x64.zip",
    "sd-master-228c707-bin-win-vulkan-x64.zip",
];

fn gpu(nvidia_with_cuda: bool, vulkan: bool) -> GpuInfo {
    GpuInfo {
        has_nvidia_gpu: nvidia_with_cuda,
        cuda_version: nvidia_with_cuda.then(|| "12.4".to_owned()),
        has_metal: false,
        has_vulkan: vulkan,
        vulkan_headers: false,
        vulkan_glslc: false,
        vulkan_spirv_headers: false,
    }
}

fn pick(os: &str, arch: &str, gpu: &GpuInfo) -> SdAsset {
    sd_platform_asset(os, arch, gpu).unwrap_or_else(|e| panic!("{os}/{arch}: {e}"))
}

/// The names of [`ASSETS`] `asset` matches.
fn matching(asset: &SdAsset) -> Vec<&'static str> {
    ASSETS
        .into_iter()
        .filter(|name| asset.matcher.matches(name))
        .collect()
}

#[test]
fn each_platform_matches_exactly_its_own_asset() {
    let none = gpu(false, false);
    let cases = [
        (
            "macos",
            "aarch64",
            none.clone(),
            "sd-master-228c707-bin-Darwin-macOS-26.6.2-arm64.zip",
        ),
        (
            "macos",
            "x86_64",
            none.clone(),
            "sd-master-228c707-bin-Darwin-macOS-26.6.2-arm64.zip",
        ),
        (
            "linux",
            "x86_64",
            gpu(false, true),
            "sd-master-228c707-bin-Linux-Ubuntu-24.04-x86_64-vulkan.zip",
        ),
        (
            "linux",
            "x86_64",
            gpu(true, false),
            "sd-master-228c707-bin-Linux-Ubuntu-24.04-x86_64.zip",
        ),
        (
            "linux",
            "x86_64",
            none.clone(),
            "sd-master-228c707-bin-Linux-Ubuntu-24.04-x86_64.zip",
        ),
        (
            "windows",
            "x86_64",
            gpu(true, true),
            "sd-master-228c707-bin-win-cuda12-x64.zip",
        ),
        (
            "windows",
            "x86_64",
            gpu(false, true),
            "sd-master-228c707-bin-win-vulkan-x64.zip",
        ),
        (
            "windows",
            "x86_64",
            none,
            "sd-master-228c707-bin-win-cpu-x64.zip",
        ),
    ];
    for (os, arch, gpu, expected) in cases {
        let asset = pick(os, arch, &gpu);
        assert_eq!(matching(&asset), [expected], "{os}/{arch} {gpu:?}");
    }
}

/// Each platform with a download, and what it matches in `assets` when that
/// is not exactly one asset; and each CUDA runtime package a platform fetches
/// that `assets` holds other than once. The platforms are each os and arch
/// listed here with every combination of the GPU fields the table reads
/// today.
fn not_matched_once<'a>(assets: &[&'a str]) -> Vec<(String, Vec<&'a str>)> {
    let gpus = [
        gpu(false, false),
        gpu(false, true),
        gpu(true, false),
        gpu(true, true),
    ];
    let mut problems = Vec::new();
    for os in ["macos", "linux", "windows"] {
        for arch in ["aarch64", "x86_64"] {
            for gpu in &gpus {
                let Ok(asset) = sd_platform_asset(os, arch, gpu) else {
                    continue;
                };
                let platform = format!("{os}/{arch} {gpu:?}");
                let found: Vec<&str> = assets
                    .iter()
                    .copied()
                    .filter(|name| asset.matcher.matches(name))
                    .collect();
                if found.len() != 1 {
                    problems.push((platform.clone(), found));
                }
                if let Some(runtime) = asset.cuda_runtime
                    && assets.iter().filter(|name| name.contains(runtime)).count() != 1
                {
                    problems.push((format!("{platform}: {runtime}"), Vec::new()));
                }
            }
        }
    }
    problems
}

/// A second build that also matches is caught, as a missing one is.
#[test]
fn a_platform_matching_two_assets_or_none_is_named() {
    assert_eq!(not_matched_once(&ASSETS), []);
    let mut doubled = ASSETS.to_vec();
    doubled.push("sd-master-228c707-bin-Linux-Ubuntu-26.04-x86_64.zip");
    let named: Vec<String> = not_matched_once(&doubled)
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    assert!(!named.is_empty(), "{named:?}");
    assert!(
        named.iter().all(|p| p.starts_with("linux/x86_64")),
        "{named:?}"
    );
    let without_windows: Vec<&str> = ASSETS
        .into_iter()
        .filter(|name| !name.contains("-win-"))
        .collect();
    let named: Vec<String> = not_matched_once(&without_windows)
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    assert!(!named.is_empty(), "{named:?}");
    assert!(named.iter().all(|p| p.starts_with("windows/")), "{named:?}");
    assert!(
        named.iter().any(|p| p.contains("cudart-sd-bin-win")),
        "{named:?}"
    );
}

/// The same of the release pinned now, as published: CI lists its assets.
/// The names carry the build machine's OS version, so each platform is held
/// to exactly one match rather than to a name.
#[test]
#[ignore = "reads the pinned release's asset list; CI sets GGLIB_ENGINE_ASSETS_DIR"]
fn each_platform_matches_exactly_one_asset_of_the_pinned_release() {
    let assets = crate::pinned_engines_tests::pinned_release_assets("sd.txt");
    let assets: Vec<&str> = assets.iter().map(String::as_str).collect();
    assert_eq!(not_matched_once(&assets), []);
}

#[test]
fn the_release_refuses_a_second_match_rather_than_taking_the_first() {
    assert_eq!(SD_RELEASE.choice, AssetChoice::ExactlyOne);
}

#[test]
fn only_a_cpu_build_carries_a_warning() {
    let none = gpu(false, false);
    assert!(pick("linux", "x86_64", &none).warning.is_some());
    assert!(pick("windows", "x86_64", &none).warning.is_some());
    assert!(pick("linux", "x86_64", &gpu(false, true)).warning.is_none());
    assert!(
        pick("windows", "x86_64", &gpu(true, false))
            .warning
            .is_none()
    );
    assert!(pick("macos", "aarch64", &none).warning.is_none());
}

/// The server is linked against a library shipped beside it; an archive
/// without that library would install a binary that cannot start.
#[test]
fn each_platform_requires_the_server_and_the_library_beside_it() {
    let none = gpu(false, false);
    assert_eq!(
        pick("macos", "aarch64", &none).required,
        ["sd-server", "libstable-diffusion.dylib"]
    );
    assert_eq!(
        pick("linux", "x86_64", &none).required,
        ["sd-server", "libstable-diffusion.so"]
    );
    assert_eq!(
        pick("windows", "x86_64", &none).required,
        ["sd-server.exe", "stable-diffusion.dll"]
    );
}

#[test]
fn only_the_cuda_build_fetches_the_cuda_runtime_and_its_pattern_finds_only_that_package() {
    let cuda = pick("windows", "x86_64", &gpu(true, true));
    let pattern = cuda.cuda_runtime.expect("the CUDA build needs its runtime");
    let found: Vec<&str> = ASSETS
        .into_iter()
        .filter(|name| name.contains(pattern))
        .collect();
    assert_eq!(found, ["cudart-sd-bin-win-cu12-x64.zip"]);

    for (os, gpu) in [
        ("windows", gpu(false, true)),
        ("windows", gpu(false, false)),
        ("linux", gpu(true, true)),
        ("macos", gpu(true, true)),
    ] {
        assert_eq!(pick(os, "x86_64", &gpu).cuda_runtime, None, "{os}");
    }
}

#[test]
fn a_platform_with_no_asset_says_so() {
    let none = gpu(false, false);
    assert_eq!(
        sd_platform_asset("linux", "aarch64", &none),
        Err("stable-diffusion.cpp publishes no pre-built linux build for aarch64".to_owned())
    );
    assert_eq!(
        sd_platform_asset("windows", "aarch64", &none),
        Err("stable-diffusion.cpp publishes no pre-built windows build for aarch64".to_owned())
    );
    assert_eq!(
        sd_platform_asset("freebsd", "x86_64", &none),
        Err("stable-diffusion.cpp publishes no pre-built build for freebsd".to_owned())
    );
}

/// Upstream tags `master-<build>-<seven hex>`; a pin of another shape would
/// 404 on a user's machine, not here.
#[test]
fn the_pin_is_a_well_formed_master_tag() {
    let rest = PINNED_SD_RELEASE
        .strip_prefix("master-")
        .expect("pin must start master-");
    let (build, commit) = rest.split_once('-').expect("master-<build>-<commit>");
    assert!(
        !build.is_empty() && build.bytes().all(|b| b.is_ascii_digit()),
        "{PINNED_SD_RELEASE}"
    );
    assert!(
        commit.len() == 7 && commit.bytes().all(|b| b.is_ascii_hexdigit()),
        "{PINNED_SD_RELEASE}"
    );
    assert_eq!(SD_RELEASE.pinned, PINNED_SD_RELEASE);
    assert_eq!(SD_RELEASE.env, SD_RELEASE_ENV);
    assert_eq!(SD_RELEASE_ENV, "GGLIB_SD_RELEASE");
    assert_eq!(SD_RELEASE.repo, "leejet/stable-diffusion.cpp");
}

#[test]
fn the_override_is_blank_latest_or_a_tag() {
    assert_eq!(
        selector_from_override(&SD_RELEASE, ""),
        ReleaseSelector::Tag("master-948-228c707".to_owned())
    );
    assert_eq!(
        selector_from_override(&SD_RELEASE, "  "),
        ReleaseSelector::Tag("master-948-228c707".to_owned())
    );
    assert_eq!(
        selector_from_override(&SD_RELEASE, "LATEST"),
        ReleaseSelector::Latest
    );
    assert_eq!(
        selector_from_override(&SD_RELEASE, " master-950-abcdef0 "),
        ReleaseSelector::Tag("master-950-abcdef0".to_owned())
    );
}

/// A llama.cpp install removes `<data root>/downloads` whole when it
/// finishes; stable-diffusion.cpp's archive must not be in there.
#[test]
fn the_download_directory_is_not_llama_cpps() {
    let root = gglib_core::paths::isolate_data_root();
    let sd = sd_download_dir().unwrap();
    assert_eq!(sd, root.join(".sd").join("downloads"));
    let llama = gglib_core::paths::data_root().unwrap().join("downloads");
    assert!(
        !sd.starts_with(&llama),
        "{} is inside {}",
        sd.display(),
        llama.display()
    );
}

#[test]
fn the_licences_and_the_cli_stay_in_the_archive() {
    for skipped in [
        "ggml.txt",
        "stable-diffusion.cpp.txt",
        "sd-cli",
        "sd-cli.exe",
    ] {
        assert!(!sd_wanted(skipped), "{skipped}");
    }
    for kept in [
        "sd-server",
        "sd-server.exe",
        "libstable-diffusion.dylib",
        "libstable-diffusion.so",
        "libggml-cpu-haswell.so",
        "stable-diffusion.dll",
    ] {
        assert!(sd_wanted(kept), "{kept}");
    }
}

/// A source build clones the repository the downloads come from.
#[test]
fn the_source_is_cloned_from_the_release_repository() {
    assert_eq!(
        super::pipeline::SD_SOURCE.url,
        format!("https://github.com/{}", SD_RELEASE.repo)
    );
}
