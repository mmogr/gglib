//! Where the Visual C++ runtime stands on this machine: installed, missing
//! (or too old for the engines, which an install puts right), or damaged
//! (which only Windows' own repair puts right). A `#[path]` child of
//! `engine_vc_runtime.rs`.
//!
//! Every Visual C++ runtime from 2015 on writes the same registry key and
//! the same two core DLLs; only those from 2019's 14.2x on add
//! `vcruntime140_1.dll`, which a program built by Visual C++ 2022 for x64
//! (llama.cpp's and stable-diffusion.cpp's builds) imports. So a machine
//! whose runtime is 2015's or 2017's lacks that DLL and is not damaged: it
//! is offered the install, which upgrades it. Damaged is a runtime of 14.30
//! (Visual C++ 2022) or later that Windows lists as installed and whose
//! DLLs are not all there, which the installer would answer with 1638
//! (a newer one is installed) instead of mending. Any older runtime, with
//! or without its files, is mended by the install's upgrade.

use std::path::Path;

use gglib_core::utils::process::bounded::{PROBE_WAIT, run_bounded};
use gglib_core::utils::process::cmd;

/// The DLLs every runtime from 2015 on puts in Windows' system folder.
const CORE_DLLS: [&str; 2] = ["vcruntime140.dll", "msvcp140.dll"];

/// The DLL runtimes from 2019's 14.2x on add, which the engines' x64
/// builds import.
const NEWER_DLL: &str = "vcruntime140_1.dll";

/// Where the runtime records that it is installed, and which version:
/// values `Installed`, `Major`, `Minor`, which Microsoft documents for
/// telling whether it is there. Its installer writes the 32-bit view; both
/// views are read.
const RUNTIME_KEY: &str = r"HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64";

/// The first runtime version (Major 14, Minor 30: Visual C++ 2022) whose
/// missing files the installer will not mend by upgrading.
pub(crate) const CURRENT: (u32, u32) = (14, 30);

/// Where the runtime stands on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeState {
    /// Its DLLs, all three, are in Windows' system folder.
    Installed,
    /// Windows lists a runtime of 14.30 or later as installed, and its DLLs
    /// are not all there.
    Damaged,
    /// Not installed, or one too old to have every DLL; on every other OS,
    /// always.
    Missing,
}

/// What the registry says of the runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Registered {
    /// `Installed` is 1.
    pub installed: bool,
    /// `Major` and `Minor`, when both are there.
    pub version: Option<(u32, u32)>,
}

/// Which of the runtime's DLLs are in the system folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Dlls {
    /// Both core DLLs.
    pub core: bool,
    /// `vcruntime140_1.dll`.
    pub newer: bool,
}

/// Where the runtime stands, read from this machine.
pub(crate) fn runtime_state() -> RuntimeState {
    let Some(root) = std::env::var_os("SystemRoot") else {
        return RuntimeState::Missing;
    };
    let system = Path::new(&root).join("System32");
    let dlls = dlls_in(&system);
    if dlls.core && dlls.newer {
        return RuntimeState::Installed;
    }
    state_of(dlls, registered(&system))
}

/// Where the runtime stands, given its DLLs and what the registry says.
pub(crate) fn state_of(dlls: Dlls, registered: Registered) -> RuntimeState {
    if dlls.core && dlls.newer {
        RuntimeState::Installed
    } else if registered.installed && registered.version.is_some_and(|version| version >= CURRENT) {
        RuntimeState::Damaged
    } else {
        RuntimeState::Missing
    }
}

/// Which of the runtime's DLLs are in `system`.
pub(crate) fn dlls_in(system: &Path) -> Dlls {
    Dlls {
        core: CORE_DLLS.iter().all(|dll| system.join(dll).is_file()),
        newer: system.join(NEWER_DLL).is_file(),
    }
}

/// What the registry says of the runtime, asked of `reg.exe` in `system`
/// in both registry views; the 32-bit view first, which the installer
/// writes.
fn registered(system: &Path) -> Registered {
    ["/reg:32", "/reg:64"]
        .iter()
        .map(|view| {
            run_bounded(
                cmd(system.join("reg.exe")).args(["query", RUNTIME_KEY, view]),
                PROBE_WAIT,
            )
            .ok()
            .filter(|run| run.status.success())
            .map(|run| read_key(&String::from_utf8_lossy(&run.stdout)))
            .unwrap_or_default()
        })
        .find(|registered| registered.installed)
        .unwrap_or_default()
}

/// What `reg query`'s answer for the key says.
pub(crate) fn read_key(printed: &str) -> Registered {
    let mut installed = None;
    let mut major = None;
    let mut minor = None;
    for line in printed.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        let [name, "REG_DWORD", value] = words.as_slice() else {
            continue;
        };
        let Some(number) = value
            .strip_prefix("0x")
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
        else {
            continue;
        };
        match *name {
            "Installed" => installed = Some(number),
            "Major" => major = Some(number),
            "Minor" => minor = Some(number),
            _ => {}
        }
    }
    Registered {
        installed: installed == Some(1),
        version: major.zip(minor),
    }
}

#[cfg(test)]
#[path = "engine_vc_state_tests.rs"]
mod tests;
