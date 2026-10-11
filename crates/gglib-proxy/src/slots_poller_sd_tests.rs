//! An `sd-server` in the primary slot is not polled: it has no `/slots` and
//! no `/props`. A llama-server there is, which shows the poller reached the
//! port at all.

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use gglib_core::domain::RuntimeKind;
use gglib_core::ports::{
    Admission, LaunchOverrides, ModelRuntimeError, ModelRuntimePort, RunningTarget,
};
use reqwest::Client;
use tokio_util::sync::CancellationToken;

use super::{SlotsCache, spawn_slots_poller};
use crate::connections::ActiveConnectionsRegistry;
use crate::sampling_audit::SamplingAuditStore;

/// A runtime whose primary slot holds `target`.
#[derive(Debug)]
struct Primary(RunningTarget);

#[async_trait]
impl ModelRuntimePort for Primary {
    async fn admit(
        &self,
        model: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        Err(ModelRuntimeError::ModelNotFound(model.to_owned()))
    }
    async fn current_model(&self) -> Option<RunningTarget> {
        Some(self.0.clone())
    }
    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }

    async fn stop_model(&self, _model_id: u32) -> Result<bool, ModelRuntimeError> {
        Ok(false)
    }
}

/// A server that reads each request head, answers 404, and counts.
fn counting() -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let asked = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&asked);
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).is_ok_and(|n| n == 1) {
                head.push(byte[0]);
            }
            count.fetch_add(1, Ordering::SeqCst);
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        }
    });
    (port, asked)
}

/// How many requests the poller sent `runtime`'s server in its first pass.
async fn asked_of(runtime: RuntimeKind) -> usize {
    let (port, asked) = counting();
    let target = RunningTarget::local(port, 1, "m".to_owned(), 4096, false).with_runtime(runtime);
    let cancel = CancellationToken::new();
    let handle = spawn_slots_poller(
        Arc::new(Primary(target)),
        Client::new(),
        Arc::new(SlotsCache::new()),
        Arc::new(ActiveConnectionsRegistry::new()),
        Arc::new(SamplingAuditStore::new()),
        cancel.clone(),
    );
    tokio::time::sleep(Duration::from_millis(500)).await;
    cancel.cancel();
    let _ = handle.await;
    asked.load(Ordering::SeqCst)
}

#[tokio::test]
async fn an_sd_server_in_the_primary_is_not_polled() {
    assert_eq!(asked_of(RuntimeKind::StableDiffusion).await, 0);
}

#[tokio::test]
async fn a_llama_server_in_the_primary_is() {
    assert!(asked_of(RuntimeKind::Llama).await > 0);
}
