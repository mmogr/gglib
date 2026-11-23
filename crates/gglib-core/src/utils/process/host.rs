//! What a gglib binary does to its own process before anything else.
//!
//! On macOS and Linux nothing: a child gets only the descriptors it is given,
//! because Rust opens every other one close-on-exec, and a library that fails
//! to load is a line on standard error.
//!
//! On Windows `prepare()` does two things, and the last two are done on
//! demand:
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
//! - **A job for each child started as a tree.** [`super::tree`] puts such
//!   a child in a kill-on-close job of its own, so stopping it ends what it
//!   started too, as `cmd.exe` running the `node.exe` of `npx.cmd` (#2012).
//!
//! It also reads and writes the user's own `Path` variable, and gglib's
//! record of the entries it added there, for [`crate::paths::user_path`]:
//! the desktop app's Windows installer runs gglib to put its folder there
//! and take it off (#1959). Off Windows there is no such variable, and
//! `Registry` says so. No other crate may call it.
//!
//! And it reads one value for the desktop app's tray: whether the taskbar
//! is light ([`taskbar_is_light`]), which is the system's mode and not the
//! app's, so the tray icon can be drawn dark on a light taskbar and light on
//! a dark one (#2050). Off Windows there is no such taskbar, and the
//! answer is `None`.
//!
//! In this crate's tests and with its `test-utils` feature it also sends
//! Ctrl-Break to a child started on this process's
//! console in a process group of its own, the orderly stop a test asks of a
//! child on Windows (`test_stop`, #2189).
//!
//! These calls have no safe form in the standard library, so the Windows
//! half of this file allows `unsafe`, and Windows is called directly from
//! here only (ADR 0030).

use crate::paths::user_path::{Store, UserPath};

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

/// The registry, where Windows keeps the user's own `Path` and gglib its
/// record of the folders it added there: the one [`Store`] outside tests.
/// Visible to gglib-core alone, so no other crate can rewrite a user's
/// `Path`.
///
/// - The `Path` is the value `Path` of `HKEY_CURRENT_USER\Environment`,
///   which Windows joins to the machine's `Path` when it builds a new
///   program's environment. It is read whole however long it is, with every
///   `%VAR%` unexpanded, and written with the type it had: `REG_EXPAND_SZ`
///   or `REG_SZ`. An empty one is written empty, never removed. After each
///   write every top-level window is told that the environment changed, so
///   a terminal started from Explorer or the Start menu from now on reads
///   it.
/// - The record is the value `PathEntriesAdded` of
///   `HKEY_CURRENT_USER\Software\gglib`, a `REG_SZ` list separated by `;`,
///   removed when it is empty.
///
/// A read fails with the error Windows gave, a value of a type other than
/// `REG_SZ` or `REG_EXPAND_SZ` among them, or on a value that is not valid
/// UTF-16, which gglib leaves alone rather than write back changed. Off
/// Windows the `Path` is [`UserPath::NotKept`], the record is empty, and a
/// write is an [`std::io::ErrorKind::Unsupported`] error that changes
/// nothing.
pub(crate) struct Registry;

impl Store for Registry {
    fn path(&mut self) -> std::io::Result<UserPath> {
        #[cfg(windows)]
        return windows::user_path();
        #[cfg(not(windows))]
        Ok(UserPath::NotKept)
    }

    fn set_path(&mut self, text: &str, expandable: bool) -> std::io::Result<()> {
        #[cfg(windows)]
        return windows::set_user_path(text, expandable);
        #[cfg(not(windows))]
        {
            let _ = (text, expandable);
            Err(not_kept())
        }
    }

    fn record(&mut self) -> std::io::Result<String> {
        #[cfg(windows)]
        return windows::path_record();
        #[cfg(not(windows))]
        Ok(String::new())
    }

    fn set_record(&mut self, text: &str) -> std::io::Result<()> {
        #[cfg(windows)]
        return windows::set_path_record(text);
        #[cfg(not(windows))]
        {
            let _ = text;
            Err(not_kept())
        }
    }
}

// --- The taskbar's colour (#2050) ---

/// Whether Windows draws its taskbar light, and with it the notification
/// area and its overflow flyout, where the tray icon is shown.
///
/// That is the system's mode ("Choose your default Windows mode"), the
/// value `SystemUsesLightTheme` of
/// `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize`,
/// and not the app mode beside it, `AppsUseLightTheme`, which is what a
/// window's theme follows: the two can differ. Read afresh at each call.
///
/// `None` off Windows, where no taskbar of this kind exists, and on a
/// Windows that keeps no such value (before Windows 10 1903) or will not
/// give it.
#[must_use]
pub fn taskbar_is_light() -> Option<bool> {
    #[cfg(windows)]
    return windows::taskbar_is_light();
    #[cfg(not(windows))]
    None
}

#[cfg(not(windows))]
fn not_kept() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "only Windows keeps a user's own Path",
    )
}

// --- A tree: a child started in a job of its own (#2012) ---

/// A kill-on-close job holding one child and every process it starts, and
/// the suspended start that puts the child in it before it runs; see
/// [`super::tree`].
#[cfg(windows)]
pub(super) use windows::{TreeJob, resume, start_suspended};

// --- An orderly stop a test asks of a child (#2189) ---

/// Ctrl-Break, and the start on this process's console in a process group
/// of its own that lets it reach a child; see [`super::test_stop`].
#[cfg(all(windows, any(test, feature = "test-utils")))]
pub(super) use windows::{ctrl_break, own_group_on_this_console};

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
        ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_SUCCESS, HANDLE_FLAG_INHERIT,
        INVALID_HANDLE_VALUE, SetHandleInformation,
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
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_EXPAND_SZ, REG_SZ, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ,
        RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
    };

    use super::UserPath;

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

    /// The key under `HKEY_CURRENT_USER` that holds the user's own
    /// variables, and the variable's name. Registry names are not case
    /// sensitive, so `Path` also finds a value written `PATH`.
    const ENVIRONMENT: &str = "Environment";
    const PATH: &str = "Path";

    /// `text` as UTF-16 with the terminating NUL every call below reads to.
    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// A registry status as an [`io::Error`].
    fn status_error(status: u32) -> io::Error {
        io::Error::from_raw_os_error(i32::try_from(status).unwrap_or(i32::MAX))
    }

    pub(super) fn user_path() -> io::Result<UserPath> {
        Ok(match read_string(ENVIRONMENT, PATH)? {
            None => UserPath::Absent,
            Some((text, expandable)) => UserPath::Text { text, expandable },
        })
    }

    pub(super) fn set_user_path(text: &str, expandable: bool) -> io::Result<()> {
        write_string(ENVIRONMENT, PATH, text, expandable)?;
        announce_environment_change();
        Ok(())
    }

    /// gglib's own key under `HKEY_CURRENT_USER`, and the value that
    /// records the folders gglib added to the user's `Path`.
    const GGLIB: &str = r"Software\gglib";
    const PATH_RECORD: &str = "PathEntriesAdded";

    pub(super) fn path_record() -> io::Result<String> {
        Ok(read_string(GGLIB, PATH_RECORD)?
            .map(|(text, _)| text)
            .unwrap_or_default())
    }

    pub(super) fn set_path_record(text: &str) -> io::Result<()> {
        if text.is_empty() {
            delete_value(GGLIB, PATH_RECORD)
        } else {
            write_string(GGLIB, PATH_RECORD, text, false)
        }
    }

    /// Remove the value `value` of the key `subkey` under
    /// `HKEY_CURRENT_USER`; one that is not there is already removed.
    fn delete_value(subkey: &str, value: &str) -> io::Result<()> {
        let key = wide(subkey);
        let name = wide(value);
        // SAFETY: the two names are NUL-terminated UTF-16 that outlive the
        // call, which only reads them.
        match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) } {
            ERROR_SUCCESS | ERROR_FILE_NOT_FOUND => Ok(()),
            other => Err(status_error(other)),
        }
    }

    /// The string value `value` of the key `subkey` under
    /// `HKEY_CURRENT_USER`, whole, with whether it is `REG_EXPAND_SZ`;
    /// `None` when there is no such value or key.
    fn read_string(subkey: &str, value: &str) -> io::Result<Option<(String, bool)>> {
        let key = wide(subkey);
        let name = wide(value);
        // Either string type, and a REG_EXPAND_SZ value as it is stored:
        // RRF_NOEXPAND is "do not automatically expand environment strings
        // if the value is of type REG_EXPAND_SZ" (RegGetValueW).
        let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
        loop {
            let mut size: u32 = 0;
            // SAFETY: the two names are NUL-terminated UTF-16 that outlive
            // the call. "If pvData is NULL, and pcbData is non-NULL, the
            // function returns ERROR_SUCCESS and stores the size of the
            // data, in bytes" (RegGetValueW); it writes only `size`.
            let status = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    name.as_ptr(),
                    flags,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    &raw mut size,
                )
            };
            match status {
                ERROR_SUCCESS => {}
                // No such value, or no such key at all.
                ERROR_FILE_NOT_FOUND => return Ok(None),
                other => return Err(status_error(other)),
            }
            // Room for the value and one more NUL: the call adds one to a
            // string stored without it "if the buffer is large enough",
            // and fails otherwise.
            let units = usize::try_from(size).unwrap_or(usize::MAX).div_ceil(2) + 1;
            let mut buffer = vec![0u16; units];
            let mut written = u32::try_from(units * 2)
                .map_err(|_| io::Error::other("the value is too long to read"))?;
            let mut kind: u32 = 0;
            // SAFETY: the names are as above, and `buffer` is `written`
            // bytes of writable memory that outlives the call. The call
            // writes at most `written` bytes there, the value's type to
            // `kind` and the number of bytes it wrote to `written`.
            let status = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    name.as_ptr(),
                    flags,
                    &raw mut kind,
                    buffer.as_mut_ptr().cast(),
                    &raw mut written,
                )
            };
            match status {
                ERROR_SUCCESS => {}
                // The value grew between the two calls: read it again.
                ERROR_MORE_DATA => continue,
                ERROR_FILE_NOT_FOUND => return Ok(None),
                other => return Err(status_error(other)),
            }
            buffer.truncate(usize::try_from(written).unwrap_or(usize::MAX) / 2);
            while buffer.last() == Some(&0) {
                buffer.pop();
            }
            let text = String::from_utf16(&buffer).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{subkey}\\{value} is not valid UTF-16, so gglib leaves it as it is"),
                )
            })?;
            return Ok(Some((text, kind == REG_EXPAND_SZ)));
        }
    }

    /// Store `text` as the string value `value` of the key `subkey` under
    /// `HKEY_CURRENT_USER`, `REG_EXPAND_SZ` when `expandable` and `REG_SZ`
    /// otherwise. An empty `text` is stored empty.
    fn write_string(subkey: &str, value: &str, text: &str, expandable: bool) -> io::Result<()> {
        let key = wide(subkey);
        let name = wide(value);
        let data = wide(text);
        let bytes = u32::try_from(data.len() * 2)
            .map_err(|_| io::Error::other("the value is too long to store"))?;
        let kind = if expandable { REG_EXPAND_SZ } else { REG_SZ };
        // SAFETY: the names are NUL-terminated UTF-16 that outlive the
        // call. `data` is NUL-terminated UTF-16 of `bytes` bytes with the
        // NUL counted, as "cbData must include the size of the terminating
        // null character" for a string type (RegSetKeyValueW); the call only
        // reads it. A missing key "is created".
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                kind,
                data.as_ptr().cast(),
                bytes,
            )
        };
        match status {
            ERROR_SUCCESS => Ok(()),
            other => Err(status_error(other)),
        }
    }

    /// Tell every top-level window that the user's variables changed.
    /// Explorer reads them again, so a terminal it or the Start menu starts
    /// from now on has the new `Path`. A program already running keeps the
    /// environment it started with.
    fn announce_environment_change() {
        let area = wide(ENVIRONMENT);
        let mut answer: usize = 0;
        // SAFETY: "to effect a change in the environment variables for the
        // system or the user, broadcast this message with lParam set to the
        // string 'Environment'" (WM_SETTINGCHANGE). `area` is that string,
        // NUL-terminated, and outlives the call, which returns once every
        // window has answered or run out of time; it writes only `answer`.
        // SMTO_ABORTIFHUNG skips a window that is not responding, and each
        // other window has two seconds (SendMessageTimeoutW). A failure
        // means only that some program reads the change later, so the
        // result is not read.
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                area.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                2_000,
                &raw mut answer,
            )
        };
    }

    // --- A tree: a child started in a job of its own (#2012) ---

    pub(in crate::utils::process) use tree::{TreeJob, resume, start_suspended};

    mod tree {
        use std::io;
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
        };
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, TerminateJobObject,
        };
        use windows_sys::Win32::System::Threading::{
            CREATE_SUSPENDED, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
        };

        use super::make_job;

        /// Have `command` start its process suspended, with no console
        /// window: "The primary thread of the new process is created in a
        /// suspended state, and does not run until the `ResumeThread` function
        /// is called" (Process Creation Flags). Nothing it would start can
        /// start before [`resume`].
        pub(in crate::utils::process) fn start_suspended(command: &mut tokio::process::Command) {
            #[expect(
                clippy::disallowed_methods,
                reason = "a tree's creation flags are set here, in the process module"
            )]
            command.creation_flags(crate::utils::process::CREATE_NO_WINDOW | CREATE_SUSPENDED);
        }

        /// Resume the process `pid`, started by [`start_suspended`]: every
        /// thread of it the system lists. A thread that was not suspended
        /// is left as it was, so only the suspended first thread starts
        /// running.
        ///
        /// # Errors
        ///
        /// When the threads cannot be listed, a thread of it cannot be
        /// opened or resumed, or none is found. The process may then never
        /// run, and the caller ends it.
        pub(in crate::utils::process) fn resume(pid: u32) -> io::Result<()> {
            // SAFETY: two plain values. TH32CS_SNAPTHREAD "Includes all
            // threads in the system in the snapshot", and the process id
            // "is ignored" for it (CreateToolhelp32Snapshot). The result is
            // a new handle, or INVALID_HANDLE_VALUE on failure.
            let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
            if raw == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: `raw` is a new handle nothing else owns; it is closed
            // when `snapshot` is dropped ("To destroy the snapshot, use the
            // CloseHandle function").
            let snapshot = unsafe { OwnedHandle::from_raw_handle(raw) };
            let owner_is_read =
                std::mem::offset_of!(THREADENTRY32, th32OwnerProcessID) + size_of::<u32>();
            let full = u32::try_from(size_of::<THREADENTRY32>()).expect("the entry fits a u32");
            let mut entry = THREADENTRY32 {
                dwSize: full,
                ..THREADENTRY32::default()
            };
            let mut resumed = 0;
            // SAFETY: `snapshot` is open; `entry` is a live THREADENTRY32
            // whose dwSize the caller "must set ... to the size, in bytes, of
            // the structure" (Thread32First), as it is before each call. The
            // calls write at most that many bytes into it.
            let mut listed = unsafe { Thread32First(snapshot.as_raw_handle(), &raw mut entry) };
            while listed != 0 {
                // "do not rely on the values of any members whose offsets
                // are greater than" the dwSize the call wrote (Thread32First).
                let read = usize::try_from(entry.dwSize).unwrap_or(0) >= owner_is_read;
                if read && entry.th32OwnerProcessID == pid {
                    resume_thread(entry.th32ThreadID)?;
                    resumed += 1;
                }
                entry.dwSize = full;
                // SAFETY: as for Thread32First above.
                listed = unsafe { Thread32Next(snapshot.as_raw_handle(), &raw mut entry) };
            }
            if resumed == 0 {
                return Err(io::Error::other("no thread of it was found to resume"));
            }
            Ok(())
        }

        /// Resume the thread `id`.
        fn resume_thread(id: u32) -> io::Result<()> {
            // SAFETY: plain values. With bInheritHandle false "the
            // processes do not inherit this handle"; the result is "an open
            // handle to the specified thread", granted the access asked
            // for, or NULL on failure (OpenThread).
            let raw = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, id) };
            if raw.is_null() {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: `raw` is a new handle nothing else owns.
            let thread = unsafe { OwnedHandle::from_raw_handle(raw) };
            // SAFETY: `thread` is open and has THREAD_SUSPEND_RESUME, the
            // right ResumeThread needs. It "Decrements a thread's suspend
            // count", and returns "(DWORD) -1" on failure (ResumeThread); a
            // thread that was not suspended is left as it was.
            if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        /// A job holding one child and every process it starts, made by
        /// [`make_job`]: kill-on-close, and its one handle is this value's.
        /// [`TreeJob::terminate`] ends them all; so does dropping it, which
        /// closes that handle ("closing the last job object handle
        /// terminates all associated processes", Job Objects), and so does
        /// this process ending, however it ends, since Windows then closes
        /// every handle the process had.
        #[derive(Debug)]
        pub(in crate::utils::process) struct TreeJob(OwnedHandle);

        impl TreeJob {
            /// Put `child`, just started and still suspended, in a new job
            /// of its own.
            ///
            /// A process the child starts joins the job: "After a process
            /// is associated with a job, by default any child processes it
            /// creates using `CreateProcess` are also associated with the
            /// job" (Job Objects), and this job sets neither breakaway
            /// limit, so none leaves it.
            pub(in crate::utils::process) fn of(child: &tokio::process::Child) -> io::Result<Self> {
                let Some(process) = child.raw_handle() else {
                    return Err(io::Error::other(
                        "it exited before it could be put in a job",
                    ));
                };
                let job = make_job().map_err(io::Error::from_raw_os_error)?;
                // SAFETY: `job` is the new, empty job made above, open while
                // this runs, with JOB_OBJECT_ALL_ACCESS and so the
                // JOB_OBJECT_ASSIGN_PROCESS the call needs
                // (CreateJobObjectW). `process` is the child's own handle
                // from CreateProcessW, with PROCESS_ALL_ACCESS and so
                // PROCESS_SET_QUOTA and PROCESS_TERMINATE; it stays open
                // while `child` is borrowed. A child already in a job (one
                // it inherited from this process, as under Windows' SSH
                // server) is nested, since Windows 8: the job "must be empty
                // or it must be in the hierarchy of nested jobs to which the
                // process already belongs" (AssignProcessToJobObject), and
                // this one is empty. On a failure `job` is dropped and
                // closed; it held nothing.
                if unsafe { AssignProcessToJobObject(job.as_raw_handle(), process) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(Self(job))
            }

            /// End every process in the job now, without waiting for them.
            pub(in crate::utils::process) fn terminate(&self) -> io::Result<()> {
                // SAFETY: the handle is open while `self` lives and has
                // JOB_OBJECT_ALL_ACCESS, which includes the
                // JOB_OBJECT_TERMINATE the call needs. It "Terminates all
                // processes currently associated with the job", and for a
                // nested job those associated with "all of its child jobs in
                // the hierarchy" (TerminateJobObject); each exits with code
                // 1. The handle stays open.
                if unsafe { TerminateJobObject(self.0.as_raw_handle(), 1) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            }
        }
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

    // --- The taskbar's colour (#2050) ---

    pub(super) use taskbar::taskbar_is_light;

    mod taskbar {
        use std::io;
        use std::ptr;

        use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
        use windows_sys::Win32::System::Registry::{
            HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW,
        };

        use super::{status_error, wide};

        /// The key that holds the user's light and dark modes, and the
        /// value for the system's (the taskbar's) mode: 1 for light.
        pub(super) const PERSONALIZE: &str =
            r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
        pub(super) const SYSTEM_USES_LIGHT_THEME: &str = "SystemUsesLightTheme";

        pub(in crate::utils::process) fn taskbar_is_light() -> Option<bool> {
            read_dword(PERSONALIZE, SYSTEM_USES_LIGHT_THEME)
                .ok()
                .flatten()
                .map(|value| value != 0)
        }

        /// The `REG_DWORD` value `value` of the key `subkey` under
        /// `HKEY_CURRENT_USER`; `None` when there is no such value or key.
        /// A value of another type is an error.
        pub(super) fn read_dword(subkey: &str, value: &str) -> io::Result<Option<u32>> {
            let key = wide(subkey);
            let name = wide(value);
            let mut data: u32 = 0;
            let mut size = u32::try_from(size_of::<u32>()).expect("four bytes fit a u32");
            // SAFETY: the two names are NUL-terminated UTF-16 that outlive
            // the call, which only reads them. The call writes at most
            // `size` bytes, four, to `data`, a live u32, and the count it
            // wrote to `size`; RRF_RT_REG_DWORD accepts only a 32-bit
            // value, and a value of another type fails with
            // ERROR_UNSUPPORTED_TYPE (RegGetValueW). No type is asked for.
            let status = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    name.as_ptr(),
                    RRF_RT_REG_DWORD,
                    ptr::null_mut(),
                    (&raw mut data).cast(),
                    &raw mut size,
                )
            };
            match status {
                ERROR_SUCCESS => Ok(Some(data)),
                ERROR_FILE_NOT_FOUND => Ok(None),
                other => Err(status_error(other)),
            }
        }
    }

    // --- An orderly stop a test asks of a child (#2189) ---

    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::utils::process) use console_stop::{ctrl_break, own_group_on_this_console};

    #[cfg(any(test, feature = "test-utils"))]
    mod console_stop {
        use std::io;

        use windows_sys::Win32::System::Console::{
            AttachConsole, CTRL_BREAK_EVENT, FreeConsole, GenerateConsoleCtrlEvent,
        };
        use windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP;

        /// Have `command` start its process on this process's console, in a
        /// process group of its own whose id is the process's id. The flags
        /// are replaced, not added to: the `CREATE_NO_WINDOW` that
        /// [`crate::utils::process::cmd`] sets gives the child a console of
        /// its own, which a Ctrl-Break sent from here does not reach. A new
        /// group ignores Ctrl-C ("CTRL+C signals will be disabled for all
        /// processes within the new process group", Process Creation Flags)
        /// and still hears Ctrl-Break.
        pub(in crate::utils::process) fn own_group_on_this_console(
            command: &mut std::process::Command,
        ) {
            use std::os::windows::process::CommandExt as _;
            #[expect(
                clippy::disallowed_methods,
                reason = "a test child's creation flags are set here, in the process module"
            )]
            command.creation_flags(CREATE_NEW_PROCESS_GROUP);
        }

        /// Send Ctrl-Break to the process group `group`, started by
        /// [`own_group_on_this_console`]. When this process has no console,
        /// that child was given one of its own, and this process attaches
        /// to it for the send and leaves it after.
        pub(in crate::utils::process) fn ctrl_break(group: u32) -> io::Result<()> {
            let Err(unsent) = send(group) else {
                return Ok(());
            };
            // SAFETY: a plain value. It fails and changes nothing when this
            // process already has a console ("If the calling process is
            // already attached to a console, the error code returned is
            // ERROR_ACCESS_DENIED", AttachConsole); the first error is
            // then the one reported.
            if unsafe { AttachConsole(group) } == 0 {
                return Err(unsent);
            }
            let sent = send(group);
            // SAFETY: no arguments. It detaches this process from the
            // console it attached to above; the child keeps it.
            unsafe { FreeConsole() };
            sent
        }

        /// `GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, group)`.
        fn send(group: u32) -> io::Result<()> {
            // SAFETY: plain values. The event goes to the processes of the
            // group `group` "that share the console associated with the
            // calling process" (GenerateConsoleCtrlEvent); this process is
            // in another group, so it is not sent it.
            if unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, group) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use std::ptr;

        use windows_sys::Win32::System::Registry::{
            HKEY, KEY_QUERY_VALUE, RegCloseKey, RegDeleteKeyW, RegOpenKeyExW, RegQueryValueExW,
        };

        use super::{
            ERROR_SUCCESS, GetErrorMode, HKEY_CURRENT_USER, QUIET, delete_value, prepare,
            read_string, wide, write_string,
        };

        #[test]
        fn prepare_switches_off_the_error_dialogs() {
            prepare();
            // SAFETY: GetErrorMode takes nothing and returns flags.
            let mode = unsafe { GetErrorMode() };
            assert_eq!(mode & QUIET, QUIET, "error mode {mode:#x}");
        }

        /// A key under `HKEY_CURRENT_USER` of this test run's own, removed
        /// when it is dropped, so a failing assertion leaves nothing
        /// behind. The user's real `Path` and record are never touched by a
        /// test.
        struct ScratchKey(String);

        impl ScratchKey {
            fn new(test: &str) -> Self {
                Self(format!(
                    r"Software\gglib-tests\{test}-{}",
                    std::process::id()
                ))
            }

            /// The bytes `value` takes as stored, read raw: `RegGetValueW`
            /// adds a NUL to a string stored without one, `RegQueryValueExW`
            /// does not.
            fn stored_bytes(&self, value: &str) -> u32 {
                let key = wide(&self.0);
                let name = wide(value);
                let mut handle: HKEY = ptr::null_mut();
                // SAFETY: `key` is NUL-terminated UTF-16 that outlives the
                // call, which writes the opened key's handle to `handle`.
                let opened = unsafe {
                    RegOpenKeyExW(
                        HKEY_CURRENT_USER,
                        key.as_ptr(),
                        0,
                        KEY_QUERY_VALUE,
                        &raw mut handle,
                    )
                };
                assert_eq!(opened, ERROR_SUCCESS, "opening {}", self.0);
                let mut size: u32 = 0;
                // SAFETY: `handle` was opened above with KEY_QUERY_VALUE;
                // `name` is NUL-terminated UTF-16. With no buffer the call
                // writes only the data's size to `size`.
                let queried = unsafe {
                    RegQueryValueExW(
                        handle,
                        name.as_ptr(),
                        ptr::null(),
                        ptr::null_mut(),
                        ptr::null_mut(),
                        &raw mut size,
                    )
                };
                // SAFETY: `handle` is open and closed only here.
                unsafe { RegCloseKey(handle) };
                assert_eq!(queried, ERROR_SUCCESS, "querying {value}");
                size
            }
        }

        impl Drop for ScratchKey {
            fn drop(&mut self) {
                for made in [self.0.as_str(), r"Software\gglib-tests"] {
                    let name = wide(made);
                    // SAFETY: `name` is NUL-terminated UTF-16 that outlives
                    // the call, which only reads it. The call removes a key
                    // with no subkeys and refuses one with any, so the
                    // parent goes only when no other run still has its key
                    // there.
                    unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, name.as_ptr()) };
                }
            }
        }

        /// The bytes a string of `text` takes stored with exactly one NUL.
        fn with_one_nul(text: &str) -> u32 {
            u32::try_from((text.encode_utf16().count() + 1) * 2).expect("fits")
        }

        /// A long value is stored and read back whole, unexpanded, with its
        /// type and one terminating NUL; an empty one is stored empty.
        #[test]
        fn a_long_string_value_is_stored_and_read_back_whole() {
            let key = ScratchKey::new("user-path");
            let long = (0..400)
                .map(|n| format!(r"%USERPROFILE%\tools\number-{n:04}"))
                .collect::<Vec<_>>()
                .join(";");
            assert!(long.len() > 8192);

            write_string(&key.0, "Path", &long, true).expect("stored");
            assert_eq!(key.stored_bytes("Path"), with_one_nul(&long));
            assert_eq!(
                read_string(&key.0, "Path").expect("read"),
                Some((long, true))
            );

            write_string(&key.0, "Path", r"C:\plain", false).expect("stored");
            assert_eq!(key.stored_bytes("Path"), with_one_nul(r"C:\plain"));
            assert_eq!(
                read_string(&key.0, "Path").expect("read"),
                Some((r"C:\plain".to_owned(), false))
            );

            write_string(&key.0, "Path", "", true).expect("stored empty");
            assert_eq!(key.stored_bytes("Path"), with_one_nul(""));
            assert_eq!(
                read_string(&key.0, "Path").expect("read"),
                Some((String::new(), true))
            );
        }

        /// The record's value is written as `REG_SZ`, read back, and
        /// removed, and removing it again is no error.
        #[test]
        fn a_record_value_is_stored_read_and_removed() {
            let key = ScratchKey::new("path-record");
            let record = r"C:\Users\me\AppData\Local\gglib-gui;D:\gglib-gui";

            write_string(&key.0, "PathEntriesAdded", record, false).expect("stored");
            assert_eq!(key.stored_bytes("PathEntriesAdded"), with_one_nul(record));
            assert_eq!(
                read_string(&key.0, "PathEntriesAdded").expect("read"),
                Some((record.to_owned(), false))
            );

            delete_value(&key.0, "PathEntriesAdded").expect("removed");
            assert_eq!(read_string(&key.0, "PathEntriesAdded").expect("read"), None);
            delete_value(&key.0, "PathEntriesAdded").expect("removing nothing is no error");
        }

        // --- The taskbar's colour (#2050) ---

        mod taskbar {
            use windows_sys::Win32::System::Registry::{REG_DWORD, RegSetKeyValueW};

            use super::super::taskbar::{PERSONALIZE, SYSTEM_USES_LIGHT_THEME, read_dword};
            use super::super::{ERROR_SUCCESS, HKEY_CURRENT_USER, wide, write_string};
            use super::ScratchKey;

            /// Store `data` as the `REG_DWORD` value `value` of `key`.
            fn write_dword(key: &ScratchKey, value: &str, data: u32) {
                let subkey = wide(&key.0);
                let name = wide(value);
                // SAFETY: the names are NUL-terminated UTF-16 and `data` is
                // four bytes, all outliving the call, which only reads
                // them. A missing key "is created" (RegSetKeyValueW).
                let status = unsafe {
                    RegSetKeyValueW(
                        HKEY_CURRENT_USER,
                        subkey.as_ptr(),
                        name.as_ptr(),
                        REG_DWORD,
                        (&raw const data).cast(),
                        4,
                    )
                };
                assert_eq!(status, ERROR_SUCCESS, "storing {value}");
            }

            /// A 32-bit value is read back as stored, a missing one is
            /// `None`, and a string where a number belongs is an error.
            #[test]
            fn a_dword_value_is_read_back_and_a_missing_one_is_none() {
                let key = ScratchKey::new("taskbar-theme");

                write_dword(&key, "SystemUsesLightTheme", 1);
                assert_eq!(
                    read_dword(&key.0, "SystemUsesLightTheme").expect("read"),
                    Some(1)
                );
                write_dword(&key, "SystemUsesLightTheme", 0);
                assert_eq!(
                    read_dword(&key.0, "SystemUsesLightTheme").expect("read"),
                    Some(0)
                );

                assert_eq!(read_dword(&key.0, "Absent").expect("read"), None);

                write_string(&key.0, "Text", "1", false).expect("stored");
                assert!(read_dword(&key.0, "Text").is_err());
            }

            /// The machine's own answer reads without error: a value of 0
            /// or 1, or none on a Windows that keeps none.
            #[test]
            fn the_taskbar_mode_reads() {
                let value = read_dword(PERSONALIZE, SYSTEM_USES_LIGHT_THEME)
                    .expect("the value reads, or is absent");
                assert!(matches!(value, None | Some(0 | 1)), "{value:?}");
            }
        }
    }
}
