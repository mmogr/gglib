//! A stand-in for the daemon's agent-run starter, for the proxy's
//! `PUT /v1/runs/{id}?kind=agent` tests. Records every turn it is handed and
//! answers from a script.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use gglib_core::domain::hub_chats::HubTurn;
use gglib_core::domain::runs::RunKind;
use gglib_core::ports::{AgentRunStarter, Created, RunsPort, TurnRefused};
use gglib_core::{CorsConfig, DevicePorts, ProxyAccessConfig};

use super::runs::{FakeRuns, info};

/// A chat run with builtins as the stub was handed it: `(device, id, body,
/// draw, servers)`.
pub(crate) type ChatStarted = (String, String, serde_json::Value, bool, BTreeSet<i64>);

/// The stub.
#[derive(Debug, Default)]
pub(crate) struct FakeTurns {
    /// Every turn, as `(device, id, turn)`.
    pub(crate) started: Mutex<Vec<(String, String, HubTurn)>>,
    /// Refuse every turn with this, when set.
    pub(crate) fail: Mutex<Option<TurnRefused>>,
    /// Every chat run with builtins, in order.
    pub(crate) chats: Mutex<Vec<ChatStarted>>,
    /// The image model each of those chose, in the same order.
    pub(crate) chose: Mutex<Vec<Option<u32>>>,
}

#[async_trait]
impl AgentRunStarter for FakeTurns {
    async fn start(&self, device: &str, id: &str, turn: HubTurn) -> Result<Created, TurnRefused> {
        self.started
            .lock()
            .unwrap()
            .push((device.to_owned(), id.to_owned(), turn.clone()));
        let fail = self.fail.lock().unwrap().clone();
        if let Some(refusal) = fail {
            return Err(refusal);
        }
        let mut info = info(id);
        info.kind = RunKind::Agent;
        info.device = Some(device.to_owned());
        info.conversation_id = Some(turn.conversation_id);
        Ok(Created {
            info,
            created: true,
        })
    }

    async fn start_chat(
        &self,
        device: &str,
        id: &str,
        body: serde_json::Value,
        draw: bool,
        image_model: Option<u32>,
        servers: BTreeSet<i64>,
    ) -> Result<Created, TurnRefused> {
        self.chose.lock().unwrap().push(image_model);
        self.chats
            .lock()
            .unwrap()
            .push((device.to_owned(), id.to_owned(), body, draw, servers));
        let fail = self.fail.lock().unwrap().clone();
        if let Some(refusal) = fail {
            return Err(refusal);
        }
        let mut info = info(id);
        info.device = Some(device.to_owned());
        info.frames = gglib_core::domain::runs::RunFrames::Agent;
        Ok(Created {
            info,
            created: true,
        })
    }
}

/// The real proxy holding `turns` and chat runs, demanding no key.
pub(crate) async fn serve(
    turns: Option<Arc<FakeTurns>>,
    runs: Arc<FakeRuns>,
) -> (String, tokio_util::sync::CancellationToken) {
    let access = ProxyAccessConfig::new(CorsConfig::LocalOnly, None, "127.0.0.1", vec![])
        .with_devices(DevicePorts {
            runs: Some(runs as Arc<dyn RunsPort>),
            turns: turns.map(|t| t as Arc<dyn AgentRunStarter>),
            ..DevicePorts::default()
        });
    let (base, _, cancel) = super::access::spawn_proxy(access).await;
    (base, cancel)
}
