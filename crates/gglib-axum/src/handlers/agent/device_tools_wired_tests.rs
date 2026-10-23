//! Both of a paired device's doors, wired to the servers the live session
//! opens: a turn on a chat this machine keeps (`hub_turn`) and a chat the
//! phone keeps itself (`chat_turn`), each driven through its own `PUT`.
//!
//! A run's late step cannot finish in a test: its model is admitted on a
//! port where no llama-server runs, so the run ends `failed` at the port
//! check, after its tools are settled and before its loop is composed. What
//! the step leaves behind is whether the opened server was started; what
//! the run is offered is `resolve` of the run's own `Asked`, read from the
//! gateway as the late step reads it.

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::domain::chat::{ConversationSettings, NewConversation};
use gglib_core::domain::hub_chats::HubTurn;
use gglib_core::domain::runs::RunStatus;
use gglib_core::domain::{Machine, ModelRef};
use gglib_core::ports::{
    Admission, LaunchOverrides, ModelRuntimeError, ModelRuntimePort, RunScope, RunningTarget,
};
use gglib_core::{McpServerStatus, NewMcpServer};
use serde_json::json;
use tokio::sync::Notify;

use super::tests::STAND_IN_SERVER;
use super::{Asked, resolve};
use crate::handlers::agent::run_fixture::{drawing_state, settled};
use crate::handlers::agent::turn_fixture::hi;
use crate::handlers::agent::{chat_turn, hub_turn, turn_fixture};
use crate::state::AppState;

const DRAW: &str = "builtin:generate_image";

/// The port a model is admitted on, where nothing serves.
const NOWHERE: u16 = 19_555;

/// A runtime that loads what it is asked for once `loaded` is notified,
/// onto [`NOWHERE`].
#[derive(Debug, Default)]
struct Loads {
    loaded: Notify,
}

#[async_trait]
impl ModelRuntimePort for Loads {
    async fn admit(
        &self,
        model: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        self.loaded.notified().await;
        let id = model.parse().unwrap_or(0);
        let target = RunningTarget::local(NOWHERE, id, model.to_owned(), 4096, true);
        Ok(Admission::detached(target))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }

    async fn stop_model(&self, _model_id: u32) -> Result<bool, ModelRuntimeError> {
        Ok(false)
    }
}

/// A machine that can draw, whose live session opens the stand-in MCP
/// server under the name `tavily`, configured lazy and not running, and
/// whose model loads when `loads` says.
struct Opened {
    _dir: tempfile::TempDir,
    state: AppState,
    loads: Arc<Loads>,
    server: i64,
    model: i64,
}

async fn opened() -> Opened {
    let (dir, state) = drawing_state().await;
    let mut context = Arc::into_inner(state).expect("the only handle");
    let loads = Arc::new(Loads::default());
    context.runtime = Arc::clone(&loads) as Arc<dyn ModelRuntimePort>;
    let state = Arc::new(context);
    let args = vec!["-c".to_owned(), STAND_IN_SERVER.to_owned()];
    let server = NewMcpServer::new_stdio("tavily", "sh", args, None);
    let server = state.mcp.add_server(server).await.unwrap().id;
    state.remote.gateway().open_for_test(&["tavily"]);
    let model = turn_fixture::model(&state, |_| {}).await;
    Opened {
        _dir: dir,
        state,
        loads,
        server,
        model,
    }
}

impl Opened {
    async fn running(&self) -> bool {
        self.state.mcp.get_server_status(self.server).await == McpServerStatus::Running
    }

    /// The stand-in's one tool, as the agent loop names it.
    fn echo(&self) -> String {
        format!("{}:echo", self.server)
    }

    /// A chat this machine keeps, on its model, with `settings` besides.
    async fn chat(&self, settings: ConversationSettings) -> i64 {
        let settings = ConversationSettings {
            model: Some(ModelRef {
                machine: Machine::Local,
                id: self.model,
            }),
            ..settings
        };
        let conversation = NewConversation {
            title: "t".to_owned(),
            model_id: None,
            system_prompt: None,
            settings: Some(settings),
        };
        let history = self.state.core.chat_history();
        history.create_conversation(conversation).await.unwrap()
    }

    /// Let the run's model load, and wait for the run to end where a test's
    /// must: at the port check, past its late step's tools.
    async fn loaded_and_ended(&self, device: &str, id: &str) {
        self.loads.loaded.notify_one();
        settled(&self.state).await;
        let scope = RunScope::Device(device.to_owned());
        let run = self.state.runs.existing(&scope, id).unwrap().unwrap();
        assert_eq!(run.status, RunStatus::Failed);
        let message = run.error.unwrap().message;
        assert!(message.contains("No running server found"), "{message}");
    }
}

fn drawn(conversation_id: i64) -> HubTurn {
    HubTurn {
        draw: true,
        ..hi(conversation_id)
    }
}

/// A turn on a Mac-kept chat is planned with the floor and its `PUT`
/// starts nothing; the opened server is started in the run's late step,
/// once its model is ready, and its tool offered by the server's id.
#[tokio::test]
async fn a_turn_starts_the_opened_server_in_its_late_step_and_not_in_its_put() {
    let o = opened().await;
    let id = o.chat(ConversationSettings::default()).await;

    let plan = hub_turn::plan(&o.state, hi(id)).await.unwrap();
    assert_eq!(plan.chat.tool_filter, Some(Vec::new()), "the floor");
    let created = hub_turn::start(&o.state, "phone", "d1", hi(id)).await;
    assert!(created.unwrap().created);
    assert!(!o.running().await, "the PUT started nothing");

    o.loaded_and_ended("phone", "d1").await;
    assert!(o.running().await, "the late step started it");
    assert_eq!(resolve(&o.state, &plan.asked).await, [o.echo()]);
    o.state.mcp.shutdown().await;
}

/// A Mac-kept chat with its tools off is offered the floor alone, and its
/// run starts no server: nothing, or with Draw exactly the image tool.
#[tokio::test]
async fn a_mac_chat_with_its_tools_off_gets_only_the_floor_and_starts_nothing() {
    let o = opened().await;
    let off = ConversationSettings {
        no_tools: Some(true),
        ..ConversationSettings::default()
    };
    let id = o.chat(off).await;

    let plan = hub_turn::plan(&o.state, hi(id)).await.unwrap();
    hub_turn::start(&o.state, "phone", "d1", hi(id))
        .await
        .unwrap();
    o.loaded_and_ended("phone", "d1").await;
    assert!(!o.running().await, "a chat with its tools off starts none");
    assert_eq!(resolve(&o.state, &plan.asked).await, Vec::<String>::new());

    let plan = hub_turn::plan(&o.state, drawn(id)).await.unwrap();
    assert_eq!(
        plan.chat.tool_filter.as_deref(),
        Some([DRAW.to_owned()].as_slice())
    );
    assert_eq!(resolve(&o.state, &plan.asked).await, [DRAW]);
    assert!(!o.running().await);
    o.state.mcp.shutdown().await;
}

/// A Mac-kept chat's own list narrows the opened tools: by the tool's bare
/// name or its whole qualified one, and a name no opened server has, or the
/// tool under another server's id, gets none of them.
#[tokio::test]
async fn a_mac_chats_list_narrows_the_opened_tools_and_never_adds_one() {
    let o = opened().await;
    let elsewhere = format!("{}:echo", o.server + 1);
    let cases = [
        (vec!["echo".to_owned()], vec![o.echo()]),
        (vec![o.echo()], vec![o.echo()]),
        (vec!["nope".to_owned()], Vec::new()),
        (vec![elsewhere], Vec::new()),
    ];

    for (tools, offered) in cases {
        let named = ConversationSettings {
            tools: tools.clone(),
            ..ConversationSettings::default()
        };
        let plan = hub_turn::plan(&o.state, hi(o.chat(named).await))
            .await
            .unwrap();
        assert_eq!(plan.chat.tool_filter, Some(Vec::new()), "{tools:?}");
        assert_eq!(resolve(&o.state, &plan.asked).await, offered, "{tools:?}");
    }
    o.state.mcp.shutdown().await;
}

/// A chat the phone keeps, run with builtins, starts the opened server in
/// its late step and not in its `PUT`, and is offered every tool of it.
#[tokio::test]
async fn a_phone_kept_chat_run_starts_the_opened_server_in_its_late_step() {
    let o = opened().await;
    let body = json!({
        "model": "catalogue-name",
        "messages": [{ "role": "user", "content": "what is new?" }],
    });

    let created = chat_turn::start(&o.state, "phone", "p1", body, false.into()).await;
    assert!(created.unwrap().created);
    assert!(!o.running().await, "the PUT started nothing");

    o.loaded_and_ended("phone", "p1").await;
    assert!(o.running().await, "the late step started it");
    let phone = Asked::of_phone(false);
    assert_eq!(resolve(&o.state, &phone).await, [o.echo()]);
    o.state.mcp.shutdown().await;
}
