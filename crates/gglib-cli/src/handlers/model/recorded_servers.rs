//! What is being served under this data root, as a one-shot command sees it.
//!
//! Whatever starts a llama-server keeps a pid file for it under the data
//! root, named for the model it serves (`gglib_runtime::pidfile`). A command
//! in a terminal is a process apart from that one, with no `ProcessManager`
//! to ask, so it reads the records.

use async_trait::async_trait;
use gglib_core::ports::{
    Admission, LaunchOverrides, ModelRuntimeError, ModelRuntimePort, ProcessHandle, RunningTarget,
};
use gglib_runtime::pidfile::{is_our_llama_server, list_pidfiles};

/// The llama-servers recorded under this data root that are still running.
///
/// A [`ModelRuntimePort`] that answers [`list_running`] and nothing else:
/// what it lists belongs to another process, so it starts nothing, routes
/// nothing and stops nothing.
///
/// The records are this data root's alone: the directory [`list_pidfiles`]
/// reads is under the root this command's database is under, so the model
/// ids in the two mean the same models. A daemon running from another data
/// root is not seen here, and nothing here is seen from there.
///
/// A record counts only while its pid is a running process of the
/// llama-server this installation manages ([`is_our_llama_server`]). A pid
/// file outlives a server that was killed, and its pid may since belong to
/// anything.
///
/// [`list_running`]: ModelRuntimePort::list_running
#[derive(Debug)]
pub(crate) struct RecordedServers;

#[async_trait]
impl ModelRuntimePort for RecordedServers {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        Err(ModelRuntimeError::Internal(
            "a one-shot command starts no model".to_string(),
        ))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn list_running(&self) -> Vec<ProcessHandle> {
        let records = match list_pidfiles() {
            Ok(records) => records,
            Err(e) => {
                tracing::warn!(
                    "could not read the pid files that say which models are being served, \
                     so none is taken to be: {e}"
                );
                return Vec::new();
            }
        };
        records
            .into_iter()
            .filter(|(_, record)| is_our_llama_server(record.pid))
            // A pid file holds neither the model's name nor when its server
            // started.
            .map(|(model_id, record)| {
                ProcessHandle::new(model_id, String::new(), Some(record.pid), record.port, 0)
            })
            .collect()
    }

    /// Refused, where [`NoopModelRuntime`] answers `Ok`: a caller told the
    /// server had stopped would go on to remove a model that is still being
    /// served.
    ///
    /// [`NoopModelRuntime`]: gglib_core::ports::NoopModelRuntime
    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Err(ModelRuntimeError::Internal(
            "a one-shot command does not stop a server another process started".to_string(),
        ))
    }
}

#[cfg(test)]
#[path = "recorded_servers_tests.rs"]
mod tests;
