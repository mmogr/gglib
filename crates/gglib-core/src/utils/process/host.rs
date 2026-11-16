//! What a gglib binary does to its own process before anything else.
//!
//! On macOS and Linux nothing: a child gets only the descriptors it is given,
//! because Rust opens every other one close-on-exec, and a library that fails
//! to load is a line on standard error.
//!
//! On Windows `prepare()` does two things, and the third is done on demand:
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
//! - **A job that ends the engines with this process.** [`super::tied`]
//!   puts each child it ties in a kill-on-close job object this process
//!   makes on first use and never closes (#1985).
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

/// Put `child` in this process's kill-on-close job, so Windows ends it when
/// this process ends; see [`super::tied`].
#[cfg(windows)]
pub(super) fn tie_by_job(child: &tokio::process::Child) -> std::io::Result<()> {
    windows::tie_by_job(child)
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "Win32 calls with no safe form in std; ADR 0030 keeps them in this file"
)]
mod windows {
    use std::io;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::ptr;
    use std::sync::OnceLock;

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
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };

    /// The error mode gglib and every process it starts run with: no dialog
    /// for a missing library or file, and none for a crash.
    pub(super) const QUIET: u32 =
        SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX;

    /// This process's job: made on the first [`tie_by_job`], kept until the
    /// process ends, or the OS error that stopped it being made, which every
    /// later call reports again. A static is never dropped, so the handle is
    /// never closed by gglib; Windows closes it when the process ends, and
    /// then ends every process in the job.
    static JOB: OnceLock<Result<OwnedHandle, i32>> = OnceLock::new();

    pub(super) fn tie_by_job(child: &tokio::process::Child) -> io::Result<()> {
        // No handle means the child has already exited and been reaped:
        // there is nothing left to end.
        let Some(process) = child.raw_handle() else {
            return Ok(());
        };
        let job = match JOB.get_or_init(make_job) {
            Ok(job) => job.as_raw_handle(),
            Err(code) => return Err(io::Error::from_raw_os_error(*code)),
        };
        // SAFETY: `job` is open for the life of this process (see `JOB`) and
        // carries JOB_OBJECT_ALL_ACCESS, which includes the
        // JOB_OBJECT_ASSIGN_PROCESS the call needs (CreateJobObjectW).
        // `process` is the child's own handle from CreateProcessW, which has
        // PROCESS_ALL_ACCESS, so PROCESS_SET_QUOTA and PROCESS_TERMINATE
        // (Process Security and Access Rights); it stays open while `child`
        // is borrowed. A child already in a job, as under Windows' SSH
        // server or a terminal that uses one, is nested: since Windows 8 the
        // call needs our job to be "empty or ... in the hierarchy of nested
        // jobs to which the process already belongs"
        // (AssignProcessToJobObject). The first child makes our empty job a
        // child of the jobs it inherited from this process; every later
        // child inherits those same jobs, so our job is in its hierarchy
        // (Nested Jobs). A failure changes nothing about the child.
        if unsafe { AssignProcessToJobObject(job, process) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// A job that ends every process in it when its last handle closes.
    fn make_job() -> Result<OwnedHandle, i32> {
        let last_error = || io::Error::last_os_error().raw_os_error().unwrap_or(0);
        // SAFETY: both arguments may be null. No security attributes gives
        // the default security descriptor and a handle that cannot be
        // inherited (CreateJobObjectW), so no child holds the job open; no
        // name makes a job no other process can open. The result is a new
        // handle, or null on failure.
        let raw = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if raw.is_null() {
            return Err(last_error());
        }
        // SAFETY: `raw` is a new handle that nothing else owns; it is closed
        // only if the job cannot be made kill-on-close, below.
        let job = unsafe { OwnedHandle::from_raw_handle(raw) };

        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        // "Causes all processes associated with the job to terminate when
        // the last handle to the job is closed"; it needs the extended
        // structure (JOBOBJECT_BASIC_LIMIT_INFORMATION).
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let size = u32::try_from(size_of_val(&limits)).expect("the limits fit a u32");
        // SAFETY: `job` is open and has JOB_OBJECT_ALL_ACCESS, which includes
        // JOB_OBJECT_SET_ATTRIBUTES. The pointer and size are those of a live
        // JOBOBJECT_EXTENDED_LIMIT_INFORMATION, the structure the
        // JobObjectExtendedLimitInformation class reads; the call only reads
        // it.
        let set = unsafe {
            SetInformationJobObject(
                job.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size,
            )
        };
        if set == 0 {
            // `job` is dropped and closed: a job that would not end its
            // processes is not kept.
            return Err(last_error());
        }
        Ok(job)
    }

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
