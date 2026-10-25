#![doc = include_str!("README.md")]
use anyhow::{Context, Result, bail};
use std::path::PathBuf;
use tokio::sync::mpsc;

use gglib_core::paths::{data_root, llama_config_path, llama_cpp_dir, llama_server_path};
use gglib_core::utils::system::GpuInfo;

use super::config::InstallRecord;
use super::install_events::{InstallPhase, LlamaProgressEvent};
use crate::binary_install::{
    ArchiveLayout, AssetChoice, AssetMatcher, PrebuiltTarget, ReleaseSpec, completed,
    install_prebuilt, retire_checkout, started,
};

/// Check if llama.cpp binaries are installed.
/// Returns true if llama-server exists.
pub fn check_llama_installed() -> bool {
    let server_path = match llama_server_path() {
        Ok(p) => p,
        Err(_) => return false,
    };
    server_path.exists()
}

/// The llama.cpp release gglib installs unless told otherwise.
///
/// # Why a pin rather than `latest`
///
/// Resolving `releases/latest` would change the inference engine underneath
/// gglib whenever upstream cut a release — silently, and differently for two
/// users who installed a day apart. Every compensation gglib applies (dialect
/// normalization, grammar origination, capability detection) is a bet about
/// what that engine does, and an unpinned engine makes those bets
/// unfalsifiable: when behaviour changes, there is no way to tell a gglib
/// regression from an upstream one.
///
/// Pinning does not stop gglib tracking upstream. It makes tracking a
/// deliberate, reviewable event: bump this constant, run the suite, ship the
/// bump as its own commit with the observed differences in the message.
/// CONTRIBUTING's "The llama.cpp pin" has the routine.
pub(super) const PINNED_LLAMA_RELEASE: &str = "b11429";

/// Environment override for [`PINNED_LLAMA_RELEASE`].
///
/// Accepts a release tag (`b10500`) to install that release, or the literal
/// `latest` to follow upstream's current release. That is a `v*` release
/// with no binaries, so the `b<n>` tag its `nightly-tag.txt` names is
/// installed. Unset — the default — installs [`PINNED_LLAMA_RELEASE`].
///
/// Provided because a user debugging against an upstream fix should not have
/// to rebuild gglib to get it, and because it is how the pin bump itself is
/// tested before the constant moves.
pub(super) const LLAMA_RELEASE_ENV: &str = "GGLIB_LLAMA_RELEASE";

/// Where llama.cpp's archive is downloaded to. The installer removes the
/// whole directory once the archive is unpacked.
fn llama_download_dir() -> Result<PathBuf> {
    Ok(data_root()?.join("downloads"))
}

/// Whether an archive member named `file_name` is extracted.
///
/// Licences, C headers and Metal shader sources are left in the archive.
fn wanted(file_name: &str) -> bool {
    !(file_name.starts_with("LICENSE")
        || file_name.ends_with(".h")
        || file_name.ends_with(".metal"))
}

/// llama.cpp as a GitHub release: `ggml-org/llama.cpp`, tar.gz archives one
/// directory deep on macOS and Linux and flat zips on Windows, the first
/// asset whose name holds the platform's pattern.
pub(crate) static LLAMA_RELEASE: ReleaseSpec = ReleaseSpec {
    product: "llama.cpp",
    repo: "ggml-org/llama.cpp",
    pinned: PINNED_LLAMA_RELEASE,
    env: LLAMA_RELEASE_ENV,
    download_dir: llama_download_dir,
    archive: ArchiveLayout::OneDirDeep,
    choice: AssetChoice::First,
    wanted,
};

/// The archive members a llama.cpp install must find.
#[cfg(target_os = "windows")]
const LLAMA_REQUIRED: &[&str] = &["llama-server.exe"];
/// The archive members a llama.cpp install must find.
#[cfg(not(target_os = "windows"))]
const LLAMA_REQUIRED: &[&str] = &["llama-server"];

/// Said when the build installed runs on the CPU alone.
pub(crate) const LLAMA_CPU_WARNING: &str = "No GPU runtime gglib can use was found, so the CPU \
     build of llama.cpp is installed. Replies will be slow.";

/// The name the release's CUDA runtime package holds.
const CUDART_PATTERN: &str = "cudart-llama-bin-win-cuda";

/// llama.cpp's archive for the platform whose asset pattern is
/// `asset_pattern`: a name starting `llama-` and ending in the pattern.
///
/// The prefix is what keeps the Windows CUDA runtime package out:
/// `cudart-llama-bin-win-cuda-12.4-x64.zip` ends in the CUDA archive's
/// pattern too, and the release lists it first.
fn llama_matcher(asset_pattern: &str) -> AssetMatcher<'_> {
    AssetMatcher {
        starts_with: Some("llama-"),
        contains: &[],
        ends_with: Some(asset_pattern),
    }
}

/// Result of checking pre-built binary availability
#[derive(Debug)]
pub enum PrebuiltAvailability {
    /// Pre-built binaries are available for this platform
    Available {
        /// The asset filename pattern to download
        asset_pattern: String,
        /// Description for user-facing messages
        description: String,
        /// What to tell the user before installing it: set for a build that
        /// runs on the CPU alone.
        warning: Option<&'static str>,
    },
    /// Pre-built binaries are not available (must build from source)
    NotAvailable {
        /// Reason why pre-built is not available
        reason: String,
    },
}

/// This machine's pre-built llama.cpp: its platform, with the GPU detected
/// where a platform has more than one build.
pub fn check_prebuilt_availability() -> PrebuiltAvailability {
    llama_platform_asset(
        std::env::consts::OS,
        std::env::consts::ARCH,
        &crate::system::gpu::gpu_for_downloads(),
    )
}

/// The pre-built llama.cpp for `os` and `arch` (`std::env::consts`
/// spellings), with `gpu` deciding between a platform's builds.
///
/// - macOS: the Metal build for the architecture.
/// - Linux `x86_64` and `aarch64`: the Vulkan build when a Vulkan GPU is there,
///   else the CPU build, with a warning. NVIDIA's CUDA builds come with a
///   440 MB runtime for one CUDA version; a CUDA build is made from source
///   with `--build`.
/// - Windows `x86_64`: CUDA 12.4 for an NVIDIA GPU with CUDA, else Vulkan,
///   else the CPU build, with a warning. Windows `aarch64`: Vulkan, else the
///   CPU build, with a warning.
/// - Anything else: none, and a source build is the way to install.
pub(crate) fn llama_platform_asset(os: &str, arch: &str, gpu: &GpuInfo) -> PrebuiltAvailability {
    let build = |asset_pattern: &str, description: &str| PrebuiltAvailability::Available {
        asset_pattern: asset_pattern.to_owned(),
        description: description.to_owned(),
        warning: None,
    };
    let cpu = |asset_pattern: &str, description: &str| PrebuiltAvailability::Available {
        asset_pattern: asset_pattern.to_owned(),
        description: description.to_owned(),
        warning: Some(LLAMA_CPU_WARNING),
    };
    match (os, arch) {
        ("macos", "aarch64") => build("bin-macos-arm64.tar.gz", "macOS ARM64 (Metal)"),
        ("macos", "x86_64") => build("bin-macos-x64.tar.gz", "macOS x64 (Metal)"),
        ("linux", "x86_64") if gpu.has_vulkan => {
            build("bin-ubuntu-vulkan-x64.tar.gz", "Linux x64 (Vulkan)")
        }
        ("linux", "x86_64") => cpu("bin-ubuntu-x64.tar.gz", "Linux x64 (CPU)"),
        ("linux", "aarch64") if gpu.has_vulkan => {
            build("bin-ubuntu-vulkan-arm64.tar.gz", "Linux ARM64 (Vulkan)")
        }
        ("linux", "aarch64") => cpu("bin-ubuntu-arm64.tar.gz", "Linux ARM64 (CPU)"),
        ("windows", "x86_64") if gpu.has_nvidia_gpu && gpu.cuda_version.is_some() => {
            build("bin-win-cuda-12.4-x64.zip", "Windows x64 (CUDA 12.4)")
        }
        ("windows", "x86_64") if gpu.has_vulkan => {
            build("bin-win-vulkan-x64.zip", "Windows x64 (Vulkan)")
        }
        ("windows", "x86_64") => cpu("bin-win-cpu-x64.zip", "Windows x64 (CPU)"),
        ("windows", "aarch64") if gpu.has_vulkan => {
            build("bin-win-vulkan-arm64.zip", "Windows ARM64 (Vulkan)")
        }
        ("windows", "aarch64") => cpu("bin-win-cpu-arm64.zip", "Windows ARM64 (CPU)"),
        _ => PrebuiltAvailability::NotAvailable {
            reason: format!("llama.cpp publishes no pre-built build for {os} on {arch}"),
        },
    }
}

/// Download and install pre-built llama.cpp binaries, streaming progress.
///
/// Resolves the pinned llama.cpp release, downloads this platform's archive,
/// extracts `llama-server` and its shared libraries, records the install and
/// verifies the binary landed where the launcher looks for it. Every stage is
/// bracketed by [`LlamaProgressEvent::PhaseStarted`] and
/// [`LlamaProgressEvent::PhaseCompleted`] on `tx`; the download itself also
/// reports bytes, rate and ETA.
///
/// Success ends with [`LlamaProgressEvent::Completed`]. Failure returns `Err`
/// *without* emitting [`LlamaProgressEvent::Failed`] — the surface that owns
/// the channel decides how a failure is worded, exactly as the source-build
/// pipeline leaves it.
///
/// This function knows nothing about terminals, HTTP responses or `WebViews`.
/// The three copies it replaced each knew about one; the pipeline itself is
/// [`install_prebuilt`], shared with every other product gglib installs.
pub async fn download_prebuilt_binaries(tx: mpsc::Sender<LlamaProgressEvent>) -> Result<()> {
    started(&tx, InstallPhase::CheckAvailability).await;
    let (asset_pattern, description) = match check_prebuilt_availability() {
        PrebuiltAvailability::Available {
            asset_pattern,
            description,
            ..
        } => (asset_pattern, description),
        PrebuiltAvailability::NotAvailable { reason } => {
            bail!("Pre-built binaries not available: {reason}");
        }
    };
    completed(&tx, InstallPhase::CheckAvailability).await;

    let server_path = llama_server_path()?;
    let bin_dir = server_path
        .parent()
        .context("llama-server's path has no directory")?;
    // Windows + CUDA only: also download the CUDA runtime DLLs.
    let cuda_runtime =
        (cfg!(target_os = "windows") && asset_pattern.contains("cuda")).then_some(CUDART_PATTERN);
    let target = PrebuiltTarget {
        matcher: llama_matcher(&asset_pattern),
        description: &description,
        required: LLAMA_REQUIRED,
        bin_dir,
        server_path: &server_path,
        cuda_runtime,
    };

    install_prebuilt(
        &LLAMA_RELEASE,
        target,
        |record| InstallRecord::Prebuilt(record).save(&llama_config_path()?),
        &tx,
    )
    .await?;
    // A source build this download replaced leaves its checkout behind.
    if let Err(e) = retire_checkout(&llama_cpp_dir()?) {
        tracing::debug!("the old llama.cpp checkout was not removed: {e:#}");
    }
    Ok(())
}

#[cfg(all(test, unix))]
#[path = "extract_tests.rs"]
mod extract_tests;

#[cfg(test)]
#[path = "platform_tests.rs"]
mod platform_tests;

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::binary_install::{ReleaseSelector, selector_from_override};

    /// llama.cpp's archive extraction, as the installer runs it.
    pub(super) fn extract_binaries(
        archive: &std::path::Path,
        bin_dir: &std::path::Path,
    ) -> Result<()> {
        crate::binary_install::extract_binaries(&LLAMA_RELEASE, LLAMA_REQUIRED, archive, bin_dir)
    }

    /// A source build clones the repository the downloads come from.
    #[test]
    fn the_source_is_cloned_from_the_release_repository() {
        assert_eq!(
            crate::llama::install::LLAMA_SOURCE.url,
            format!("https://github.com/{}", LLAMA_RELEASE.repo)
        );
    }

    /// The case the pin exists for: no override installs the pin, not
    /// whatever upstream cut this morning.
    #[test]
    fn unset_override_resolves_to_the_pin() {
        assert_eq!(
            selector_from_override(&LLAMA_RELEASE, ""),
            ReleaseSelector::Tag(PINNED_LLAMA_RELEASE.to_owned())
        );
    }

    /// A blank value is unset, not an empty tag — an empty tag would build a
    /// URL ending in `/tags/` that can never resolve.
    #[test]
    fn blank_override_resolves_to_the_pin() {
        assert_eq!(
            selector_from_override(&LLAMA_RELEASE, "   \t "),
            ReleaseSelector::Tag(PINNED_LLAMA_RELEASE.to_owned())
        );
    }

    #[test]
    fn latest_override_floats_with_upstream() {
        assert_eq!(
            selector_from_override(&LLAMA_RELEASE, "latest"),
            ReleaseSelector::Latest
        );
        assert_eq!(
            selector_from_override(&LLAMA_RELEASE, "  LATEST "),
            ReleaseSelector::Latest
        );
    }

    #[test]
    fn a_tag_override_is_taken_verbatim() {
        assert_eq!(
            selector_from_override(&LLAMA_RELEASE, " b10500 "),
            ReleaseSelector::Tag("b10500".to_owned())
        );
    }

    /// The two selectors must hit different GitHub endpoints — a tag resolved
    /// through the `latest` URL would silently install the wrong release.
    #[test]
    fn selectors_resolve_to_distinct_endpoints() {
        let api = crate::binary_install::GITHUB_API;
        let tag = ReleaseSelector::Tag("b10327".to_owned());
        assert_eq!(
            tag.api_url(api, LLAMA_RELEASE.repo),
            "https://api.github.com/repos/ggml-org/llama.cpp/releases/tags/b10327"
        );
        assert_eq!(
            ReleaseSelector::Latest.api_url(api, LLAMA_RELEASE.repo),
            "https://api.github.com/repos/ggml-org/llama.cpp/releases/latest"
        );
    }

    /// A pin upstream no longer publishes is reported as a missing release
    /// that names the way out, word for word as it always has been.
    #[tokio::test]
    async fn a_pin_upstream_no_longer_publishes_names_the_override() {
        use crate::binary_install::fake_github::FakeGitHub;
        let fake = FakeGitHub::serve().await;
        let tmp = tempfile::tempdir().expect("a temp dir");
        let server = tmp.path().join("llama-server");
        let target = PrebuiltTarget {
            matcher: llama_matcher("bin-macos-arm64.tar.gz"),
            description: "macOS ARM64 (Metal)",
            required: LLAMA_REQUIRED,
            bin_dir: tmp.path(),
            server_path: &server,
            cuda_runtime: None,
        };
        let (tx, _rx) = mpsc::channel(64);

        let err = crate::binary_install::install_prebuilt_from(
            &fake.base,
            &ReleaseSelector::Tag("b1".to_owned()),
            &LLAMA_RELEASE,
            target,
            |_| panic!("nothing is recorded"),
            &tx,
        )
        .await
        .expect_err("the tag is not there");

        assert_eq!(
            err.to_string(),
            "llama.cpp release 'b1' not found upstream. \
             Set GGLIB_LLAMA_RELEASE=latest to install the current release, \
             or GGLIB_LLAMA_RELEASE=<tag> to name a different one."
        );
        assert_eq!(fake.asked(), ["/repos/ggml-org/llama.cpp/releases/tags/b1"]);
    }

    /// Every asset of llama.cpp's b11429 release, in the order GitHub lists
    /// them.
    pub(super) const B11429_ASSETS: &[&str] = &[
        "cudart-llama-b11429-bin-ubuntu-cuda-12.8-x64.tar.gz",
        "cudart-llama-b11429-bin-ubuntu-cuda-13.4-arm64.tar.gz",
        "cudart-llama-b11429-bin-ubuntu-cuda-13.4-x64.tar.gz",
        "cudart-llama-bin-win-cuda-12.4-x64.zip",
        "cudart-llama-bin-win-cuda-13.4-arm64.zip",
        "cudart-llama-bin-win-cuda-13.4-x64.zip",
        "llama-b11429-bin-android-arm64-snapdragon.tar.gz",
        "llama-b11429-bin-android-arm64.tar.gz",
        "llama-b11429-bin-linux-arm64-snapdragon.tar.gz",
        "llama-b11429-bin-macos-arm64.tar.gz",
        "llama-b11429-bin-macos-x64.tar.gz",
        "llama-b11429-bin-ubuntu-arm64.tar.gz",
        "llama-b11429-bin-ubuntu-cuda-12.8-x64.tar.gz",
        "llama-b11429-bin-ubuntu-cuda-13.4-arm64.tar.gz",
        "llama-b11429-bin-ubuntu-cuda-13.4-x64.tar.gz",
        "llama-b11429-bin-ubuntu-openvino-2026.4.1-x64.tar.gz",
        "llama-b11429-bin-ubuntu-rocm-10.0-x64.tar.gz",
        "llama-b11429-bin-ubuntu-s390x.tar.gz",
        "llama-b11429-bin-ubuntu-sycl-fp16-x64.tar.gz",
        "llama-b11429-bin-ubuntu-sycl-fp32-x64.tar.gz",
        "llama-b11429-bin-ubuntu-vulkan-arm64.tar.gz",
        "llama-b11429-bin-ubuntu-vulkan-x64.tar.gz",
        "llama-b11429-bin-ubuntu-x64.tar.gz",
        "llama-b11429-bin-win-cpu-arm64.zip",
        "llama-b11429-bin-win-cpu-x64.zip",
        "llama-b11429-bin-win-cuda-12.4-x64.zip",
        "llama-b11429-bin-win-cuda-13.4-arm64.zip",
        "llama-b11429-bin-win-cuda-13.4-x64.zip",
        "llama-b11429-bin-win-opencl-adreno-arm64.zip",
        "llama-b11429-bin-win-openvino-2026.4.1-x64.zip",
        "llama-b11429-bin-win-rocm-10.0-x64.zip",
        "llama-b11429-bin-win-sycl-x64.zip",
        "llama-b11429-bin-win-vulkan-arm64.zip",
        "llama-b11429-bin-win-vulkan-x64.zip",
        "llama-b11429-ui.tar.gz",
        "llama-b11429-xcframework.zip",
    ];

    /// The pin has to be a real tag shape; a stray `v` prefix or a bare
    /// number would 404 at install time on a user's machine, not here.
    #[test]
    fn the_pin_is_a_well_formed_build_tag() {
        let rest = PINNED_LLAMA_RELEASE
            .strip_prefix('b')
            .expect("pin must be a b-prefixed build tag");
        assert!(
            !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()),
            "pin must be b<digits>, got {PINNED_LLAMA_RELEASE}"
        );
    }

    #[test]
    fn test_check_prebuilt_availability() {
        let availability = check_prebuilt_availability();
        // Just verify it doesn't panic and returns a valid variant
        match availability {
            PrebuiltAvailability::Available { .. } => {}
            PrebuiltAvailability::NotAvailable { .. } => {}
        }
    }

    /// Verify that `extract_binaries_tar_gz` correctly handles modern llama.cpp
    /// release archives where binaries live one level inside a versioned directory
    /// (e.g. `llama-b8223/llama-server`, `llama-b8223/llama-cli`), and that
    /// dangling dylib symlinks (versioned aliases present in real macOS archives)
    /// do not cause a spurious "No such file or directory" error.
    #[test]
    fn test_extract_binaries_tar_gz_modern_layout() {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use std::fs::File;
        use tar::Builder;

        let tmp = tempfile::tempdir().expect("failed to create temp dir");
        let archive_path = tmp.path().join("llama-b9999-bin-test.tar.gz");
        let bin_dir = tmp.path().join("bin");

        // Build a minimal tar.gz with the modern llama-b<tag>/<file> layout,
        // including a symlink entry whose target is not in the archive (dangling).
        // Real macOS llama.cpp releases contain such versioned-dylib symlinks.
        {
            let archive_file = File::create(&archive_path).expect("failed to create archive file");
            let gz = GzEncoder::new(archive_file, Compression::fast());
            let mut tar = Builder::new(gz);

            // Regular files
            let entries: &[(&str, &[u8])] = &[
                ("llama-b9999/llama-server", b"#!/bin/sh\necho server"),
                ("llama-b9999/llama-cli", b"#!/bin/sh\necho cli"),
                (
                    "llama-b9999/libggml-metal.0.dylib",
                    b"\x7fELF placeholder dylib",
                ),
                // Top-level directory entry — must be skipped by component-count guard
                ("llama-b9999/", b""),
            ];

            for (name, content) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_size(content.len() as u64);
                header.set_mode(0o755);
                header.set_cksum();
                tar.append_data(&mut header, name, *content as &[u8])
                    .unwrap();
            }

            // Symlink entry: libggml.dylib -> libggml-metal.0.dylib
            // The target is NOT included in this archive (dangling symlink).
            // Before the symlink_metadata fix this caused ENOENT via fs::metadata.
            let mut link_header = tar::Header::new_gnu();
            link_header.set_entry_type(tar::EntryType::Symlink);
            link_header.set_size(0);
            link_header.set_mode(0o777);
            link_header.set_link_name("libggml-metal.0.dylib").unwrap();
            link_header.set_cksum();
            tar.append_data(&mut link_header, "llama-b9999/libggml.dylib", &b""[..])
                .unwrap();

            tar.finish().unwrap();
        }

        // Must not fail — the dangling symlink must be handled gracefully.
        extract_binaries(&archive_path, &bin_dir)
            .expect("extract_binaries_tar_gz should succeed even with dangling symlink entries");

        assert!(
            bin_dir.join("llama-server").exists(),
            "llama-server should be extracted"
        );
        assert!(
            bin_dir.join("libggml-metal.0.dylib").exists(),
            "dylib should be extracted"
        );
        // The symlink itself should be present (its target being missing is fine)
        assert!(
            bin_dir.join("libggml.dylib").symlink_metadata().is_ok(),
            "symlink entry should be extracted"
        );
    }
}
