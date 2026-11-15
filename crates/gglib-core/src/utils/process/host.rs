//! What a gglib binary does to its own process before anything else.
//!
//! On macOS and Linux nothing: a child gets only the descriptors it is given,
//! because Rust opens every other one close-on-exec, and a library that fails
//! to load is a line on standard error.
//!
//! On Windows two things:
//!
//! - **The standard handles stay private.** Windows hands a new process every
//!   inheritable handle of its parent, not only the three it is given. The
//!   handles gglib was started with are inheritable, so a service started by
//!   `gglib up -y` held the pipe of a caller reading that command's output,
//!   and the caller waited until the service and its model process exited
//!   (#1982). A child that is meant to share a handle still gets it:
//!   `Stdio::inherit()` passes it on as a copy made for that child. This
//!   covers the three standard handles only. Another handle a caller let
//!   gglib inherit still reaches gglib's children: Rust's `Command` always
//!   passes every inheritable handle, and a handle list is not stable.
//! - **No error dialogs.** An engine missing a DLL put a modal dialog on the
//!   screen at every probe, and a crash can wait on the "stopped working" box.
//!   With the error mode set, each is an exit code gglib reads instead. Every
//!   process gglib starts inherits the error mode.
//!
//! These calls have no safe form in the standard library, so the Windows
//! half of this file allows `unsafe`, and Windows is called directly from
//! here only (ADR 0030).

/// Prepare this process. Each `main` calls it first, before any child
/// process is started.
pub fn prepare() {
    #[cfg(windows)]
    windows::prepare();
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "Win32 calls with no safe form in std; ADR 0030 keeps them in this file"
)]
mod windows {
    use windows_sys::Win32::Foundation::{
        HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation,
    };
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };
    use windows_sys::Win32::System::Diagnostics::Debug::{
        GetErrorMode, SEM_FAILCRITICALERRORS, SEM_NOGPFAULTERRORBOX, SEM_NOOPENFILEERRORBOX,
        SetErrorMode,
    };

    /// The error mode gglib and every process it starts run with: no dialog
    /// for a missing library or file, and none for a crash.
    pub(super) const QUIET: u32 =
        SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX;

    pub(super) fn prepare() {
        for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            // SAFETY: GetStdHandle takes one of the three constants, reads
            // this process's own table and returns a handle, null when there
            // is none, or INVALID_HANDLE_VALUE. No pointer is passed.
            let handle = unsafe { GetStdHandle(which) };
            if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                continue;
            }
            // SAFETY: `handle` is this process's own standard handle, open
            // for as long as the process runs. Clearing its inherit flag
            // changes only what a child is handed; the handle itself, and
            // every read and write on it, are as before. A failure leaves the
            // flag as it was, which is today's behaviour, so it is ignored.
            unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) };
        }
        // SAFETY: both take and return plain flags. The mode only adds to
        // what was set, so a mode the process started with is kept.
        unsafe { SetErrorMode(GetErrorMode() | QUIET) };
    }

    #[cfg(test)]
    mod tests {
        use super::{GetErrorMode, QUIET, prepare};

        #[test]
        fn prepare_switches_off_the_error_dialogs() {
            prepare();
            // SAFETY: GetErrorMode takes nothing and returns flags.
            let mode = unsafe { GetErrorMode() };
            assert_eq!(mode & QUIET, QUIET, "error mode {mode:#x}");
        }
    }
}
