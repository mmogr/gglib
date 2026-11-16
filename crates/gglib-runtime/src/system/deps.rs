//! How a row of the dependency table is looked for on this machine.
//!
//! The table (`gglib_core::utils::system::NEEDS`) says what each row is
//! called on each OS and how its version is read; this runs that, through the
//! time-limited helpers in `llama::detect::tools` and `pkg-config`.

use gglib_core::utils::process::bounded::{PROBE_WAIT, run_bounded};
use gglib_core::utils::process::cmd;
use gglib_core::utils::system::{HostOs, Need, Probe};

use crate::llama::detect::tools::{compiler_version, version_line, version_word};

/// The version `need` reports on `os`, from the first of its names that
/// answers, or `None` when none does.
pub(super) fn version_of(need: &Need, os: HostOs) -> Option<String> {
    need.names(os).iter().find_map(|name| match need.probe {
        Probe::Word(index) => version_word(name, index),
        Probe::Line => version_line(name),
        Probe::LineLessV => version_line(name).map(|line| line.trim_start_matches('v').to_owned()),
        Probe::Compiler => compiler_version(name),
        // `python` is Python 3 on some systems and Python 2 on others; on
        // Windows either name may be the Store's stand-in, which fails.
        Probe::Python3 => version_word(name, 1).filter(|version| version.starts_with('3')),
        Probe::PkgConfig => check_pkg_config_lib(name),
        Probe::LibClang => check_libclang(),
    })
}

/// Check if libclang-dev is installed (needed by bindgen for FFI bindings).
///
/// libclang doesn't have a pkg-config file, so we check for the shared library
/// directly: where llvm-config says it is, then the standard library paths,
/// then the per-version LLVM directories. Asked on Linux alone, as the table
/// says.
fn check_libclang() -> Option<String> {
    use std::path::Path;

    if let Ok(output) = run_bounded(cmd("llvm-config").arg("--libdir"), PROBE_WAIT)
        && output.status.success()
        && dir_has_libclang(Path::new(String::from_utf8_lossy(&output.stdout).trim()))
    {
        // The LLVM version, for display.
        if let Ok(ver_output) = run_bounded(cmd("llvm-config").arg("--version"), PROBE_WAIT)
            && ver_output.status.success()
        {
            return Some(
                String::from_utf8_lossy(&ver_output.stdout)
                    .trim()
                    .to_string(),
            );
        }
        return Some("installed".to_string());
    }

    if ["/usr/lib/x86_64-linux-gnu", "/usr/lib/aarch64-linux-gnu"]
        .iter()
        .any(|dir| dir_has_libclang(Path::new(dir)))
    {
        return Some("installed".to_string());
    }

    (11..=20)
        .rev()
        .find(|major| dir_has_libclang(Path::new(&format!("/usr/lib/llvm-{major}/lib"))))
        .map(|major| major.to_string())
}

/// Whether `dir` holds a libclang shared library.
fn dir_has_libclang(dir: &std::path::Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("libclang") && name.contains(".so")
        })
    })
}

/// Check for a library using pkg-config.
pub(super) fn check_pkg_config_lib(lib_name: &str) -> Option<String> {
    let output = run_bounded(
        cmd("pkg-config").args(["--modversion", lib_name]),
        PROBE_WAIT,
    )
    .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_string()
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory that is not there holds none, and neither does one whose
    /// only matches are libclang's static archive or another library's `.so`.
    #[test]
    fn a_directory_holds_libclang_when_a_shared_library_of_that_name_is_in_it() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!dir_has_libclang(&dir.path().join("absent")));
        assert!(!dir_has_libclang(dir.path()));

        std::fs::write(dir.path().join("libclang.a"), "").unwrap();
        std::fs::write(dir.path().join("libLLVM-18.so"), "").unwrap();
        assert!(!dir_has_libclang(dir.path()));

        std::fs::write(dir.path().join("libclang-18.so.1"), "").unwrap();
        assert!(dir_has_libclang(dir.path()));
    }

    #[test]
    fn test_check_pkg_config_lib_nonexistent() {
        // A library that definitely doesn't exist
        assert!(check_pkg_config_lib("nonexistent-library-12345").is_none());
    }

    /// A row with no name on an OS is not looked for there at all.
    #[test]
    fn a_row_with_no_name_on_an_os_is_not_asked() {
        let patchelf = gglib_core::utils::system::need("patchelf").unwrap();
        assert_eq!(version_of(patchelf, HostOs::Windows), None);
    }
}
