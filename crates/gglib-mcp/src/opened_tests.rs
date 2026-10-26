//! What paired devices may use: the flag on each server, set by its own
//! write, and the tools a device's run is offered from the servers open.

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::ports::{McpRepositoryError, McpServerRepository};
use gglib_core::{DeviceServers, McpServer, McpServiceError, NewMcpServer};

use crate::service::McpService;
use crate::service::tests::{service, stdio};

/// A repository that cannot be read.
struct Unreadable;

#[async_trait]
impl McpServerRepository for Unreadable {
    async fn insert(&self, _: NewMcpServer) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::Internal("down".to_owned()))
    }
    async fn get_by_id(&self, _: i64) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::Internal("down".to_owned()))
    }
    async fn get_by_name(&self, _: &str) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::Internal("down".to_owned()))
    }
    async fn list(&self) -> Result<Vec<McpServer>, McpRepositoryError> {
        Err(McpRepositoryError::Internal("down".to_owned()))
    }
    async fn update(&self, _: &McpServer) -> Result<(), McpRepositoryError> {
        Err(McpRepositoryError::Internal("down".to_owned()))
    }
    async fn delete(&self, _: i64) -> Result<(), McpRepositoryError> {
        Err(McpRepositoryError::Internal("down".to_owned()))
    }
    async fn update_last_connected(&self, _: i64) -> Result<(), McpRepositoryError> {
        Err(McpRepositoryError::Internal("down".to_owned()))
    }
    async fn set_allow_remote(&self, _: i64, _: bool) -> Result<(), McpRepositoryError> {
        Err(McpRepositoryError::Internal("down".to_owned()))
    }
}

/// The stand-in MCP server under `name`, as `gglib mcp add` stores one: a
/// bare command, lazy, valid.
#[cfg(unix)]
fn stand_in(name: &str) -> gglib_core::NewMcpServer {
    gglib_core::NewMcpServer::new_stdio(
        name,
        "sh",
        vec![
            "-c".to_owned(),
            crate::service::tests::STAND_IN_SERVER.to_owned(),
        ],
        None,
    )
}

#[cfg(unix)]
async fn running(service: &crate::McpService, id: i64) -> bool {
    service.get_server_status(id).await == gglib_core::McpServerStatus::Running
}

/// `new` added and opened to paired devices, as `gglib mcp add` then
/// `allow-remote` leave it.
#[cfg(unix)]
async fn opened(service: &McpService, new: NewMcpServer) -> McpServer {
    let added = service.add_server(new).await.unwrap();
    service.set_allow_remote(added.id, true).await.unwrap()
}

/// An open lazy server that is not running is started, and its tools are
/// offered by the id the agent loop calls them by, never by its name.
#[cfg(unix)]
#[tokio::test]
async fn an_open_lazy_server_is_started_and_its_tools_named_by_its_id() {
    let (service, _) = service();
    service.add_server(stdio("fs", "npx")).await.unwrap();
    let tavily = opened(&service, stand_in("tavily")).await;
    assert!(!running(&service, tavily.id).await);

    let offered = service.opened_tools(&DeviceServers::All).await;

    assert_eq!(offered, [format!("{}:echo", tavily.id)]);
    assert!(running(&service, tavily.id).await, "started for the run");
    service.shutdown().await;
}

/// A server running for some other reason is not offered unless it is open
/// to paired devices.
#[cfg(unix)]
#[tokio::test]
async fn a_running_server_not_open_is_not_offered() {
    let (service, _) = service();
    let tavily = opened(&service, stand_in("tavily")).await;
    let shell = service.add_server(stand_in("shell")).await.unwrap();
    service.start_server(shell.id).await.unwrap();

    let offered = service.opened_tools(&DeviceServers::All).await;

    assert_eq!(offered, [format!("{}:echo", tavily.id)]);
    service.shutdown().await;
}

/// A disabled server and a manual one are neither started nor offered,
/// though both are open and would start; the one opened after them still is.
#[cfg(unix)]
#[tokio::test]
async fn a_disabled_and_a_manual_server_are_skipped() {
    let (service, _) = service();
    let mut off = stand_in("off");
    off.enabled = false;
    let off = opened(&service, off).await;
    let manual = stand_in("manual").with_lifecycle(gglib_core::McpLifecycle::Manual);
    let manual = opened(&service, manual).await;
    let tavily = opened(&service, stand_in("tavily")).await;

    let offered = service.opened_tools(&DeviceServers::All).await;

    assert_eq!(offered, [format!("{}:echo", tavily.id)]);
    assert!(
        !running(&service, off.id).await,
        "a disabled server stays stopped"
    );
    assert!(!running(&service, manual.id).await, "a manual one too");
    service.shutdown().await;
}

/// A server whose command cannot be found, and one that exits before its
/// handshake, are skipped, and the servers opened after them still offered.
#[cfg(unix)]
#[tokio::test]
async fn a_server_that_fails_to_start_is_skipped_and_the_rest_offered() {
    let (service, _) = service();
    opened(&service, stdio("ghost", "gglib-no-such-command")).await;
    let exits = gglib_core::NewMcpServer::new_stdio(
        "exits",
        "sh",
        vec!["-c".to_owned(), "exit 1".to_owned()],
        None,
    );
    opened(&service, exits).await;
    let tavily = opened(&service, stand_in("tavily")).await;

    let offered = service.opened_tools(&DeviceServers::All).await;

    assert_eq!(offered, [format!("{}:echo", tavily.id)]);
    service.shutdown().await;
}

/// A server stamped invalid when it was added, its working folder missing
/// then, is skipped as that verdict says, though the folder exists now and
/// it would start; the server opened after it is still offered.
#[cfg(unix)]
#[tokio::test]
async fn a_server_stamped_invalid_is_skipped_though_it_would_start_now() {
    let (service, _) = service();
    let dir = tempfile::tempdir().unwrap();
    let later = dir.path().join("later");
    let mut stale = stand_in("stale");
    stale.config.working_dir = Some(later.to_string_lossy().into_owned());
    let stale = opened(&service, stale).await;
    assert!(!stale.is_valid, "stamped invalid when added");
    std::fs::create_dir(&later).unwrap();
    let tavily = opened(&service, stand_in("tavily")).await;

    let offered = service.opened_tools(&DeviceServers::All).await;

    assert_eq!(offered, [format!("{}:echo", tavily.id)]);
    assert!(
        !running(&service, stale.id).await,
        "an invalid server stays stopped"
    );
    service.shutdown().await;
}

/// Two servers sharing a name are each opened by their own flag: both are
/// started and both offered, each by its own id.
#[cfg(unix)]
#[tokio::test]
async fn two_servers_of_one_name_are_each_offered_when_open() {
    let (service, repo) = service();
    // An older database can hold two of one name; the service refuses it now.
    let first = repo.insert(stand_in("tavily")).await.unwrap();
    let second = repo.insert(stand_in("tavily")).await.unwrap();
    service.initialize().await.unwrap();
    for id in [first.id, second.id] {
        service.set_allow_remote(id, true).await.unwrap();
    }

    let mut offered = service.opened_tools(&DeviceServers::All).await;
    offered.sort();

    let mut both = vec![format!("{}:echo", first.id), format!("{}:echo", second.id)];
    both.sort();
    assert_eq!(offered, both);
    assert!(running(&service, first.id).await && running(&service, second.id).await);
    service.shutdown().await;
}

/// Of the open servers, only those a device's run chose are started and
/// offered; another open one stays stopped.
#[cfg(unix)]
#[tokio::test]
async fn only_the_chosen_open_servers_are_started_and_offered() {
    let (service, _) = service();
    let tavily = opened(&service, stand_in("tavily")).await;
    let shell = opened(&service, stand_in("shell")).await;

    let offered = service
        .opened_tools(&DeviceServers::Only([tavily.id].into()))
        .await;

    assert_eq!(offered, [format!("{}:echo", tavily.id)]);
    assert!(running(&service, tavily.id).await);
    assert!(
        !running(&service, shell.id).await,
        "not chosen, not started"
    );
    service.shutdown().await;
}

/// An id a device's run chose that is not open, a server here but closed or
/// no server at all, is never started or offered; the open one chosen
/// beside it still is.
#[cfg(unix)]
#[tokio::test]
async fn a_chosen_id_that_is_not_open_is_never_started_or_offered() {
    let (service, _) = service();
    let tavily = opened(&service, stand_in("tavily")).await;
    let shell = service.add_server(stand_in("shell")).await.unwrap();
    let nowhere = shell.id + 100;

    let chosen = DeviceServers::Only([tavily.id, shell.id, nowhere].into());
    let offered = service.opened_tools(&chosen).await;

    assert_eq!(offered, [format!("{}:echo", tavily.id)]);
    assert!(!running(&service, shell.id).await, "closed, never started");
    service.shutdown().await;
}

/// A device's run that chose no server starts none and is offered nothing,
/// though one is open.
#[cfg(unix)]
#[tokio::test]
async fn choosing_none_starts_nothing() {
    let (service, _) = service();
    let tavily = opened(&service, stand_in("tavily")).await;

    let offered = service.opened_tools(&DeviceServers::none()).await;

    assert_eq!(offered, Vec::<String>::new());
    assert!(!running(&service, tavily.id).await);
    service.shutdown().await;
}

/// Two open servers sharing a name are chosen apart by their ids: the one
/// named is started and offered, the other is not.
#[cfg(unix)]
#[tokio::test]
async fn two_servers_sharing_a_name_are_chosen_apart_by_id() {
    let (service, repo) = service();
    // An older database can hold two of one name; the service refuses it now.
    let first = repo.insert(stand_in("tavily")).await.unwrap();
    let second = repo.insert(stand_in("tavily")).await.unwrap();
    service.initialize().await.unwrap();
    for id in [first.id, second.id] {
        service.set_allow_remote(id, true).await.unwrap();
    }

    let offered = service
        .opened_tools(&DeviceServers::Only([second.id].into()))
        .await;

    assert_eq!(offered, [format!("{}:echo", second.id)]);
    assert!(running(&service, second.id).await);
    assert!(!running(&service, first.id).await, "the other of the name");
    service.shutdown().await;
}

/// What a device's run is offered is exactly what the agent loop's MCP
/// executor lists for that server: one naming rule.
#[cfg(unix)]
#[tokio::test]
async fn qualified_is_what_the_executor_lists() {
    use gglib_core::ports::ToolExecutorPort as _;

    let (service, _) = service();
    service.add_server(stdio("fs", "npx")).await.unwrap();
    opened(&service, stand_in("tavily")).await;
    let service = std::sync::Arc::new(service);

    let offered = service.opened_tools(&DeviceServers::All).await;
    let (images, _) = crate::tool_images::tool_images_tests::images();
    let executor =
        crate::tool_executor::McpToolExecutorAdapter::new(std::sync::Arc::clone(&service), images);
    let listed: Vec<String> = executor
        .list_tools()
        .await
        .into_iter()
        .map(|tool| tool.name)
        .collect();

    assert!(!offered.is_empty());
    assert_eq!(offered, listed);
    service.shutdown().await;
}

/// Whether `id` is open to paired devices, as stored.
async fn open(service: &McpService, id: i64) -> bool {
    service.get_server(id).await.unwrap().allow_remote
}

/// A server running when it is opened, and when it is closed again, is still
/// running after each: the flag's write does not go through an update,
/// which stops a running server.
#[cfg(unix)]
#[tokio::test]
async fn a_running_server_keeps_running_when_opened_and_closed() {
    let (service, _) = service();
    let tavily = service.add_server(stand_in("tavily")).await.unwrap();
    service.start_server(tavily.id).await.unwrap();

    let opened = service.set_allow_remote(tavily.id, true).await.unwrap();
    assert!(opened.allow_remote, "the server comes back open");
    assert!(open(&service, tavily.id).await, "and is stored open");
    assert!(
        running(&service, tavily.id).await,
        "still running once open"
    );

    service.set_allow_remote(tavily.id, false).await.unwrap();
    assert!(!open(&service, tavily.id).await);
    assert!(
        running(&service, tavily.id).await,
        "still running once closed"
    );
    service.shutdown().await;
}

/// An SSE server and one stored as `builtin` are refused when opened, each
/// with its own reason, and stay closed; closing either is never refused.
#[tokio::test]
async fn an_sse_server_and_builtin_are_refused_opening_but_not_closing() {
    let (service, repo) = service();
    // Only the repository can hold these: the service refuses both on add.
    let sse = repo
        .insert(NewMcpServer::new_sse("remote", "http://localhost:3001/sse"))
        .await
        .unwrap();
    let builtin = repo.insert(stdio("builtin", "npx")).await.unwrap();

    let refused = service.set_allow_remote(sse.id, true).await;
    assert!(
        matches!(refused, Err(McpServiceError::SseNotSupported)),
        "{refused:?}"
    );
    let refused = service.set_allow_remote(builtin.id, true).await;
    let Err(why) = refused else {
        panic!("builtin was opened: {refused:?}")
    };
    assert!(matches!(why, McpServiceError::NeverOpened(_)), "{why:?}");
    assert_eq!(
        why.to_string(),
        "'builtin' is gglib's own tools and is never opened to devices"
    );
    assert!(!open(&service, sse.id).await && !open(&service, builtin.id).await);

    // A database written before either was refused can hold them open.
    repo.set_allow_remote(sse.id, true).await.unwrap();
    repo.set_allow_remote(builtin.id, true).await.unwrap();
    for id in [sse.id, builtin.id] {
        let closed = service.set_allow_remote(id, false).await.unwrap();
        assert!(!closed.allow_remote && !open(&service, id).await);
    }
}

#[tokio::test]
async fn an_unknown_server_is_not_found() {
    let (service, _) = service();

    let refused = service.set_allow_remote(42, true).await;

    assert!(
        matches!(
            refused,
            Err(McpServiceError::Repository(McpRepositoryError::NotFound(_)))
        ),
        "{refused:?}"
    );
}

/// Only a server flagged open, enabled, stdio and not `builtin` is open to
/// devices; an unflagged one is not, whatever else it is.
#[tokio::test]
async fn only_a_flagged_enabled_stdio_server_is_open_to_devices() {
    let (service, repo) = service();
    let tavily = repo.insert(stdio("tavily", "npx")).await.unwrap();
    repo.insert(stdio("fs", "npx")).await.unwrap();
    let mut off = stdio("off", "npx");
    off.enabled = false;
    let off = repo.insert(off).await.unwrap();
    let sse = repo
        .insert(NewMcpServer::new_sse("remote", "http://localhost:3001/sse"))
        .await
        .unwrap();
    let builtin = repo.insert(stdio("builtin", "npx")).await.unwrap();
    for id in [tavily.id, off.id, sse.id, builtin.id] {
        repo.set_allow_remote(id, true).await.unwrap();
    }

    let opened: Vec<i64> = service
        .open_to_devices()
        .await
        .unwrap()
        .iter()
        .map(|server| server.id)
        .collect();

    assert_eq!(opened, [tavily.id]);
    assert_eq!(service.device_server_names().await, ["tavily"]);
}

/// Two servers sharing a name can each be open, and the name is listed once.
#[tokio::test]
async fn a_name_two_open_servers_share_is_listed_once() {
    let (service, repo) = service();
    // An older database can hold two of one name; the service refuses it now.
    let first = repo.insert(stdio("tavily", "npx")).await.unwrap();
    let fs = repo.insert(stdio("fs", "npx")).await.unwrap();
    let second = repo.insert(stdio("tavily", "uvx")).await.unwrap();
    for id in [first.id, fs.id, second.id] {
        service.set_allow_remote(id, true).await.unwrap();
    }

    assert_eq!(service.open_to_devices().await.unwrap().len(), 3);
    assert_eq!(service.device_server_names().await, ["fs", "tavily"]);
}

/// The servers a device chooses from are each open server, by name and then
/// by id, a name two share listed under each: the order is not the order
/// they were added in.
#[tokio::test]
async fn device_servers_lists_each_open_server_by_id_in_name_then_id_order() {
    let (service, repo) = service();
    // An older database can hold two of one name; the service refuses it now.
    let first = repo.insert(stdio("tavily", "npx")).await.unwrap();
    let fs = repo.insert(stdio("fs", "npx")).await.unwrap();
    let closed = repo.insert(stdio("closed", "npx")).await.unwrap();
    let second = repo.insert(stdio("tavily", "uvx")).await.unwrap();
    for id in [second.id, first.id, fs.id] {
        service.set_allow_remote(id, true).await.unwrap();
    }

    let listed: Vec<(i64, String)> = service
        .device_servers()
        .await
        .into_iter()
        .map(|server| (server.id, server.name))
        .collect();

    let tavily = "tavily".to_owned();
    assert_eq!(
        listed,
        [
            (fs.id, "fs".to_owned()),
            (first.id, tavily.clone()),
            (second.id, tavily)
        ]
    );
    assert!(!listed.iter().any(|(id, _)| *id == closed.id));
}

/// Servers that cannot be read open none: the read says so, and the servers
/// a device chooses from, the names a status shows and the tools a run is
/// offered are none.
#[tokio::test]
async fn unreadable_servers_open_none_to_devices() {
    let service = McpService::new(Arc::new(Unreadable));

    assert!(service.open_to_devices().await.is_err());
    assert!(service.device_servers().await.is_empty());
    assert_eq!(service.device_server_names().await, Vec::<String>::new());
    assert_eq!(
        service.opened_tools(&DeviceServers::All).await,
        Vec::<String>::new()
    );
}
