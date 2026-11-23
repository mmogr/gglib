//! GUI-oriented process lifecycle management.
//!
//! This module provides process spawning, tracking, and management with
//! integrated log streaming and event broadcasting.
//!
//! [`GuiProcessCore`] is the only process core, and it serves every caller,
//! not only the GUI ([#849]).
//!
//! [#849]: https://github.com/mmogr/gglib/pull/849

use super::exit_report::{self, StopReason};
use super::output_tail::OutputTail;
use super::ports::{allocate_port, is_port_available};
use super::shutdown::shutdown_child;
use super::spawn_config::{RuntimeBinaries, SpawnConfig};
use super::types::{RunningProcess, ServerInfo};
use crate::command::{build_and_spawn, spawn_log_readers};
use crate::pidfile::{delete_pidfile, write_pidfile};
use crate::sd::build_and_spawn_sd;
use anyhow::{Result, anyhow};
use gglib_core::domain::RuntimeKind;
use gglib_core::utils::process::tied::tie_to_this_process;
use std::collections::HashMap;
use std::path::Path;
use std::process::ExitStatus;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

/// How a child ended, as [`GuiProcessCore::exited`] reads it.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Exit {
    /// It exited with this status.
    Status(ExitStatus),
    /// It is gone and there is no status to give: it is untracked, or its
    /// status could not be read.
    Unknown,
}

/// GUI-oriented process lifecycle manager.
///
/// Handles spawning, tracking, and killing model servers, llama-server and
/// `sd-server` alike, with integrated log streaming for GUI applications.
/// Uses `u32` model IDs for frontend compatibility.
///
/// The only process core. See the module docs above for why the name still
/// carries a `Gui` prefix.
pub struct GuiProcessCore {
    /// Running processes keyed by `model_id`
    processes: HashMap<u32, RunningProcess>,
    /// Base port for allocation
    base_port: u16,
    /// The program each runtime is started with.
    binaries: RuntimeBinaries,
    /// How each child [`Self::cleanup_dead`] took out of `processes` ended,
    /// by model id, until [`Self::exited`] reads it or the id is spawned
    /// again: another launch's cleanup can reap a child whose own launch has
    /// not yet looked, and that launch still says how it ended.
    reaped: HashMap<u32, ExitStatus>,
}

impl GuiProcessCore {
    /// Create a new `GuiProcessCore`
    pub fn new(base_port: u16, binaries: RuntimeBinaries) -> Self {
        Self {
            processes: HashMap::new(),
            base_port,
            binaries,
            reaped: HashMap::new(),
        }
    }

    /// Spawn a server: a llama-server or an `sd-server`, as `config` says,
    /// from the binary this core holds for that runtime.
    ///
    /// Returns the port it was allocated **and** its process id. The pid comes
    /// back with the port because a caller that owns a freshly spawned child
    /// needs its identity to clean it up, and reading it afterwards means a
    /// second lock acquisition with an `await` in between — a suspension point
    /// at which the child exists and nothing owns it. Handing both out of the
    /// same critical section removes the window rather than arguing it is
    /// small.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a model id is a database row id the core keys by u32; one outside u32 is cast as it always has been, which this lint pass leaves alone"
    )]
    pub fn spawn(&mut self, config: SpawnConfig) -> Result<(u16, u32)> {
        let model_id = config.model_id() as u32;
        let runtime = config.runtime();

        if self.processes.contains_key(&model_id) {
            return Err(anyhow!("Model {model_id} is already running"));
        }
        // What an earlier child under this id ended with is not this one's.
        self.reaped.remove(&model_id);

        if !config.model_path().exists() {
            return Err(anyhow!(
                "Model file not found: {}",
                config.model_path().display()
            ));
        }

        let port = self.resolve_port(config.port())?;
        // A diagnostic, added with the proxy dashboard (#568), for a
        // port-mismatch report: logs the configured base port alongside the
        // port actually allocated for this spawn, so a repro can confirm
        // whether `--llama-port` reached this point correctly. Safe to remove
        // once that report is resolved or a root cause is found elsewhere.
        tracing::info!(
            model_id = %model_id,
            base_port = %self.base_port,
            requested_port = ?config.port(),
            allocated_port = %port,
            runtime = runtime.label(),
            "GuiProcessCore::spawn allocated port"
        );
        let binary = self.binaries.for_runtime(runtime);
        let child = match &config {
            SpawnConfig::Llama(llama) => build_and_spawn(Some(binary), llama, port)?,
            SpawnConfig::Sd(sd) => build_and_spawn_sd(binary, sd, port)?,
        };
        // So it ends with this process even if this process dies without
        // its teardown (#1985).
        if let Err(e) = tie_to_this_process(&child) {
            warn!(
                model_id = %model_id,
                runtime = runtime.label(),
                error = %e,
                "the server is not tied to this process: if gglib dies without its teardown, \
                 it keeps running until the next start"
            );
        }
        let pid = child
            .id()
            .ok_or_else(|| anyhow!("Failed to get child PID"))?;

        // Write PID file
        if let Err(e) = write_pidfile(config.model_id(), pid, port) {
            debug!("Failed to write PID file: {}", e);
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let info = ServerInfo::new(model_id, config.into_model_name(), pid, port, now, runtime);
        let mut running = RunningProcess::new(info, child);
        spawn_log_readers(&mut running.child, port, &running.output);
        self.processes.insert(model_id, running);

        Ok((port, pid))
    }

    fn resolve_port(&self, requested: Option<u16>) -> Result<u16> {
        match requested {
            Some(p) if p < 1024 => Err(anyhow!(
                "Port {p} is a privileged port. Please use a port >= 1024."
            )),
            Some(p) if !is_port_available(p) => Err(anyhow!(
                "Port {p} is already in use. Please choose a different port."
            )),
            Some(p) => Ok(p),
            None => {
                let used: Vec<u16> = self.processes.values().map(|p| p.info.port).collect();
                allocate_port(self.base_port, &used)
            }
        }
    }

    /// The binary that serves `runtime`, whether or not it is installed.
    #[must_use]
    pub fn binary(&self, runtime: RuntimeKind) -> &Path {
        self.binaries.for_runtime(runtime)
    }

    /// Kill a running process with graceful shutdown, and say in gglib's
    /// log that it was stopped and `why`; or, if it had already exited by
    /// itself, how it ended and what it printed last.
    pub async fn kill(&mut self, model_id: u32, why: StopReason) -> Result<()> {
        let mut running = self
            .processes
            .remove(&model_id)
            .ok_or_else(|| anyhow!("Model {model_id} is not running"))?;

        let pid = running.info.pid;
        debug!(model_id = %model_id, pid = %pid, port = %running.info.port, "Stopping process");

        // Read before it is signalled, which would end it either way.
        let ended_by_itself = match running.child.try_wait() {
            Ok(Some(status)) => Some(status),
            Ok(None) | Err(_) => None,
        };
        let (info, output, told) = (running.info.clone(), running.output.clone(), running.told);

        // Use graceful shutdown with SIGTERM → SIGKILL
        let _ = shutdown_child(running.child).await;

        match ended_by_itself {
            _ if told => {}
            Some(status) => exit_report::exited_by_itself(&info, status, output),
            None => exit_report::stopped(&info, why),
        }

        // Remove PID file
        if let Err(e) = delete_pidfile(i64::from(model_id)) {
            debug!("Failed to delete PID file: {}", e);
        }

        Ok(())
    }

    /// Kill every child outright and wait for none of them: for a process
    /// with seconds left, such as the service after its console window is
    /// closed on Windows. Each still running is told of as stopped because
    /// gglib is shutting down, after every kill is sent.
    ///
    /// Each child is killed through its own handle: `start_kill` is SIGKILL
    /// on Unix and `TerminateProcess` on Windows, needs no runtime, and
    /// signals nothing once the handle has reaped its child, so no pid the OS
    /// may since have handed on is touched. The children stay tracked and
    /// their pidfiles stay: a [`Self::kill`] afterwards reaps the child and
    /// deletes its pidfile as it always does, and if the process ends first
    /// the next start's sweep finds the pidfile.
    pub fn kill_all_now(&mut self) {
        let mut stopped = Vec::new();
        for (model_id, running) in &mut self.processes {
            // One that has already exited is left to be told of as an exit
            // by itself.
            if !running.told && matches!(running.child.try_wait(), Ok(None)) {
                running.told = true;
                stopped.push(running.info.clone());
            }
            if let Err(e) = running.child.start_kill() {
                debug!(model_id = %model_id, error = %e, "could not kill the child at once");
            }
        }
        for info in &stopped {
            exit_report::stopped(info, StopReason::Shutdown);
        }
    }

    /// Stop the child tracked for `model_id`, but only if it is still the one
    /// with `pid`.
    ///
    /// Returns whether anything was stopped. A mismatch is the ABA case and is
    /// not an error: the process this caller owned is already gone, and what
    /// holds the id now belongs to somebody else. Killing it because the names
    /// match would take down a healthy server that had just replaced it.
    pub async fn kill_if_pid(&mut self, model_id: u32, pid: u32, why: StopReason) -> Result<bool> {
        if self.processes.get(&model_id).map(|p| p.info.pid) != Some(pid) {
            return Ok(false);
        }
        self.kill(model_id, why).await?;
        Ok(true)
    }

    /// Whether the child tracked for `model_id` has already exited.
    ///
    /// Reaps as it asks — `try_wait` is how a child stops being a zombie — so
    /// this is `&mut self`. An untracked id reads as exited: either it never
    /// started or something has already cleaned it up, and both mean "stop
    /// waiting for it".
    ///
    /// Reaping here is also what makes a later kill safe: once `try_wait` has
    /// collected the child, `tokio::process::Child::id()` returns `None`, so
    /// `shutdown_child` sends no signal at all rather than signalling a pid the
    /// OS may since have recycled. A refactor that stored the raw pid instead
    /// of the `Child`, or reaped with a bare `waitpid`, would lose that.
    ///
    /// Exists so a launch can stop polling `/health` the moment the server it
    /// is polling for is gone. Without it a llama-server that dies on startup
    /// — bad arguments, OOM, a missing GPU library — is waited out for the
    /// whole launch budget, which is minutes for a large model.
    pub fn has_exited(&mut self, model_id: u32) -> bool {
        self.exited(model_id).is_some()
    }

    /// How the child tracked for `model_id` ended, once it has: what
    /// [`Self::has_exited`] answers, with the status it exited with.
    /// A child [`Self::cleanup_dead`] already took is read from what it
    /// kept, once.
    pub(crate) fn exited(&mut self, model_id: u32) -> Option<Exit> {
        let Some(running) = self.processes.get_mut(&model_id) else {
            return Some(
                self.reaped
                    .remove(&model_id)
                    .map_or(Exit::Unknown, Exit::Status),
            );
        };
        match running.child.try_wait() {
            Ok(None) => None,
            Ok(Some(status)) => Some(Exit::Status(status)),
            Err(_) => Some(Exit::Unknown),
        }
    }

    /// Tell gglib's log that the child `pid`, filed under `model_id`, exited
    /// by itself, if it has and nothing has told of it yet. Answers whether
    /// it is still running, so [`exit_report::watch`] knows to look again.
    ///
    /// The child stays tracked: a stop asked of it afterwards still finds
    /// it, and says nothing more.
    pub(crate) fn tell_if_exited(&mut self, model_id: u32, pid: u32) -> bool {
        let Some(running) = self.processes.get_mut(&model_id) else {
            return false;
        };
        if running.info.pid != pid || running.told {
            return false;
        }
        match running.child.try_wait() {
            Ok(None) => true,
            Ok(Some(status)) => {
                running.told = true;
                exit_report::exited_by_itself(&running.info, status, running.output.clone());
                false
            }
            Err(_) => false,
        }
    }

    /// The last lines the child tracked for `model_id` printed, kept as it
    /// runs and after it exits.
    pub(crate) fn output_of(&self, model_id: u32) -> Option<OutputTail> {
        self.processes
            .get(&model_id)
            .map(|running| running.output.clone())
    }

    /// List all running processes
    pub fn list_all(&self) -> Vec<&ServerInfo> {
        debug!(process_count = %self.processes.len(), "GuiProcessCore: list_all called");
        self.processes.values().map(|p| &p.info).collect()
    }

    /// Check if a model is running.
    ///
    /// Test-only, gated so `dead_code` keeps telling the truth about
    /// production reach.
    #[cfg(test)]
    pub(crate) fn is_running(&self, model_id: u32) -> bool {
        self.processes.contains_key(&model_id)
    }

    /// Get count of running processes.
    ///
    /// Test-only: nothing outside the test below calls it. Gated for the same
    /// reason as [`Self::is_running`].
    #[cfg(test)]
    pub(crate) fn count(&self) -> usize {
        self.processes.len()
    }

    /// Remove dead processes from tracking and clean PID files, telling
    /// gglib's log of each exit nothing has told of yet.
    pub fn cleanup_dead(&mut self) -> Vec<u32> {
        debug!(process_count = %self.processes.len(), "cleanup_dead called");
        let mut dead = Vec::new();

        for (id, running) in &mut self.processes {
            match running.child.try_wait() {
                Ok(Some(status)) => {
                    debug!(id = %id, status = ?status, "Process exited");
                    dead.push(*id);
                    self.reaped.insert(*id, status);
                }
                Ok(None) => {}
                Err(e) => {
                    // Deliberately *not* `dead.push`. Dropping the entry is the
                    // only way to lose the handle to a still-running child, and
                    // "I could not read its status" is not "it exited" — that
                    // is the same inversion the launch guard exists to correct.
                    // Keep tracking it so a later kill can still reach it.
                    warn!(id = %id, error = %e, "could not read process status; still tracking it");
                }
            }
        }

        for id in &dead {
            debug!(id = %id, "Removing dead process from map");
            if let Some(running) = self.processes.remove(id)
                && !running.told
                && let Some(status) = self.reaped.get(id)
            {
                exit_report::exited_by_itself(&running.info, *status, running.output);
            }
            // Remove PID file for naturally exited process
            if let Err(e) = delete_pidfile(i64::from(*id)) {
                debug!("Failed to delete PID file for {}: {}", id, e);
            }
        }

        debug!(removed_count = %dead.len(), remaining_count = %self.processes.len(), "cleanup_dead finished");
        dead
    }
}

// Drop cannot await, so the SIGTERM-then-SIGKILL sequence `kill` uses cannot
// run here; anything wanting a graceful stop goes through `kill` first. This
// backstop kills outright, as `kill_all_now` does. The pidfiles stay, so the
// next sweep can still find a child this failed to stop.
impl Drop for GuiProcessCore {
    fn drop(&mut self) {
        self.kill_all_now();
    }
}

#[cfg(test)]
#[path = "core_drop_tests.rs"]
pub(in crate::process) mod drop_tests;

#[cfg(test)]
#[path = "core_spawn_tests.rs"]
mod spawn_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_creation() {
        let core = GuiProcessCore::new(8080, RuntimeBinaries::llama_only("llama-server"));
        assert_eq!(core.count(), 0);
    }

    #[test]
    fn test_is_running() {
        let core = GuiProcessCore::new(8080, RuntimeBinaries::llama_only("llama-server"));
        assert!(!core.is_running(1));
    }

    #[test]
    fn test_list_all_empty() {
        let core = GuiProcessCore::new(8080, RuntimeBinaries::llama_only("llama-server"));
        assert_eq!(core.list_all().len(), 0);
    }
}
