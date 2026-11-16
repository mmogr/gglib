//! Starting a child as the root of a tree, so stopping it stops everything
//! it started (#2012).
//!
//! An MCP server whose command is a shim is more than one process: on
//! Windows `npx.cmd` is `cmd.exe` running `node.exe`, and on macOS and
//! Linux a wrapper script is `sh` running the program it names (`npx`
//! itself runs the server it fetches as a child). Killing the child gglib
//! started ended only the shim, and the server behind it kept running.
//!
//! [`spawn`] is the same call on every OS, and [`Tree::stop`] ends the
//! child and every process it started:
//!
//! - **Windows: a job of the child's own.** The child is started
//!   suspended, put in a new job object, and only then resumed, so every
//!   process it starts is in the job: a shim cannot start the program behind
//!   it before the job holds the shim. Stopping terminates the job. The job
//!   is kill-on-close and only this process holds a handle to it, so the
//!   tree also ends when the [`Tree`] is dropped, and when this process
//!   ends, however it ends: a tree is tied to this process as
//!   [`super::tied`] ties a child. If the job cannot be made, the child is
//!   resumed anyway, [`Tree::loose`] says why, and stopping ends the child
//!   alone, as before. If the child cannot be resumed, it is ended and
//!   [`spawn`] fails.
//! - **macOS and Linux: a process group of the child's own.** The child
//!   leads a new group, and what it starts is in the group unless it makes
//!   one of its own. Stopping sends `SIGKILL` to the group. The group is
//!   signalled only while the child is unreaped, so its id cannot have been
//!   given to another process. In a group of its own the child no longer
//!   gets the terminal's Ctrl-C or hangup: it is stopped by the process
//!   that started it, and nothing ties it to that process.
//! - **iOS: the child alone.** gglib starts no process on a phone, and a
//!   phone build links nothing that signals one (`nix` is kept out of it,
//!   `scripts/check_phone_closure.sh`), so a tree there is its child,
//!   stopped as a child is.
//!
//! Everything the child starts means a program it opens too: a browser an
//! MCP server starts as a new process for a sign-in closes when the server
//! is stopped (on Windows, and on Linux unless the browser was already
//! running; macOS's `open` hands the browser to launchd).
//!
//! Neither waits for the processes to end: [`Tree::stop`] is called from
//! `Drop`, where waiting would block a runtime thread.

use std::io;

use tokio::process::{Child, Command};

/// A child started with [`spawn`], and everything it starts.
///
/// Dropping it stops it.
#[derive(Debug)]
pub struct Tree {
    child: Child,
    /// What holds the tree together.
    whole: imp::Whole,
    /// Why stopping the tree ends only the child, when it does.
    loose: Option<io::Error>,
}

/// Start `command` as the root of a tree that [`Tree::stop`] ends whole.
///
/// `command` comes from [`super::async_cmd`].
///
/// # Errors
///
/// When the program cannot be started, or on Windows cannot be resumed
/// once it has been (it is then ended). A tree that cannot be held whole is
/// still started; [`Tree::loose`] says why it is not whole.
pub fn spawn(command: &mut Command) -> io::Result<Tree> {
    imp::prepare(command);
    let mut child = command.spawn()?;
    let (whole, loose) = match imp::hold(&child) {
        Ok(held) => held,
        Err(e) => {
            let _ = child.start_kill();
            let _ = child.try_wait();
            return Err(e);
        }
    };
    Ok(Tree {
        child,
        whole,
        loose,
    })
}

impl Tree {
    /// The child at the root, to take its pipes from. Waiting for it
    /// before [`Self::stop`] leaves what it started running on macOS and
    /// Linux, since its group is then no longer signalled.
    pub const fn child(&mut self) -> &mut Child {
        &mut self.child
    }

    /// Why stopping this tree ends only the child, or `None` when it ends
    /// everything the child started: on Windows, why the child could not be
    /// put in a job of its own. The caller logs it.
    pub fn loose(&self) -> Option<&io::Error> {
        self.loose.as_ref()
    }

    /// End the child and every process it started, without waiting for
    /// them. Stopping a stopped tree does nothing more.
    pub fn stop(&mut self) {
        imp::stop(&mut self.whole, &self.child);
        let _ = self.child.start_kill();
        // Reaps a child that has already exited; tokio reaps one that has
        // not once the `Child` is dropped.
        let _ = self.child.try_wait();
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(all(unix, not(target_os = "ios")))]
mod imp {
    use std::io;

    use nix::sys::signal::{Signal, killpg};
    use nix::unistd::Pid;
    use tokio::process::{Child, Command};

    /// A group is named by its leader's pid, which the child gives.
    pub(super) type Whole = ();

    pub(super) fn prepare(command: &mut Command) {
        #[expect(
            clippy::disallowed_methods,
            reason = "a tree's process group is set here, as the service's is in detached.rs"
        )]
        command.process_group(0);
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "the signature Windows' hold has, which can fail"
    )]
    pub(super) const fn hold(_: &Child) -> io::Result<(Whole, Option<io::Error>)> {
        Ok(((), None))
    }

    pub(super) fn stop((): &mut Whole, child: &Child) {
        // tokio gives no id once it has reaped the child. Until then the
        // child, running or a zombie, holds its pid, so the group it leads
        // is still this tree's. Group 0 would be this process's own, and 1
        // is init's.
        let Some(group) = child
            .id()
            .and_then(|pid| i32::try_from(pid).ok())
            .filter(|pid| *pid > 1)
        else {
            return;
        };
        // A group already gone (ESRCH) has nothing left to end.
        let _ = killpg(Pid::from_raw(group), Signal::SIGKILL);
    }
}

#[cfg(not(any(windows, all(unix, not(target_os = "ios")))))]
mod imp {
    use std::io;

    use tokio::process::{Child, Command};

    /// Nothing holds the tree: it is the child alone.
    pub(super) type Whole = ();

    pub(super) const fn prepare(_: &mut Command) {}

    #[expect(
        clippy::unnecessary_wraps,
        reason = "the signature Windows' hold has, which can fail"
    )]
    pub(super) const fn hold(_: &Child) -> io::Result<(Whole, Option<io::Error>)> {
        Ok(((), None))
    }

    pub(super) const fn stop((): &mut Whole, _: &Child) {}
}

#[cfg(windows)]
mod imp {
    use std::io;

    use tokio::process::{Child, Command};

    use super::super::host::{TreeJob, resume, start_suspended};

    /// The child's job, until the tree is stopped.
    pub(super) type Whole = Option<TreeJob>;

    pub(super) fn prepare(command: &mut Command) {
        start_suspended(command);
    }

    /// Put the suspended child in its job, then resume it, whether or not
    /// the job could be made.
    pub(super) fn hold(child: &Child) -> io::Result<(Whole, Option<io::Error>)> {
        let held = match TreeJob::of(child) {
            Ok(job) => (Some(job), None),
            Err(e) => (None, Some(e)),
        };
        let pid = child
            .id()
            .ok_or_else(|| io::Error::other("it exited before it could be resumed"))?;
        resume(pid)
            .map_err(|e| io::Error::new(e.kind(), format!("it could not be resumed: {e}")))?;
        Ok(held)
    }

    pub(super) fn stop(whole: &mut Whole, _: &Child) {
        if let Some(job) = whole.take() {
            // A job that cannot be terminated still ends its processes when
            // it is dropped here, closing its one handle.
            let _ = job.terminate();
        }
    }
}
