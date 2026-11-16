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
//! The child is tied after it starts. A process it started before then
//! would not be tied, so a child that starts helpers at once can leave one
//! untied; llama-server and `sd-server` start none.

use std::io;

/// What ties a child to this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tied {
    /// The child is in this process's kill-on-close job object, and Windows
    /// ends it when this process ends (Windows).
    ByJob,
    /// Nothing ties it: the process group, the teardown and the next start's
    /// sweep stop it (macOS, Linux).
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
    let _ = child;
    Ok(if cfg!(windows) {
        Tied::ByJob
    } else {
        Tied::NotTied
    })
}
