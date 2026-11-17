//! The pin, its override, and which asset each platform installs, against
//! the asset names `master-948-228c707` really published (read with
//! `gh release view` on 2026-10-10).

use gglib_core::utils::system::GpuInfo;

use super::release::{
    CPU_WARNING, PINNED_SD_LINUX_GLIBC, SD_RELEASE, SdAsset, glibc_too_old, sd_download_dir,
    sd_platform_asset, sd_wanted,
};
use super::{PINNED_SD_RELEASE, SD_RELEASE_ENV};
use crate::binary_install::{AssetChoice, ReleaseSelector, selector_from_override};
use crate::system::glibc::Glibc;
use crate::system::vulkan_devices::{fixtures, vulkan_gpu_present};

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

/// What is wrong with `floor` against `reading`, the newest
/// `GLIBC_<version>` the installed Linux build names, as CI saved it: a
/// reading that is missing, one that is no version, or another version.
fn floor_against(floor: Glibc, reading: std::io::Result<String>) -> Result<(), String> {
    let said = reading.map_err(|e| format!("no reading of the installed Linux build: {e}"))?;
    let asks = Glibc::parse(&said).ok_or_else(|| format!("{said:?} is no glibc version"))?;
    if asks == floor {
        return Ok(());
    }
    Err(format!(
        "{PINNED_SD_RELEASE}'s Linux build asks for glibc {asks} and PINNED_SD_LINUX_GLIBC is \
         {floor}: it, and the README's line on which Linux the image download needs, move \
         with the pin"
    ))
}

/// The floor is what the pinned Linux build asks for, read from the build
/// itself. CI's Linux leg installs it and saves the newest `GLIBC_<version>`
/// any of its files names as `sd-glibc.txt` in `GGLIB_ENGINE_HELP_DIR`. A
/// floor below it lets a download through that cannot start; one above it
/// refuses a download that runs. Only Linux installs a Linux build, so only
/// there is a reading expected, and there a missing one fails.
#[test]
#[ignore = "reads what the installed pinned Linux build asks of glibc; CI sets GGLIB_ENGINE_HELP_DIR"]
fn the_floor_is_the_glibc_the_pinned_linux_build_asks_for() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let dir = std::env::var_os("GGLIB_ENGINE_HELP_DIR")
        .map(std::path::PathBuf::from)
        .expect("GGLIB_ENGINE_HELP_DIR names the folder CI saved the reading in");
    let reading = std::fs::read_to_string(dir.join("sd-glibc.txt"));
    assert_eq!(floor_against(PINNED_SD_LINUX_GLIBC, reading), Ok(()));
}

/// The check above fails on a reading that is missing or is no version, and
/// on a floor either side of what the build asks for; it passes on the one
/// version. Run everywhere, since the check itself runs only on CI's Linux
/// leg.
#[test]
fn a_floor_is_held_to_the_reading_and_a_missing_reading_fails() {
    let floor = Glibc(2, 43);
    let reads = |text: &str| Ok(text.to_owned());
    assert_eq!(floor_against(floor, reads("GLIBC_2.43\n")), Ok(()));
    for (reading, wrong) in [
        (
            reads("GLIBC_2.38\n"),
            "asks for glibc 2.38 and PINNED_SD_LINUX_GLIBC is 2.43",
        ),
        (
            reads("GLIBC_2.45\n"),
            "asks for glibc 2.45 and PINNED_SD_LINUX_GLIBC is 2.43",
        ),
        (reads(""), "\"\" is no glibc version"),
        (
            Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
            "no reading of the installed Linux build",
        ),
    ] {
        let found = floor_against(floor, reading).expect_err(wrong);
        assert!(found.contains(wrong), "{found}");
    }
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

/// The Vulkan loader alone, a software rasteriser or a list with no device
/// in it is no GPU: the CPU build is installed, and said to be. The twin of
/// llama.cpp's test of the same choice.
#[test]
fn the_cpu_build_is_chosen_when_no_vulkan_device_is_a_gpu() {
    let mut wrong = Vec::new();
    for os in ["linux", "windows"] {
        for (what, devices) in fixtures::without_a_gpu() {
            let found = GpuInfo {
                has_vulkan: vulkan_gpu_present(os, true, &devices),
                ..gpu(false, false)
            };
            let asset = pick(os, "x86_64", &found);
            if !asset.description.ends_with("(CPU)") || asset.warning != Some(CPU_WARNING) {
                wrong.push(format!("{os}, {what}: {}", asset.description));
            }
        }
    }
    assert!(wrong.is_empty(), "not the CPU build:\n{}", wrong.join("\n"));
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

fn the_pin() -> ReleaseSelector {
    ReleaseSelector::Tag(PINNED_SD_RELEASE.to_owned())
}

/// The floor a test compares against: what the builds made on Ubuntu 26.04
/// ask for, whatever the pin asks for today.
const NEEDS: Glibc = Glibc(2, 43);

/// Ubuntu 24.04 has glibc 2.39 and Debian 13 has 2.41: the pinned Linux
/// build is not downloaded there, and the reason names both versions. It is
/// the sentence every surface puts its own words around ("…; building
/// sd-server from source instead.").
#[test]
fn the_linux_download_is_refused_where_glibc_is_older_than_it_needs() {
    for (has, said) in [(Glibc(2, 39), "2.39"), (Glibc(2, 41), "2.41")] {
        assert_eq!(
            glibc_too_old("linux", &the_pin(), NEEDS, || Some(has)),
            Some(format!(
                "stable-diffusion.cpp's pre-built Linux build needs glibc 2.43 or newer, and \
                 this system has glibc {said}"
            ))
        );
    }
}

#[test]
fn the_linux_download_is_taken_where_glibc_is_what_it_needs_or_newer() {
    for has in [Glibc(2, 43), Glibc(2, 44), Glibc(3, 0)] {
        assert_eq!(
            glibc_too_old("linux", &the_pin(), NEEDS, || Some(has)),
            None,
            "{has}"
        );
    }
}

/// A glibc that cannot be read refuses nothing: the install's own check
/// says why a server cannot start.
#[test]
fn an_unread_glibc_refuses_nothing() {
    assert_eq!(glibc_too_old("linux", &the_pin(), NEEDS, || None), None);
}

/// The floor is the pin's. A release an override names may have been built
/// against another glibc, so nothing is said of it, and the system is not
/// even asked.
#[test]
fn a_release_an_override_names_is_not_held_to_the_pins_floor() {
    for selector in [
        ReleaseSelector::Tag("master-1043-3be2378".to_owned()),
        ReleaseSelector::Latest,
    ] {
        assert_eq!(
            glibc_too_old("linux", &selector, NEEDS, || panic!("not asked")),
            None,
            "{selector:?}"
        );
    }
}

/// macOS and Windows have no glibc, and are not asked for one.
#[test]
fn only_linux_is_held_to_a_glibc() {
    for os in ["macos", "windows"] {
        assert_eq!(
            glibc_too_old(os, &the_pin(), NEEDS, || panic!("not asked")),
            None,
            "{os}"
        );
    }
}

/// The pin's own floor refuses a system older than it and no other: the
/// constant is the one the refusal is given.
#[test]
fn the_pins_floor_is_a_glibc_this_decade_shipped() {
    assert!(
        PINNED_SD_LINUX_GLIBC >= Glibc(2, 31),
        "{PINNED_SD_LINUX_GLIBC}"
    );
    assert!(
        PINNED_SD_LINUX_GLIBC < Glibc(3, 0),
        "{PINNED_SD_LINUX_GLIBC}"
    );
    let older = Glibc(PINNED_SD_LINUX_GLIBC.0, PINNED_SD_LINUX_GLIBC.1 - 1);
    assert!(glibc_too_old("linux", &the_pin(), PINNED_SD_LINUX_GLIBC, || Some(older)).is_some());
    assert_eq!(
        glibc_too_old("linux", &the_pin(), PINNED_SD_LINUX_GLIBC, || Some(
            PINNED_SD_LINUX_GLIBC
        )),
        None
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
        ReleaseSelector::Tag(PINNED_SD_RELEASE.to_owned())
    );
    assert_eq!(
        selector_from_override(&SD_RELEASE, "  "),
        ReleaseSelector::Tag(PINNED_SD_RELEASE.to_owned())
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
