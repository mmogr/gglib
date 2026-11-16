//! Tying a child to this process, so it ends when this process ends,
//! however it ends (#1985).
//!
//! The service starts llama-server and `sd-server`, and its teardown stops
//! them. A service that dies without its teardown (killed with
//! `taskkill /F`, crashed, its console closed while it was already
//! stopping) used to leave them running on Windows, holding gigabytes until
//! the next start's sweep found their pidfiles.
//!
//! [`tie_to_this_process`] is the same call on every OS; what it promises is
//! the [`Tied`] it returns:
//!
//! - **Windows: [`Tied::ByJob`].** This process keeps one job object, made on
//!   the first call, that ends every process in it when its last handle
//!   closes (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`). Only this process holds a
//!   handle to it, and Windows closes every handle of a process that ends, so
//!   the child ends with this process whatever ends it.
//! - **macOS and Linux: [`Tied::NotTied`], by decision.** The engines keep
//!   their process groups, the teardown stops them, and the orphan sweep at
//!   the next start stops any a dead service left.
//!
//! Whoever starts a child ties it, so the job belongs to the process that
//! runs it: the engines run only in the service (`gglib daemon run`, or the
//! desktop app when it hosts the service itself). An MCP server is started
//! with [`super::tree::spawn`] instead, whose job of the server's own ties
//! it the same way. This process is not put in its own job. A process that
//! never ties a child makes no job.
//!
//! A child already in a job is still tied: Windows nests the jobs (since
//! Windows 8), as for a service started under Windows' SSH server or a
//! terminal that runs its programs in one. If tying fails anyway, the error
//! says why, the child runs on untied as it did before this existed, and
//! the caller logs a warning.
//!
//! The child is tied after it starts. A process it started before then
//! would not be tied, so a child that starts helpers at once can leave one
//! untied; llama-server and `sd-server` start none.
//!
//! Tied means everything a tied child starts, for as long as it runs. A
//! helper that asks Windows to start outside its parent's jobs is refused,
//! since the job does not allow breaking away.
//!
//! Whether a service that its starter's job ends (Windows' SSH server ends
//! every process of a session when it closes) should outlive it is a
//! separate matter, not handled here.

use std::io;

/// What ties a child to this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tied {
    /// The child is in this process's kill-on-close job object, and Windows
    /// ends it when this process ends (Windows).
    ByJob,
    /// Nothing ties it, and stopping it is the caller's: for an engine, the
    /// process group, the teardown and the next start's sweep (macOS,
    /// Linux).
    NotTied,
}

/// Tie `child`, just started, to this process, so it ends when this process
/// ends, however it ends.
///
/// # Errors
///
/// When the child could not be tied where this OS ties it (on Windows, the
/// job could not be made or the child could not be put in it). The child
/// keeps running, untied, as if this had not been called; the caller logs it.
pub fn tie_to_this_process(child: &tokio::process::Child) -> io::Result<Tied> {
    #[cfg(windows)]
    {
        super::host::tie_by_job(child)?;
        Ok(Tied::ByJob)
    }
    #[cfg(not(windows))]
    {
        let _ = child;
        Ok(Tied::NotTied)
    }
}
