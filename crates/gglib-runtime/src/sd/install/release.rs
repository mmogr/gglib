//! Which stable-diffusion.cpp release gglib installs, and which of its assets
//! is this platform's.

use anyhow::Result;
use std::path::PathBuf;

use gglib_core::paths::sd_data_dir;
use gglib_core::utils::system::GpuInfo;

use crate::binary_install::{
    ArchiveLayout, AssetChoice, AssetMatcher, ReleaseSelector, ReleaseSpec, resolve_selector,
};
use crate::system::glibc::{Glibc, host_glibc};

/// The stable-diffusion.cpp release gglib installs unless told otherwise.
///
/// Pinned for the reason llama.cpp is (ADR 0001): an engine that moves under
/// gglib whenever upstream tags a build makes every behaviour gglib relies
/// on — the server's flags, its routes, how it answers a health probe while
/// it renders — unfalsifiable, and two users who installed a day apart run
/// different engines. Moving the pin is a deliberate commit with what changed
/// in its message. Upstream tags `master-<build>-<short commit>`.
pub const PINNED_SD_RELEASE: &str = "master-1098-b521db4";

/// The newest glibc the pinned release's Linux builds ask for: the oldest a
/// system can have and run them.
///
/// It moves with the pin, in the same commit. Upstream builds each release on
/// one Ubuntu image, and a build needs at least that image's glibc:
/// `master-1025-8ee08e3` was built on Ubuntu 24.04 and asks for 2.38, and
/// from `master-1044-f8ea2e1` they are built on Ubuntu 26.04 and ask for
/// 2.43. CI reads the installed build and holds this to it
/// (`the_floor_is_the_glibc_the_pinned_linux_build_asks_for`), so a pin
/// whose builds ask for another version cannot merge without it.
pub(crate) const PINNED_SD_LINUX_GLIBC: Glibc = Glibc(2, 38);

/// Environment override for [`PINNED_SD_RELEASE`]: a release tag, or
/// `latest` for upstream's newest. Unset or blank installs the pin.
pub const SD_RELEASE_ENV: &str = "GGLIB_SD_RELEASE";

/// Where stable-diffusion.cpp's archive is downloaded to: inside `.sd/`, so a
/// llama.cpp install running at the same moment, which removes its own
/// download directory whole, never removes this one.
pub(crate) fn sd_download_dir() -> Result<PathBuf> {
    Ok(sd_data_dir()?.join("downloads"))
}

/// Whether an archive member named `file_name` is installed.
///
/// The licences (`ggml.txt`, `stable-diffusion.cpp.txt`) stay in the archive,
/// and so does `sd-cli`: gglib runs only the server.
pub(crate) fn sd_wanted(file_name: &str) -> bool {
    let is_text = std::path::Path::new(file_name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"));
    !(is_text || file_name.starts_with("sd-cli"))
}

/// stable-diffusion.cpp as a GitHub release: `leejet/stable-diffusion.cpp`,
/// flat zips, and exactly one asset per platform, because the names carry the
/// build runner's OS version and a loose pattern could match two.
pub(crate) static SD_RELEASE: ReleaseSpec = ReleaseSpec {
    product: "stable-diffusion.cpp",
    repo: "leejet/stable-diffusion.cpp",
    pinned: PINNED_SD_RELEASE,
    env: SD_RELEASE_ENV,
    download_dir: sd_download_dir,
    archive: ArchiveLayout::Flat,
    choice: AssetChoice::ExactlyOne,
    wanted: sd_wanted,
};

/// One platform's pre-built stable-diffusion.cpp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SdAsset {
    /// Which release asset it is.
    pub(crate) matcher: AssetMatcher<'static>,
    /// The platform build as the record names it.
    pub description: &'static str,
    /// What the archive must hold: the server and the shared library it
    /// loads from beside itself.
    pub(crate) required: &'static [&'static str],
    /// For the CUDA build, a name the release's CUDA runtime package holds.
    pub(crate) cuda_runtime: Option<&'static str>,
    /// Something the user should be told about this choice.
    pub warning: Option<&'static str>,
}

const MACOS_REQUIRED: &[&str] = &["sd-server", "libstable-diffusion.dylib"];
const LINUX_REQUIRED: &[&str] = &["sd-server", "libstable-diffusion.so"];
const WINDOWS_REQUIRED: &[&str] = &["sd-server.exe", "stable-diffusion.dll"];

/// Said when the build installed runs on the CPU alone.
pub(crate) const CPU_WARNING: &str = "No GPU runtime gglib can use was found, so the CPU build of \
     stable-diffusion.cpp is installed. Images will take minutes each.";

/// The pre-built asset for `os` and `arch` (`std::env::consts` spellings),
/// with `gpu` deciding between a platform's builds; `Err` says why there is
/// none, and a source build is then the way to install.
///
/// - macOS, either arch: the one universal Metal build. Its asset is named
///   `-arm64` after the runner, though it holds both architectures.
/// - Linux `x86_64`: the Vulkan build when a Vulkan GPU is there, else the
///   CPU build, with a warning.
/// - Windows `x86_64`: CUDA 12 for an NVIDIA GPU whose driver runs it
///   (`GpuInfo::runs_cuda_12_downloads`, with no CUDA toolkit asked for), else
///   Vulkan, else the CPU build, with a warning.
pub(crate) fn sd_platform_asset(
    os: &str,
    arch: &str,
    gpu: &GpuInfo,
) -> std::result::Result<SdAsset, String> {
    let asset = |contains, ends_with, description, required| SdAsset {
        matcher: AssetMatcher {
            starts_with: None,
            contains,
            ends_with: Some(ends_with),
        },
        description,
        required,
        cuda_runtime: None,
        warning: None,
    };
    match (os, arch) {
        ("macos", _) => Ok(asset(
            &["-bin-Darwin-macOS-"],
            "-arm64.zip",
            "macOS universal (Metal)",
            MACOS_REQUIRED,
        )),
        ("linux", "x86_64") if gpu.has_vulkan => Ok(asset(
            &["-bin-Linux-"],
            "-x86_64-vulkan.zip",
            "Linux x64 (Vulkan)",
            LINUX_REQUIRED,
        )),
        ("linux", "x86_64") => Ok(SdAsset {
            warning: Some(CPU_WARNING),
            ..asset(
                &["-bin-Linux-"],
                "-x86_64.zip",
                "Linux x64 (CPU)",
                LINUX_REQUIRED,
            )
        }),
        ("windows", "x86_64") if gpu.has_nvidia_gpu && gpu.runs_cuda_12_downloads() => {
            Ok(SdAsset {
                cuda_runtime: Some("cudart-sd-bin-win-cu12"),
                ..asset(
                    &["-bin-win-cuda12-"],
                    "-x64.zip",
                    "Windows x64 (CUDA 12)",
                    WINDOWS_REQUIRED,
                )
            })
        }
        ("windows", "x86_64") if gpu.has_vulkan => Ok(asset(
            &["-bin-win-vulkan-"],
            "-x64.zip",
            "Windows x64 (Vulkan)",
            WINDOWS_REQUIRED,
        )),
        ("windows", "x86_64") => Ok(SdAsset {
            warning: Some(CPU_WARNING),
            ..asset(
                &["-bin-win-cpu-"],
                "-x64.zip",
                "Windows x64 (CPU)",
                WINDOWS_REQUIRED,
            )
        }),
        ("linux" | "windows", _) => Err(format!(
            "stable-diffusion.cpp publishes no pre-built {os} build for {arch}"
        )),
        _ => Err(format!(
            "stable-diffusion.cpp publishes no pre-built build for {os}"
        )),
    }
}

/// Why the pinned release's Linux download cannot run on a system whose
/// glibc is `has`, when it is older than the `needs` the download was built
/// against; `None` when it runs, or when nothing here can say.
///
/// Nothing is said of a release an override names, whose builds may need
/// another glibc, nor where `has` reads nothing: the install's own check
/// then says why a server cannot start. `has` is asked only on Linux, of
/// the pin.
pub(crate) fn glibc_too_old(
    os: &str,
    selector: &ReleaseSelector,
    needs: Glibc,
    has: impl FnOnce() -> Option<Glibc>,
) -> Option<String> {
    let pinned = matches!(selector, ReleaseSelector::Tag(tag) if tag == PINNED_SD_RELEASE);
    if os != "linux" || !pinned {
        return None;
    }
    let has = has()?;
    (has < needs).then(|| {
        format!(
            "stable-diffusion.cpp's pre-built Linux build needs glibc {needs} or newer, and \
             this system has glibc {has}"
        )
    })
}

/// The pre-built asset for this machine, probing its GPU where the choice
/// depends on one.
///
/// `Err` also where the pinned Linux build needs a newer glibc than this
/// system has ([`glibc_too_old`]): nothing that cannot start is downloaded,
/// and a source build is the way to install.
pub fn check_sd_prebuilt_availability() -> std::result::Result<SdAsset, String> {
    let os = std::env::consts::OS;
    let gpu = crate::system::gpu::gpu_for_downloads();
    let asset = sd_platform_asset(os, std::env::consts::ARCH, &gpu)?;
    let too_old = glibc_too_old(
        os,
        &resolve_selector(&SD_RELEASE),
        PINNED_SD_LINUX_GLIBC,
        host_glibc,
    );
    too_old.map_or(Ok(asset), Err)
}
