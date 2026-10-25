#![doc = include_str!("README.md")]
use super::build::{BuildInfo, build_llama_cpp};
use super::build_events::{BuildEvent, BuildPhase};
use super::config::{BuildConfig, InstallRecord};
use super::detect::Acceleration;
use super::download::LLAMA_RELEASE;
use crate::binary_install::{SourceRepo, prepare_checkout, resolve_selector, settle};
use anyhow::{Context, Result, bail};
use gglib_core::paths::{llama_bench_path, llama_config_path};
use std::fs;
use std::path::PathBuf;
use tokio::sync::mpsc;

/// Where llama.cpp's source is cloned from: the repository its releases are
/// published from.
pub(crate) const LLAMA_SOURCE: SourceRepo<'static> = SourceRepo {
    product: "llama.cpp",
    url: "https://github.com/ggml-org/llama.cpp",
    submodules: false,
};

/// Core streaming build pipeline for llama.cpp from source.
///
/// Builds the release the install selects, the pin unless
/// `GGLIB_LLAMA_RELEASE` names another: a checkout already at it is reused,
/// any other is replaced by a clone of its tag (see [`prepare_checkout`]).
/// Then configures, compiles, installs the binaries and saves the build's
/// record. All progress is emitted as [`BuildEvent`] values on `tx`.
///
/// This function has no CLI concerns (no user prompts, no dependency checks). Callers
/// must perform pre-flight validation before calling this.
///
/// # Threading
///
/// Blocking subprocess work ([`prepare_checkout`], [`build_llama_cpp`]) runs inside
/// [`tokio::task::spawn_blocking`] so the Tokio executor is never blocked and
/// `blocking_send` calls are always on OS threads.
pub async fn run_llama_source_build(
    acceleration: Acceleration,
    llama_dir: PathBuf,
    server_path: PathBuf,
    tx: mpsc::Sender<BuildEvent>,
) -> Result<()> {
    // Step 1: The checkout, at the selected release.
    let selector = resolve_selector(&LLAMA_RELEASE);
    let checkout = {
        let tx = tx.clone();
        tokio::task::spawn_blocking(move || {
            prepare_checkout(&LLAMA_SOURCE, &selector, &llama_dir, &tx)
        })
        .await??
    };

    // Step 2: Configure and compile.
    let info = BuildInfo::of(checkout.tag.as_deref(), &checkout.short_sha);
    {
        let tx_clone = tx.clone();
        let dir = checkout.dir.clone();
        tokio::task::spawn_blocking(move || {
            build_llama_cpp(&dir, acceleration, info.as_ref(), &tx_clone)
        })
        .await??;
    }

    // Step 3: Install binaries.
    {
        let tx_clone = tx.clone();
        let dir = checkout.dir.clone();
        let sp = server_path.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let _ = tx_clone.blocking_send(BuildEvent::PhaseStarted {
                phase: BuildPhase::InstallBinaries,
            });
            install_binary(&dir, "llama-server", &sp)?;
            let bench_dest = llama_bench_path()?;
            install_binary(&dir, "llama-bench", &bench_dest)?;
            let _ = tx_clone.blocking_send(BuildEvent::PhaseCompleted {
                phase: BuildPhase::InstallBinaries,
            });
            Ok(())
        })
        .await??;
    }

    // Step 4: Persist build configuration. The binaries are linked
    // statically, so the checkout they were built in can now replace the old
    // one. The install is done either way, so a checkout that cannot be
    // moved is said, not failed.
    let version = checkout.version.clone();
    let config = BuildConfig::new(version.clone(), checkout.commit_sha.clone(), acceleration);
    InstallRecord::Built(config).save(&llama_config_path()?)?;
    settle(checkout, &tx).await;

    // Step 5: Signal successful completion.
    let _ = tx
        .send(BuildEvent::Completed {
            version,
            acceleration: acceleration.display_name().to_string(),
        })
        .await;

    Ok(())
}

/// Install the binary `binary_name` from the checkout's `build/` directory
/// to `destination`.
pub(crate) fn install_binary(
    llama_dir: &std::path::Path,
    binary_name: &str,
    destination: &std::path::Path,
) -> Result<()> {
    let binary_src_root = llama_dir.join("build");

    #[cfg(target_os = "windows")]
    let relative_binary = format!("bin\\Release\\{binary_name}.exe");

    #[cfg(not(target_os = "windows"))]
    let relative_binary = format!("bin/{binary_name}");

    let binary_src = binary_src_root.join(relative_binary);

    if !binary_src.exists() {
        bail!("Built binary not found at: {}", binary_src.display());
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).context("Failed to create bin directory")?;
    }

    fs::copy(&binary_src, destination).context("Failed to copy binary")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(destination)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(destination, perms)?;
    }

    Ok(())
}
