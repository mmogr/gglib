//! The install end to end against a loopback GitHub listing the pinned
//! release's real asset names, what an archive must hold, and the source
//! build's command lines.

use std::cell::RefCell;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use gglib_core::utils::system::GpuInfo;
use tokio::sync::mpsc;

use super::install::{sd_build_args, sd_clone_args, sd_cmake_flag, sd_configure_args};
use super::release::{SD_RELEASE, sd_platform_asset};
use crate::binary_install::fake_github::{FakeGitHub, Reply};
use crate::binary_install::{
    PrebuiltRecord, PrebuiltTarget, ReleaseSelector, ReleaseSpec, extract_binaries,
    install_prebuilt_from,
};
use crate::llama::{Acceleration, LlamaProgressEvent};

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

thread_local! {
    /// This test's download directory, in place of `.sd/downloads`, so two
    /// installs running at once do not remove each other's archive.
    static DOWNLOADS: RefCell<PathBuf> = const { RefCell::new(PathBuf::new()) };
}

#[allow(
    clippy::unnecessary_wraps,
    reason = "a ReleaseSpec's download_dir is fallible"
)]
fn test_downloads() -> anyhow::Result<PathBuf> {
    Ok(DOWNLOADS.with(|d| d.borrow().clone()))
}

/// stable-diffusion.cpp's release as installed, downloading into this
/// test's own directory.
fn spec(downloads: &Path) -> ReleaseSpec {
    DOWNLOADS.with(|d| *d.borrow_mut() = downloads.to_owned());
    ReleaseSpec {
        download_dir: test_downloads,
        ..SD_RELEASE
    }
}

const LISTING: &str = "/repos/leejet/stable-diffusion.cpp/releases/tags/master-948-228c707";

/// What the macOS asset holds, flat, as `zip -j` packed it.
const MACOS_ZIP: [&str; 5] = [
    "sd-server",
    "sd-cli",
    "libstable-diffusion.dylib",
    "ggml.txt",
    "stable-diffusion.cpp.txt",
];

fn no_gpu() -> GpuInfo {
    GpuInfo {
        has_nvidia_gpu: false,
        cuda_version: None,
        has_metal: false,
        has_vulkan: false,
        vulkan_headers: false,
        vulkan_glslc: false,
        vulkan_spirv_headers: false,
    }
}

fn zip_of(names: &[&str]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for name in names {
        zip.start_file(*name, options).expect("a member");
        zip.write_all(b"fixture").expect("its bytes");
    }
    zip.finish().expect("finish the zip").into_inner()
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read the directory")
        .map(|e| {
            e.expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// A fake serving the pinned release with every real asset name; each
/// archive is `zip`.
async fn serve_pinned(zip: &[u8]) -> FakeGitHub {
    let fake = FakeGitHub::serve().await;
    let listed: Vec<serde_json::Value> = ASSETS
        .iter()
        .map(|name| {
            fake.route(&format!("/download/{name}"), Reply::ok(zip.to_vec()));
            serde_json::json!({
                "name": name,
                "browser_download_url": format!("{}/download/{name}", fake.base),
            })
        })
        .collect();
    let body =
        serde_json::json!({ "tag_name": "master-948-228c707", "assets": listed }).to_string();
    fake.route(LISTING, Reply::ok(body));
    fake
}

/// Install `os`/`arch`'s asset from `fake` into `<root>/bin`, downloading
/// into `<root>/downloads`; the result and the record handed over.
async fn install(
    fake: &FakeGitHub,
    os: &str,
    arch: &str,
    root: &Path,
) -> (anyhow::Result<()>, Option<PrebuiltRecord>) {
    let bin = root.join("bin");
    let spec = spec(&root.join("downloads"));
    let asset = sd_platform_asset(os, arch, &no_gpu()).expect("an asset");
    let server = bin.join(asset.required[0]);
    let target = PrebuiltTarget {
        matcher: asset.matcher,
        description: asset.description,
        required: asset.required,
        bin_dir: &bin,
        server_path: &server,
        cuda_runtime: asset.cuda_runtime,
    };
    let (tx, _rx) = mpsc::channel::<LlamaProgressEvent>(256);
    let mut record = None;
    let result = install_prebuilt_from(
        &fake.base,
        &ReleaseSelector::Tag("master-948-228c707".to_owned()),
        &spec,
        target,
        |r| {
            record = Some(r);
            Ok(())
        },
        &tx,
    )
    .await;
    (result, record)
}

#[tokio::test]
async fn the_macos_asset_installs_its_server_and_library_and_nothing_else() {
    let root = tempfile::tempdir().expect("a temp dir");
    let fake = serve_pinned(&zip_of(&MACOS_ZIP)).await;

    let (result, record) = install(&fake, "macos", "aarch64", root.path()).await;

    result.expect("the install succeeds");
    assert_eq!(
        fake.asked(),
        [
            LISTING.to_owned(),
            "/download/sd-master-228c707-bin-Darwin-macOS-26.6.2-arm64.zip".to_owned(),
        ]
    );
    assert_eq!(
        names_in(&root.path().join("bin")),
        ["libstable-diffusion.dylib", "sd-server"]
    );
    let record = record.expect("a record");
    assert_eq!(record.version, "master-948-228c707");
    assert_eq!(record.platform, "macOS universal (Metal)");
    assert!(
        !root.path().join("downloads").exists(),
        "the archive was left behind"
    );
}

/// With every real name listed, the Linux CPU pick downloads the CPU zip and
/// neither of the two GPU builds whose names also hold `-x86_64`.
#[tokio::test]
async fn the_linux_cpu_pick_downloads_the_cpu_zip_among_the_real_names() {
    let root = tempfile::tempdir().expect("a temp dir");
    let fake = serve_pinned(&zip_of(&["sd-server", "libstable-diffusion.so"])).await;

    let (result, _) = install(&fake, "linux", "x86_64", root.path()).await;

    result.expect("the install succeeds");
    assert_eq!(
        fake.asked()[1],
        "/download/sd-master-228c707-bin-Linux-Ubuntu-24.04-x86_64.zip"
    );
}

#[test]
fn a_flat_archive_without_the_library_is_refused() {
    let tmp = tempfile::tempdir().expect("a temp dir");
    let archive = tmp
        .path()
        .join("sd-master-228c707-bin-Darwin-macOS-26.6.2-arm64.zip");
    std::fs::write(&archive, zip_of(&["sd-server", "sd-cli", "ggml.txt"])).unwrap();
    let required = sd_platform_asset("macos", "aarch64", &no_gpu())
        .unwrap()
        .required;
    let bin = tmp.path().join("bin");

    let err = extract_binaries(&SD_RELEASE, required, &archive, &bin)
        .expect_err("no dylib in the archive");

    assert_eq!(
        err.to_string(),
        "Failed to extract all required binaries. Found 1 of 2"
    );
    assert_eq!(names_in(&bin), ["sd-server"]);
}

#[test]
fn a_flat_archive_with_both_unpacks_them_beside_each_other() {
    let tmp = tempfile::tempdir().expect("a temp dir");
    let archive = tmp.path().join("a.zip");
    std::fs::write(&archive, zip_of(&MACOS_ZIP)).unwrap();
    let bin = tmp.path().join("bin");

    extract_binaries(
        &SD_RELEASE,
        &["sd-server", "libstable-diffusion.dylib"],
        &archive,
        &bin,
    )
    .expect("both are there");

    assert_eq!(names_in(&bin), ["libstable-diffusion.dylib", "sd-server"]);
}

#[test]
fn the_source_build_clones_the_pinned_tag_shallow_with_its_submodules() {
    let dir = PathBuf::from("/x/.sd/stable-diffusion.cpp");
    assert_eq!(
        sd_clone_args(&ReleaseSelector::Tag("master-948-228c707".into()), &dir),
        [
            "clone",
            "--depth",
            "1",
            "--branch",
            "master-948-228c707",
            "--recurse-submodules",
            "--shallow-submodules",
            "https://github.com/leejet/stable-diffusion.cpp",
            "/x/.sd/stable-diffusion.cpp",
        ]
    );
    assert_eq!(
        sd_clone_args(&ReleaseSelector::Latest, &dir),
        [
            "clone",
            "--depth",
            "1",
            "--recurse-submodules",
            "--shallow-submodules",
            "https://github.com/leejet/stable-diffusion.cpp",
            "/x/.sd/stable-diffusion.cpp",
        ]
    );
}

#[test]
fn the_source_build_configures_the_server_alone_with_its_backend_on() {
    let (src, build) = (PathBuf::from("/s"), PathBuf::from("/s/build"));
    let base = [
        "-S",
        "/s",
        "-B",
        "/s/build",
        "-DCMAKE_BUILD_TYPE=Release",
        "-DSD_SERVER_BUILD_FRONTEND=OFF",
    ];
    for (acceleration, flag) in [
        (Acceleration::Metal, Some("-DSD_METAL=ON")),
        (Acceleration::Cuda, Some("-DSD_CUDA=ON")),
        (Acceleration::Vulkan, Some("-DSD_VULKAN=ON")),
        (Acceleration::Cpu, None),
    ] {
        assert_eq!(sd_cmake_flag(acceleration), flag);
        let mut expected: Vec<&str> = base.to_vec();
        expected.extend(flag);
        assert_eq!(
            sd_configure_args(&src, &build, acceleration),
            expected,
            "{acceleration}"
        );
    }
    assert_eq!(
        sd_build_args(&build, 12),
        [
            "--build",
            "/s/build",
            "--config",
            "Release",
            "--target",
            "sd-server",
            "-j",
            "12"
        ]
    );
}
