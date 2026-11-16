//! Where the runtime stands, from its DLLs and its registry key as values,
//! so every case runs on every OS.

use super::{Dlls, Registered, RuntimeState, dlls_in, read_key, state_of};

const KEY: &str =
    "HKEY_LOCAL_MACHINE\\SOFTWARE\\WOW6432Node\\Microsoft\\VisualStudio\\14.0\\VC\\Runtimes\\x64";

/// `reg query`'s answer for a runtime of `major.minor`.
fn key(major: u32, minor: u32) -> String {
    format!(
        "\r\n{KEY}\r\n    Installed    REG_DWORD    0x1\r\n    Major    REG_DWORD    {major:#x}\r\n    \
         Minor    REG_DWORD    {minor:#x}\r\n    Bld    REG_DWORD    0x7685\r\n    \
         Version    REG_SZ    v{major}.{minor}.30341.00\r\n\r\n"
    )
}

const ALL: Dlls = Dlls {
    core: true,
    newer: true,
};
const CORE_ONLY: Dlls = Dlls {
    core: true,
    newer: false,
};
const NONE: Dlls = Dlls {
    core: false,
    newer: false,
};

/// The key reads as installed, with its version.
#[test]
fn the_key_is_read_with_its_version() {
    assert_eq!(
        read_key(&key(14, 44)),
        Registered {
            installed: true,
            version: Some((14, 44)),
        }
    );
    assert_eq!(read_key(&key(14, 16)).version, Some((14, 16)));
    assert_eq!(
        read_key("ERROR: The system was unable to find the specified registry key or value.\r\n"),
        Registered::default()
    );
    assert!(!read_key(&format!("{KEY}\r\n    Installed    REG_DWORD    0x0\r\n")).installed);
}

/// Only 2015's or 2017's runtime (14.0x, 14.1x), which never had
/// `vcruntime140_1.dll`: not damaged, offered the install, which upgrades.
#[test]
fn a_2015_or_2017_runtime_is_offered_the_install() {
    for minor in [0, 10, 16] {
        assert_eq!(
            state_of(CORE_ONLY, read_key(&key(14, minor))),
            RuntimeState::Missing,
            "14.{minor}"
        );
    }
}

/// A runtime of 14.30 or later that Windows lists and whose DLLs are not
/// all there is damaged: the install would answer 1638.
#[test]
fn a_current_runtime_with_a_dll_gone_is_damaged() {
    for dlls in [CORE_ONLY, NONE] {
        assert_eq!(
            state_of(dlls, read_key(&key(14, 44))),
            RuntimeState::Damaged
        );
    }
    assert_eq!(
        state_of(ALL, read_key(&key(14, 44))),
        RuntimeState::Installed
    );
}

/// Nothing there, or an older runtime with its files gone: offered the
/// install.
#[test]
fn nothing_there_or_an_older_runtime_is_offered_the_install() {
    assert_eq!(state_of(NONE, Registered::default()), RuntimeState::Missing);
    assert_eq!(
        state_of(NONE, read_key(&key(14, 29))),
        RuntimeState::Missing
    );
    let no_version = Registered {
        installed: true,
        version: None,
    };
    assert_eq!(state_of(NONE, no_version), RuntimeState::Missing);
}

/// The DLLs are read from the folder as files.
#[test]
fn the_dlls_are_read_from_the_folder() {
    let system = tempfile::tempdir().unwrap();
    assert_eq!(dlls_in(system.path()), NONE);
    for dll in ["vcruntime140.dll", "msvcp140.dll"] {
        std::fs::write(system.path().join(dll), "").unwrap();
    }
    assert_eq!(dlls_in(system.path()), CORE_ONLY);
    std::fs::write(system.path().join("vcruntime140_1.dll"), "").unwrap();
    assert_eq!(dlls_in(system.path()), ALL);
}
