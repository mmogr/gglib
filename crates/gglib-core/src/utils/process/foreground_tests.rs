//! What a grant of the foreground reports: never a grant that was not made.
//!
//! What a CI runner can show is limited. Windows grants only when the
//! caller can set the foreground window itself, and whether a test process
//! on a runner can do that depends on the runner's desktop, which no test
//! controls. So these tests hold the answer to the truth either way; they
//! cannot prove that a grant is made, which only the Windows VM's Repair
//! run, with the app's window in front, shows.

use std::io;

use super::{Foreground, allow_foreground};

/// `u32::MAX` is Windows' `ASFW_ANY`, every process: never a process's id,
/// and a grant to every process is never what gglib asks for. It is refused
/// on every OS, before Windows is asked.
#[test]
fn every_process_at_once_is_refused() {
    let refused = allow_foreground(u32::MAX).expect_err("a grant to every process is refused");
    assert_eq!(refused.kind(), io::ErrorKind::InvalidInput, "{refused}");
}

/// `0` is the System Idle Process, never a program gglib started, and
/// Windows does not say what a grant to it does. A lock record that says
/// `0` is refused on every OS, before Windows is asked.
#[test]
fn process_id_zero_is_refused() {
    let refused = allow_foreground(0).expect_err("a grant to process id 0 is refused");
    assert_eq!(refused.kind(), io::ErrorKind::InvalidInput, "{refused}");
}

/// This process's own id. Off Windows there is no such right, and the
/// answer says nothing was granted. On Windows the answer is Windows' own
/// (`host.rs` holds the two side by side): granted, or refused with
/// Windows' reason, never a guess.
#[test]
fn this_process_is_answered_with_what_happened() {
    let answer = allow_foreground(std::process::id());
    if cfg!(windows) {
        match answer {
            Ok(Foreground::Allowed) => {}
            Ok(other) => panic!("Windows answered {other:?}"),
            Err(refused) => assert!(
                refused.raw_os_error().is_some(),
                "a refusal carries Windows' reason: {refused}"
            ),
        }
    } else {
        assert_eq!(answer.expect("nothing to refuse"), Foreground::NotNeeded);
    }
}

/// An id no process has is never reported allowed. Windows documents that
/// the call fails when the caller cannot set the foreground, and names no
/// result for an id of no process. So what this checks depends on the
/// runner:
///
/// - where this process can grant (its own id is granted), the unknown id
///   must be refused: that is the check of the unknown id, which Windows
///   does not document;
/// - where it cannot, the call fails for that reason whatever the id, and
///   this checks only that nothing is reported allowed, and says so.
///
/// No process id comes near this one (Windows' ids are small multiples of
/// four, though no document promises it).
#[test]
fn a_process_that_does_not_exist_is_not_allowed() {
    if !cfg!(windows) {
        assert_eq!(
            allow_foreground(0xFFFF_FFF0).expect("nothing to refuse"),
            Foreground::NotNeeded
        );
        return;
    }
    let can_grant = allow_foreground(std::process::id()).is_ok();
    let answer = allow_foreground(0xFFFF_FFF0);
    assert!(
        answer.is_err(),
        "allowed a process that does not exist: {answer:?}"
    );
    if !can_grant {
        eprintln!(
            "this process cannot set the foreground here, so the unknown id was refused for that \
             reason; the unknown id itself was not checked"
        );
    }
}
