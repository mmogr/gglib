//! Which repair a failure calls for, with its words. The failure is the
//! value `failure_kind` reads from how a run ended, built here as every OS
//! reads it, so the Windows rows run on macOS and Linux too.

use gglib_core::domain::RuntimeKind;
use gglib_core::utils::process::bounded::{Ended, Failure, failure_kind};

use super::{Fix, FixOffer, fix_for};

const RUNTIME_MISSING: &str = "runtime is not asked about";

fn not_asked() -> bool {
    panic!("{RUNTIME_MISSING}")
}

/// How Windows ends a program whose DLL is missing, read as every OS reads
/// it.
fn windows_dll_not_found() -> Failure {
    failure_kind(Ended::Exited {
        code: Some(0xC000_0135_u32.cast_signed()),
        stderr: "",
    })
}

/// A Windows engine that cannot find a DLL, with the runtime missing, is
/// offered the runtime, in words that say Windows will ask.
#[test]
fn a_missing_windows_library_without_the_runtime_offers_the_runtime() {
    let fix = fix_for(&windows_dll_not_found(), "windows", || false);

    assert_eq!(fix, Some(Fix::InstallVisualCppRuntime));
    assert_eq!(
        FixOffer::new(Fix::InstallVisualCppRuntime, RuntimeKind::Llama),
        FixOffer {
            fix: Fix::InstallVisualCppRuntime,
            offer: "gglib can install the Microsoft Visual C++ Redistributable (x64) from \
                    Microsoft. Windows will ask for permission."
                .to_owned(),
        }
    );
}

/// With the runtime there, the DLL Windows could not find came with the
/// download, so the repair is a reinstall, in the engine's name.
#[test]
fn a_missing_windows_library_with_the_runtime_there_is_a_reinstall() {
    assert_eq!(
        fix_for(&windows_dll_not_found(), "windows", || true),
        Some(Fix::Reinstall)
    );
    assert_eq!(
        Fix::Reinstall.offer(RuntimeKind::StableDiffusion),
        "gglib can download stable-diffusion.cpp again and reinstall it."
    );
}

/// Off Windows the runtime is never offered, and never looked for.
#[test]
fn the_runtime_is_never_offered_off_windows() {
    for os in ["macos", "linux"] {
        assert_eq!(
            fix_for(&windows_dll_not_found(), os, not_asked),
            None,
            "{os}"
        );
    }
}

/// A damaged binary is reinstalled; a named library, which may be the
/// system's, a time-out and a refusal to start have no repair.
#[test]
fn other_failures_carry_a_reinstall_or_nothing() {
    let reinstall = [
        Failure::NotExecutable,
        Failure::Crashed { code: None },
        Failure::Failed { code: 1 },
    ];
    for failure in reinstall {
        assert_eq!(
            fix_for(&failure, "windows", not_asked),
            Some(Fix::Reinstall),
            "{failure:?}"
        );
    }
    let none = [
        Failure::MissingLibrary {
            name: Some("libvulkan.so.1".to_owned()),
        },
        Failure::BadLibrary { name: None },
        Failure::TimedOut,
        Failure::CouldNotStart {
            reason: "Access is denied.".to_owned(),
        },
    ];
    for failure in none {
        assert_eq!(fix_for(&failure, "windows", not_asked), None, "{failure:?}");
        assert_eq!(fix_for(&failure, "linux", not_asked), None, "{failure:?}");
    }
}
