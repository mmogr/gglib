//! Which repair a failure calls for, with its words. The failure is the
//! value `failure_kind` reads from how a run ended, built here as every OS
//! reads it, and the OS is a value, so the Windows rows run on macOS and
//! Linux too.

use gglib_core::domain::RuntimeKind;
use gglib_core::host::HostOs;
use gglib_core::utils::process::bounded::{Ended, Failure, failure_kind};

use super::{Fix, FixOffer, RuntimeState, fix_for};

fn not_asked() -> RuntimeState {
    panic!("the runtime is not asked about")
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
/// offered the runtime, with Repair, in words that say Windows will ask
/// and where its question may be, before and while it waits: the service
/// asks, so Windows may leave the question on the taskbar.
#[test]
fn a_missing_windows_library_without_the_runtime_offers_the_runtime() {
    let fix = fix_for(&windows_dll_not_found(), HostOs::Windows, || {
        RuntimeState::Missing
    });

    assert_eq!(fix, Some(Fix::InstallVisualCppRuntime));
    assert_eq!(
        FixOffer::new(Fix::InstallVisualCppRuntime, RuntimeKind::Llama),
        FixOffer {
            fix: Fix::InstallVisualCppRuntime,
            offer: "gglib can install the Microsoft Visual C++ Redistributable (x64) from \
                    Microsoft. Windows will ask for permission; if no question appears, look \
                    for a flashing shield on the taskbar."
                .to_owned(),
            action: Some("Repair".to_owned()),
            waiting: Some(
                "Waiting for Windows' permission to install the Microsoft Visual C++ \
                 Redistributable (x64); if no question appears, look for a flashing shield on \
                 the taskbar."
                    .to_owned()
            ),
        }
    );
    assert_eq!(Fix::Reinstall.waiting(), None);
    assert_eq!(Fix::RepairVisualCppRuntime.waiting(), None);
}

/// With the runtime there, the DLL Windows could not find came with the
/// download, so the repair is a reinstall, in the engine's name.
#[test]
fn a_missing_windows_library_with_the_runtime_there_is_a_reinstall() {
    assert_eq!(
        fix_for(&windows_dll_not_found(), HostOs::Windows, || {
            RuntimeState::Installed
        }),
        Some(Fix::Reinstall)
    );
    assert_eq!(
        Fix::Reinstall.offer(RuntimeKind::StableDiffusion),
        "gglib can download stable-diffusion.cpp again and reinstall it."
    );
}

/// A runtime Windows lists as installed whose files are gone is damaged:
/// the fix is Windows' repair of it, said with where it is, and gglib
/// offers no button that would only install it again.
#[test]
fn a_damaged_runtime_is_repaired_in_windows_not_offered_again() {
    let fix = fix_for(&windows_dll_not_found(), HostOs::Windows, || {
        RuntimeState::Damaged
    });

    assert_eq!(fix, Some(Fix::RepairVisualCppRuntime));
    let offer = FixOffer::new(Fix::RepairVisualCppRuntime, RuntimeKind::Llama);
    assert_eq!(offer.action, None);
    assert_eq!(
        offer.offer,
        "The Microsoft Visual C++ Redistributable (x64) is installed but damaged: its files are \
         missing. Repair it in Windows: Settings › Apps › Installed apps › Microsoft Visual C++ \
         v14 Redistributable (x64) (2015-2022 on an older one) › Modify › Repair, then check \
         again."
    );
}

/// Off Windows the runtime is never offered, and never looked for.
#[test]
fn the_runtime_is_never_offered_off_windows() {
    for os in [HostOs::MacOs, HostOs::Linux] {
        assert_eq!(
            fix_for(&windows_dll_not_found(), os, not_asked),
            None,
            "{os:?}"
        );
    }
}

/// A damaged binary is reinstalled; a named library, which may be the
/// system's, one the system has too old a copy of, a time-out and a
/// refusal to start have no repair.
#[test]
fn other_failures_carry_a_reinstall_or_nothing() {
    let reinstall = [
        Failure::NotExecutable,
        Failure::Crashed { code: None },
        Failure::Failed { code: 1 },
    ];
    for failure in reinstall {
        assert_eq!(
            fix_for(&failure, HostOs::Windows, not_asked),
            Some(Fix::Reinstall),
            "{failure:?}"
        );
    }
    let none = [
        Failure::MissingLibrary {
            name: Some("libvulkan.so.1".to_owned()),
        },
        Failure::BadLibrary { name: None },
        Failure::NewerLibraryNeeded {
            name: "libm.so.6".to_owned(),
            version: "GLIBC_2.43".to_owned(),
        },
        Failure::TimedOut,
        Failure::CouldNotStart {
            reason: "Access is denied.".to_owned(),
        },
    ];
    for failure in none {
        for os in [HostOs::Windows, HostOs::Linux, HostOs::MacOs] {
            assert_eq!(fix_for(&failure, os, not_asked), None, "{failure:?} {os:?}");
        }
    }
}
