//! How a run failed, read from what each OS reports, on every OS.
//!
//! Each sample is what a real run printed or exited with; where it came from
//! is said beside it. The exit status is a value here, so the samples of all
//! three OSes are read on whichever OS runs the tests.

use std::time::Duration;

use super::{super::cmd, Ended, Failure, failure_kind, failure_of, run_bounded};

/// Windows' `STATUS_DLL_NOT_FOUND` as `ExitStatus::code` gives it: the
/// NTSTATUS's bits as an `i32`.
const DLL_NOT_FOUND: i32 = 0xC000_0135_u32.cast_signed();

fn exited(code: Option<i32>, stderr: &str) -> Failure {
    failure_kind(Ended::Exited { code, stderr })
}

fn missing(name: &str) -> Failure {
    Failure::MissingLibrary {
        name: Some(name.to_owned()),
    }
}

/// llama-server on the Windows 11 VM with no Visual C++ runtime, 2026-11-15:
/// the daemon's log read `probe exited with status exit code: 0xc0000135`,
/// and nothing on stderr (gglib switches the loader's dialog off). Windows
/// does not say which DLL.
#[test]
fn windows_dll_not_found_is_a_missing_library() {
    assert_eq!(DLL_NOT_FOUND, -1_073_741_515);
    assert_eq!(
        exited(Some(DLL_NOT_FOUND), ""),
        Failure::MissingLibrary { name: None }
    );
}

/// `STATUS_INVALID_IMAGE_FORMAT` ([MS-ERREF] 2.3.1): a DLL it loads is not
/// for its architecture, such as an x86 DLL found by an x64 program.
#[test]
fn windows_invalid_image_format_is_a_bad_library() {
    assert_eq!(
        exited(Some(0xC000_007B_u32.cast_signed()), ""),
        Failure::BadLibrary { name: None }
    );
}

/// glibc's loader: `fatal_error` (elf/dl-catch.c) prints
/// `<program>: error while loading shared libraries: <name>: <reason>` and
/// `_dl_fatal_printf` (elf/dl-printf.c) exits 127. This line is quoted from
/// ggml-org/llama.cpp#17193.
#[test]
fn the_linux_loader_names_the_library_that_is_missing() {
    assert_eq!(
        exited(
            Some(127),
            "/app/llama-server: error while loading shared libraries: libmtmd.so.0: \
             cannot open shared object file: No such file or directory\n"
        ),
        missing("libmtmd.so.0")
    );
}

/// glibc's `wrong ELF class` (elf/dl-load.c): the library is there, built
/// for another word size.
#[test]
fn the_linux_loader_names_a_library_of_another_architecture() {
    assert_eq!(
        exited(
            Some(127),
            "./llama-server: error while loading shared libraries: libggml.so: \
             wrong ELF class: ELFCLASS32\n"
        ),
        Failure::BadLibrary {
            name: Some("libggml.so".to_owned())
        }
    );
}

/// dyld on macOS 27.2, from a program linked to `@rpath/libfoo.dylib` run
/// with the library removed. dyld aborts it, so there is no exit code.
#[test]
fn macos_dyld_names_the_library_that_is_missing() {
    let stderr = "dyld[97301]: Library not loaded: @rpath/libfoo.dylib\n  \
        Referenced from: <86584836-5CD4-3B7D-90BF-695EF0E02F32> /tmp/dyld/prog\n  \
        Reason: tried: '/tmp/dyld/libfoo.dylib' (no such file), \
        '/tmp/dyld/libfoo.dylib' (no such file)\n";
    assert_eq!(exited(None, stderr), missing("libfoo.dylib"));
}

/// The same program on macOS 27.2 with an x86-64 build of the library put in
/// its place.
#[test]
fn macos_dyld_names_a_library_of_another_architecture() {
    let stderr = "dyld[97306]: Library not loaded: @rpath/libfoo.dylib\n  \
        Referenced from: <86584836-5CD4-3B7D-90BF-695EF0E02F32> /tmp/dyld/prog\n  \
        Reason: tried: '/tmp/dyld/libfoo.dylib' (mach-o file, but is an incompatible \
        architecture (have 'x86_64', need 'arm64e, arm64e.v1, arm64, arm64'))\n";
    assert_eq!(
        exited(None, stderr),
        Failure::BadLibrary {
            name: Some("libfoo.dylib".to_owned())
        }
    );
}

/// dyld before macOS 12 wrote `dyld: Library not loaded: <path>`, with no
/// pid.
#[test]
fn macos_dyld_before_12_names_the_library_too() {
    let stderr = "dyld: Library not loaded: @rpath/libggml.dylib\n  \
        Referenced from: /usr/local/bin/llama-server\n  Reason: image not found\n";
    assert_eq!(exited(None, stderr), missing("libggml.dylib"));
}

/// 127 alone is not a missing library: `sh` exits 127 for a command it
/// cannot find, and a program may exit 127 itself.
#[test]
fn an_exit_of_127_without_the_loaders_words_is_a_failure() {
    assert_eq!(
        exited(Some(127), "sh: 1: llama-server: not found\n"),
        Failure::Failed { code: 127 }
    );
}

#[test]
fn a_program_that_exits_non_zero_failed() {
    assert_eq!(
        exited(Some(1), "error: unknown argument: --version\n"),
        Failure::Failed { code: 1 }
    );
}

/// A Windows crash is an NTSTATUS such as `STATUS_ACCESS_VIOLATION`; a Unix
/// one is a signal, with no code.
#[test]
fn a_crash_is_a_crash_on_either_os() {
    let access_violation = 0xC000_0005_u32.cast_signed();
    assert_eq!(
        exited(Some(access_violation), ""),
        Failure::Crashed {
            code: Some(access_violation)
        }
    );
    assert_eq!(exited(None, ""), Failure::Crashed { code: None });
}

#[test]
fn a_program_that_could_not_start_says_why() {
    let gone = std::io::Error::from(std::io::ErrorKind::NotFound);
    assert_eq!(failure_kind(Ended::NotStarted(&gone)), Failure::NotFound);

    let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
    assert_eq!(
        failure_kind(Ended::NotStarted(&denied)),
        Failure::NotExecutable
    );

    let other = std::io::Error::other("Exec format error");
    assert_eq!(
        failure_kind(Ended::NotStarted(&other)),
        Failure::CouldNotStart {
            reason: "Exec format error".to_owned()
        }
    );
    assert_eq!(failure_kind(Ended::TimedOut), Failure::TimedOut);
}

/// A run read through [`failure_of`]: a program that answers has no failure,
/// even with its banner on stderr as llama-server prints it; one that prints
/// the loader's words and exits 127 is missing a library; one that is not
/// there is not found.
#[test]
fn a_bounded_run_is_read_the_same_way() {
    let dir = tempfile::tempdir().unwrap();
    let answers = gglib_test_proc::install(
        dir.path(),
        "answers",
        "eprint version: 7563 (b2a0c0f)\nexit 0",
    );
    let run = run_bounded(cmd(&answers).arg("--version"), Duration::from_secs(10));
    assert_eq!(failure_of(&run), None);

    let missing_one = gglib_test_proc::install(
        dir.path(),
        "missing-one",
        "eprint ./missing-one: error while loading shared libraries: libvulkan.so.1: \
         cannot open shared object file: No such file or directory\nexit 127",
    );
    let run = run_bounded(cmd(&missing_one).arg("--version"), Duration::from_secs(10));
    assert_eq!(failure_of(&run), Some(missing("libvulkan.so.1")));

    let run = run_bounded(
        &mut cmd(dir.path().join("not-there")),
        Duration::from_secs(1),
    );
    assert_eq!(failure_of(&run), Some(Failure::NotFound));
}

/// What a person reads for Windows' missing DLL says which runtime it most
/// likely is, and where it comes from.
#[test]
fn an_unnamed_missing_library_is_worded_as_the_visual_cpp_runtime() {
    let words = Failure::MissingLibrary { name: None }.to_string();
    assert!(words.contains("Visual C++"), "{words}");
    assert!(words.contains("x64"), "{words}");
    assert!(
        words.contains("https://aka.ms/vc14/vc_redist.x64.exe"),
        "{words}"
    );
    assert_eq!(
        missing("libvulkan.so.1").to_string(),
        "the library libvulkan.so.1 it needs is missing"
    );
}
