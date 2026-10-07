//! How an install is chosen, and how the choice is carried out.

use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

fn available() -> PrebuiltAvailability {
    PrebuiltAvailability::Available {
        asset_pattern: "bin-macos-arm64".to_owned(),
        description: "macOS ARM64 (Metal)".to_owned(),
    }
}

fn not_available() -> PrebuiltAvailability {
    PrebuiltAvailability::NotAvailable {
        reason: "Linux requires building from source for CUDA support".to_owned(),
    }
}

const BUILD: InstallMethod = InstallMethod::Source { no_prebuilt: None };

fn download() -> InstallMethod {
    InstallMethod::Prebuilt {
        description: "macOS ARM64 (Metal)".to_owned(),
    }
}

/// The installed gglib on each platform, with no flags: a download where the
/// platform has a release, and a build, with the platform's reason, where it
/// has none.
#[test]
fn an_installed_gglib_downloads_where_its_platform_has_a_release() {
    let detect = AccelerationFlags::default();

    assert_eq!(
        choose_install_method(false, detect, false, available),
        download()
    );
    assert_eq!(
        choose_install_method(false, detect, false, not_available),
        InstallMethod::Source {
            no_prebuilt: Some("Linux requires building from source for CUDA support".to_owned()),
        }
    );
}

/// The choice over every combination of flags, where gglib runs from and
/// what the platform offers, against the rule as `config llama install` has
/// always applied it: a build if `--build`, or a checkout, or any
/// acceleration flag, or no release for the platform.
#[test]
fn the_choice_is_a_build_exactly_when_a_flag_a_checkout_or_the_platform_says_so() {
    for bits in 0u8..64 {
        let [build, cuda, metal, vulkan, from_checkout, has_release] =
            [0, 1, 2, 3, 4, 5].map(|bit| bits & (1 << bit) != 0);
        let flags = AccelerationFlags {
            cuda,
            metal,
            vulkan,
        };

        let chosen = choose_install_method(
            build,
            flags,
            from_checkout,
            if has_release {
                available
            } else {
                not_available
            },
        );

        let builds = build || from_checkout || cuda || metal || vulkan || !has_release;
        assert_eq!(
            matches!(chosen, InstallMethod::Source { .. }),
            builds,
            "--build: {build}, {flags:?}, from a checkout: {from_checkout}, a release: {has_release}"
        );
    }
}

/// A flag or a checkout decides before the platform is asked, and asking
/// runs GPU detection on Windows.
#[test]
fn the_platform_is_not_asked_once_a_flag_or_a_checkout_has_decided() {
    let never = || -> PrebuiltAvailability { panic!("the platform was asked") };

    let detect = AccelerationFlags::default();
    let vulkan = AccelerationFlags {
        vulkan: true,
        ..detect
    };
    assert_eq!(choose_install_method(true, detect, false, never), BUILD);
    assert_eq!(choose_install_method(false, vulkan, false, never), BUILD);
    assert_eq!(choose_install_method(false, detect, true, never), BUILD);
}

/// Which of the two steps `install_by` ran for `method`, when the download
/// answers `download_result`.
async fn steps_run(method: &InstallMethod, download_result: Result<()>) -> (bool, bool, bool) {
    let (downloaded, built) = (AtomicBool::new(false), AtomicBool::new(false));

    let outcome = install_by(
        method,
        async || {
            downloaded.store(true, Ordering::Relaxed);
            download_result
        },
        async || {
            built.store(true, Ordering::Relaxed);
            Ok(())
        },
    )
    .await;

    (
        downloaded.load(Ordering::Relaxed),
        built.load(Ordering::Relaxed),
        outcome.is_ok(),
    )
}

#[tokio::test]
async fn a_download_that_works_is_the_whole_install() {
    assert_eq!(steps_run(&download(), Ok(())).await, (true, false, true));
}

#[tokio::test]
async fn a_download_that_fails_falls_back_to_a_build() {
    let failed = Err(anyhow::anyhow!("no route to host"));
    assert_eq!(steps_run(&download(), failed).await, (true, true, true));
}

#[tokio::test]
async fn a_source_install_downloads_nothing() {
    assert_eq!(steps_run(&BUILD, Ok(())).await, (false, true, true));
}

/// The fallback's own failure is the install's: nothing swallows it.
#[tokio::test]
async fn a_build_that_fails_after_a_failed_download_fails_the_install() {
    let outcome = install_by(
        &download(),
        async || anyhow::bail!("no route to host"),
        async || anyhow::bail!("Missing required build dependencies"),
    )
    .await;

    assert_eq!(
        outcome.unwrap_err().to_string(),
        "Missing required build dependencies"
    );
}
