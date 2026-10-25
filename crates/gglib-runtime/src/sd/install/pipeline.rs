//! Installing `sd-server`: the pre-built release, or a source build.

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::Path;
use tokio::sync::mpsc;
use tracing::debug;

use gglib_core::paths::{sd_config_path, sd_cpp_dir, sd_server_path};
use gglib_core::utils::process::cmd;

use super::record::{SdBuildRecord, SdInstallRecord};
use super::release::{SD_RELEASE, check_sd_prebuilt_availability};
use crate::binary_install::{
    PrebuiltTarget, SourceRepo, completed, install_prebuilt, prepare_checkout, resolve_selector,
    settle, started,
};
use crate::llama::build::{build_parallelism, run_compile, run_configure};
use crate::llama::install::install_binary;
use crate::llama::{
    Acceleration, BuildEvent, BuildPhase, InstallPhase, LlamaProgressEvent, build_tools,
    missing_build_tools,
};

/// Where stable-diffusion.cpp's source is cloned from, with the submodules
/// its build needs.
pub(crate) const SD_SOURCE: SourceRepo<'static> = SourceRepo {
    product: "stable-diffusion.cpp",
    url: "https://github.com/leejet/stable-diffusion.cpp",
    submodules: true,
};

/// Download and install the pinned pre-built `sd-server`, streaming progress
/// on `tx` exactly as llama.cpp's install does.
///
/// The asset is this platform's one (see [`check_sd_prebuilt_availability`]);
/// `sd-server` and the shared library it loads from beside itself land in
/// `.sd/bin`, and `sd-config.json` records the download. `Err` without a
/// `Failed` event, as llama.cpp's: the surface words the failure.
pub async fn install_sd_prebuilt(tx: mpsc::Sender<LlamaProgressEvent>) -> Result<()> {
    started(&tx, InstallPhase::CheckAvailability).await;
    let asset = match check_sd_prebuilt_availability() {
        Ok(asset) => asset,
        Err(reason) => bail!("Pre-built binaries not available: {reason}"),
    };
    // Each surface shows the warning itself, the command line before the
    // download and Settings from the status, so it is logged only here.
    if let Some(warning) = asset.warning {
        debug!("{warning}");
    }
    completed(&tx, InstallPhase::CheckAvailability).await;

    let server_path = sd_server_path()?;
    let bin_dir = server_path
        .parent()
        .context("sd-server's path has no directory")?;
    let target = PrebuiltTarget {
        matcher: asset.matcher,
        description: asset.description,
        required: asset.required,
        bin_dir,
        server_path: &server_path,
        cuda_runtime: asset.cuda_runtime,
    };

    install_prebuilt(
        &SD_RELEASE,
        target,
        |record| SdInstallRecord::Prebuilt(record).save(&sd_config_path()?),
        &tx,
    )
    .await
}

/// The `CMake` flag that turns `acceleration` on in stable-diffusion.cpp, which
/// names its backends `SD_*` rather than ggml's `GGML_*`.
pub(crate) const fn sd_cmake_flag(acceleration: Acceleration) -> Option<&'static str> {
    match acceleration {
        Acceleration::Metal => Some("-DSD_METAL=ON"),
        Acceleration::Cuda => Some("-DSD_CUDA=ON"),
        Acceleration::Vulkan => Some("-DSD_VULKAN=ON"),
        Acceleration::Cpu => None,
    }
}

/// The configure step's arguments: a Release build of the checkout at `src`
/// into `build`, the server's web frontend left out (it needs `pnpm`), and
/// `acceleration` on.
pub(crate) fn sd_configure_args(
    src: &Path,
    build: &Path,
    acceleration: Acceleration,
) -> Vec<String> {
    let mut args = vec![
        "-S".to_owned(),
        src.display().to_string(),
        "-B".to_owned(),
        build.display().to_string(),
        "-DCMAKE_BUILD_TYPE=Release".to_owned(),
        "-DSD_SERVER_BUILD_FRONTEND=OFF".to_owned(),
    ];
    args.extend(sd_cmake_flag(acceleration).map(str::to_owned));
    args
}

/// The compile step's arguments: only the `sd-server` target, on `jobs`
/// jobs.
pub(crate) fn sd_build_args(build: &Path, jobs: usize) -> Vec<String> {
    vec![
        "--build".to_owned(),
        build.display().to_string(),
        "--config".to_owned(),
        "Release".to_owned(),
        "--target".to_owned(),
        "sd-server".to_owned(),
        "-j".to_owned(),
        jobs.to_string(),
    ]
}

/// Build `sd-server` from source for `acceleration` and install it,
/// streaming [`BuildEvent`]s on `tx`.
///
/// Checks out the pinned release (or the override's) in
/// `.sd/stable-diffusion.cpp`, reusing a checkout already at it and replacing
/// any other (see [`prepare_checkout`]), configures and compiles only the
/// `sd-server` target, copies the binary to `.sd/bin` and records the build.
/// The build links stable-diffusion.cpp statically, so the one binary is the
/// whole install. The way in when no pre-built asset fits the platform, or
/// when the user asks for a build.
pub async fn run_sd_source_build(
    acceleration: Acceleration,
    tx: mpsc::Sender<BuildEvent>,
) -> Result<()> {
    let missing = missing_build_tools(&build_tools());
    if !missing.is_empty() {
        bail!(
            "Building stable-diffusion.cpp needs: {}",
            missing.join(", ")
        );
    }

    let src = sd_cpp_dir()?;
    let server_path = sd_server_path()?;
    let selector = resolve_selector(&SD_RELEASE);

    let checkout = {
        let tx = tx.clone();
        tokio::task::spawn_blocking(move || prepare_checkout(&SD_SOURCE, &selector, &src, &tx))
            .await??
    };

    {
        let (tx, src) = (tx.clone(), checkout.dir.clone());
        tokio::task::spawn_blocking(move || build_sd(&src, acceleration, &tx)).await??;
    }

    {
        let (tx, src, dest) = (tx.clone(), checkout.dir.clone(), server_path.clone());
        tokio::task::spawn_blocking(move || -> Result<()> {
            let _ = tx.blocking_send(BuildEvent::PhaseStarted {
                phase: BuildPhase::InstallBinaries,
            });
            install_binary(&src, "sd-server", &dest)?;
            let _ = tx.blocking_send(BuildEvent::PhaseCompleted {
                phase: BuildPhase::InstallBinaries,
            });
            Ok(())
        })
        .await??;
    }

    let flags = sd_cmake_flag(acceleration)
        .map(str::to_owned)
        .into_iter()
        .collect();
    let version = checkout.version.clone();
    SdInstallRecord::Built(SdBuildRecord::new(
        version.clone(),
        checkout.commit_sha.clone(),
        acceleration,
        flags,
    ))
    .save(&sd_config_path()?)?;
    settle(checkout, &tx).await;

    let _ = tx
        .send(BuildEvent::Completed {
            version,
            acceleration: acceleration.display_name().to_string(),
        })
        .await;

    Ok(())
}

/// Configure and compile the `sd-server` target; blocking.
fn build_sd(src: &Path, acceleration: Acceleration, tx: &mpsc::Sender<BuildEvent>) -> Result<()> {
    let build = src.join("build");
    fs::create_dir_all(&build).context("Failed to create build directory")?;

    let _ = tx.blocking_send(BuildEvent::PhaseStarted {
        phase: BuildPhase::Configure,
    });
    let mut configure = cmd("cmake");
    configure.args(sd_configure_args(src, &build, acceleration));
    run_configure(configure, tx)?;

    let _ = tx.blocking_send(BuildEvent::PhaseStarted {
        phase: BuildPhase::Compile,
    });
    let mut compile = cmd("cmake");
    compile.args(sd_build_args(&build, build_parallelism(acceleration)));
    run_compile(compile, tx)
}
