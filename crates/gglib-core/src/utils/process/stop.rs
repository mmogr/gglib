//! Hearing that this process is asked to stop, and how long it has to.
//!
//! [`stop_requested`] is the same call on every OS. What it listens for:
//!
//! | Event | OS | [`Pace`] |
//! |---|---|---|
//! | Ctrl-C | every OS | orderly |
//! | SIGTERM (a service manager, `kill`) | macOS, Linux | orderly |
//! | Ctrl-Break | Windows | orderly |
//! | its console window closed | Windows | hurried |
//! | the user logging off, the system shutting down | Windows | hurried |
//!
//! A closed console gives the process about five seconds: Windows ends it
//! then, whatever it is doing. While something listens for the close (or for
//! a logoff or shutdown), tokio's handler holds the event, so the process
//! lives until it exits or those seconds run out; with nothing listening,
//! the process ends at once.
//!
//! Logoff and shutdown are listened for, but Windows does not send them to
//! every process. A console program that loads `user32.dll`, as gglib does,
//! is sent neither (Microsoft's `HandlerRoutine` documentation); it would
//! hear them as a window's `WM_QUERYENDSESSION` and `WM_ENDSESSION`, and
//! this module has no window.
//!
//! SIGHUP, which a closed terminal sends on macOS and Linux, is not listened
//! for: the default ends the process, as it always has.

use std::io;

/// How long a stop leaves the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pace {
    /// The process may take its time over its teardown: Ctrl-C, SIGTERM,
    /// Ctrl-Break.
    Orderly,
    /// The process has seconds before the OS ends it: its console window
    /// was closed, or the session is ending. Whatever must not be left
    /// running goes first.
    Hurried,
}

/// Resolve when this process is asked to stop, with how long it has.
///
/// Every listener is in place when this is first polled; a listener that
/// cannot be made is logged and never fires, and the others still do.
pub async fn stop_requested() -> Pace {
    imp::stop_requested().await
}

/// Wait for `listener`'s first event, or forever when it could not be made
/// or can hear no more.
async fn heard<L, F>(listener: io::Result<L>, what: &str, recv: F)
where
    F: AsyncFnOnce(&mut L) -> Option<()>,
{
    match listener {
        Ok(mut listener) => {
            if recv(&mut listener).await.is_some() {
                return;
            }
            tracing::warn!("stopped listening for {what}");
        }
        Err(e) => tracing::warn!("could not listen for {what} ({e}); it will not stop gglib"),
    }
    std::future::pending::<()>().await;
}

#[cfg(unix)]
mod imp {
    use super::{Pace, heard};
    use tokio::signal::unix::{SignalKind, signal};

    pub(super) async fn stop_requested() -> Pace {
        let interrupt = signal(SignalKind::interrupt());
        let terminate = signal(SignalKind::terminate());
        tokio::select! {
            () = heard(interrupt, "Ctrl-C", async |s| s.recv().await) => Pace::Orderly,
            () = heard(terminate, "SIGTERM", async |s| s.recv().await) => Pace::Orderly,
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::{Pace, heard};
    use tokio::signal::windows;

    pub(super) async fn stop_requested() -> Pace {
        let interrupt = windows::ctrl_c();
        let brk = windows::ctrl_break();
        let close = windows::ctrl_close();
        let logoff = windows::ctrl_logoff();
        let shutdown = windows::ctrl_shutdown();
        tokio::select! {
            () = heard(close, "the console closing", async |s| s.recv().await) => Pace::Hurried,
            () = heard(logoff, "a logoff", async |s| s.recv().await) => Pace::Hurried,
            () = heard(shutdown, "a shutdown", async |s| s.recv().await) => Pace::Hurried,
            () = heard(interrupt, "Ctrl-C", async |s| s.recv().await) => Pace::Orderly,
            () = heard(brk, "Ctrl-Break", async |s| s.recv().await) => Pace::Orderly,
        }
    }
}
