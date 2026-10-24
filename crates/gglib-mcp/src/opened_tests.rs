//! Which names `--allow-mcp` may be given: exactly one configured server
//! each, and never gglib's own tools.

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::ports::{McpRepositoryError, McpServerRepository};
use gglib_core::{McpServer, McpServiceError, NewMcpServer};

use super::Unopenable;
use crate::service::McpService;
use crate::service::tests::{service, stdio};

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

/// What a refusal said of a name, or `""` when it was not about a name.
fn said(refused: &Result<(), Unopenable>) -> &str {
    match refused {
        Err(Unopenable::Name(why)) => why,
        _ => "",
    }
}

#[tokio::test]
async fn a_name_that_one_server_has_is_accepted() {
    let (service, repo) = service();
    repo.insert(stdio("tavily", "npx")).await.unwrap();
    repo.insert(stdio("fs", "npx")).await.unwrap();

    let both = service.refuse_unopenable(&names(&["tavily", "fs"])).await;
    assert!(both.is_ok(), "{both:?}");
    let none = service.refuse_unopenable(&[]).await;
    assert!(none.is_ok(), "naming none opens none: {none:?}");
}

#[tokio::test]
async fn builtin_is_refused_even_when_a_server_was_stored_under_it() {
    let (service, repo) = service();
    // Only the repository can hold this: the service refuses the name on add.
    repo.insert(stdio("builtin", "npx")).await.unwrap();

    let refused = service.refuse_unopenable(&names(&["builtin"])).await;
    assert!(said(&refused).contains("gglib's own tools"), "{refused:?}");
}

#[tokio::test]
async fn a_name_no_server_has_is_refused() {
    let (service, repo) = service();
    repo.insert(stdio("tavily", "npx")).await.unwrap();

    let refused = service
        .refuse_unopenable(&names(&["tavily", "tavilly"]))
        .await;
    assert!(said(&refused).contains("'tavilly'"), "{refused:?}");
}

#[tokio::test]
async fn a_name_two_servers_share_is_refused() {
    let (service, repo) = service();
    // An older database can hold two of one name; the service refuses it now.
    repo.insert(stdio("tavily", "npx")).await.unwrap();
    repo.insert(stdio("tavily", "uvx")).await.unwrap();

    let refused = service.refuse_unopenable(&names(&["tavily"])).await;
    assert!(said(&refused).contains("2 MCP servers"), "{refused:?}");
}

#[tokio::test]
async fn a_blank_name_is_refused() {
    let (service, repo) = service();
    repo.insert(stdio("tavily", "npx")).await.unwrap();

    for blank in ["", "  "] {
        let refused = service.refuse_unopenable(&names(&["tavily", blank])).await;
        assert!(said(&refused).contains("blank"), "{blank:?}: {refused:?}");
    }
}

/// A name is the server's whole name, as typed: not another case of it,
/// and not the start of a longer one.
#[tokio::test]
async fn a_name_is_matched_whole_and_by_case() {
    let (cased, repo) = service();
    repo.insert(stdio("tavily", "npx")).await.unwrap();
    let refused = cased.refuse_unopenable(&names(&["Tavily"])).await;
    assert!(said(&refused).contains("'Tavily'"), "{refused:?}");

    let (longer, repo) = service();
    repo.insert(stdio("tavily-dev", "npx")).await.unwrap();
    let refused = longer.refuse_unopenable(&names(&["tavily"])).await;
    assert!(said(&refused).contains("'tavily'"), "{refused:?}");
}

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

/// Servers that cannot be read are this machine's failure, not a bad name,
/// and a list naming none never reads them.
#[tokio::test]
async fn unreadable_servers_are_not_a_bad_name_and_naming_none_reads_nothing() {
    let service = McpService::new(Arc::new(Unreadable));

    let refused = service.refuse_unopenable(&names(&["tavily"])).await;
    assert!(matches!(refused, Err(Unopenable::Unread(_))), "{refused:?}");
    let none = service.refuse_unopenable(&[]).await;
    assert!(none.is_ok(), "a plain enable is not refused: {none:?}");
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

/// A named lazy server that is not running is started, and its tools are
/// offered by the id the agent loop calls them by, never by its name.
#[cfg(unix)]
#[tokio::test]
async fn a_named_lazy_server_is_started_and_its_tools_named_by_its_id() {
    let (service, _) = service();
    service.add_server(stdio("fs", "npx")).await.unwrap();
    let tavily = service.add_server(stand_in("tavily")).await.unwrap();
    assert!(!running(&service, tavily.id).await);

    let offered = service.opened_tools(&names(&["tavily"])).await;

    assert_eq!(offered, [format!("{}:echo", tavily.id)]);
    assert!(running(&service, tavily.id).await, "started for the run");
    service.shutdown().await;
}

/// A server running for some other reason is not offered unless named.
#[cfg(unix)]
#[tokio::test]
async fn a_running_server_not_named_is_not_offered() {
    let (service, _) = service();
    let tavily = service.add_server(stand_in("tavily")).await.unwrap();
    let shell = service.add_server(stand_in("shell")).await.unwrap();
    service.start_server(shell.id).await.unwrap();

    let offered = service.opened_tools(&names(&["tavily"])).await;

    assert_eq!(offered, [format!("{}:echo", tavily.id)]);
    service.shutdown().await;
}

/// A disabled server and a manual one are neither started nor offered,
/// though both would start, and a name no server has offers nothing; the
/// one named after them still is.
#[cfg(unix)]
#[tokio::test]
async fn a_disabled_a_manual_and_an_unknown_server_are_skipped() {
    let (service, _) = service();
    let mut off = stand_in("off");
    off.enabled = false;
    let off = service.add_server(off).await.unwrap();
    let manual = stand_in("manual").with_lifecycle(gglib_core::McpLifecycle::Manual);
    let manual = service.add_server(manual).await.unwrap();
    let tavily = service.add_server(stand_in("tavily")).await.unwrap();

    let offered = service
        .opened_tools(&names(&["off", "manual", "nope", "tavily"]))
        .await;

    assert_eq!(offered, [format!("{}:echo", tavily.id)]);
    assert!(
        !running(&service, off.id).await,
        "a disabled server stays stopped"
    );
    assert!(!running(&service, manual.id).await, "a manual one too");
    service.shutdown().await;
}

/// A server whose command cannot be found, and one that exits before its
/// handshake, are skipped, and the servers named after them still offered.
#[cfg(unix)]
#[tokio::test]
async fn a_server_that_fails_to_start_is_skipped_and_the_rest_offered() {
    let (service, _) = service();
    service
        .add_server(stdio("ghost", "gglib-no-such-command"))
        .await
        .unwrap();
    let exits = gglib_core::NewMcpServer::new_stdio(
        "exits",
        "sh",
        vec!["-c".to_owned(), "exit 1".to_owned()],
        None,
    );
    service.add_server(exits).await.unwrap();
    let tavily = service.add_server(stand_in("tavily")).await.unwrap();

    let offered = service
        .opened_tools(&names(&["ghost", "exits", "tavily"]))
        .await;

    assert_eq!(offered, [format!("{}:echo", tavily.id)]);
    service.shutdown().await;
}

/// A server stamped invalid when it was added, its working folder missing
/// then, is skipped as that verdict says, though the folder exists now and
/// it would start; the server named after it is still offered.
#[cfg(unix)]
#[tokio::test]
async fn a_server_stamped_invalid_is_skipped_though_it_would_start_now() {
    let (service, _) = service();
    let dir = tempfile::tempdir().unwrap();
    let later = dir.path().join("later");
    let mut stale = stand_in("stale");
    stale.config.working_dir = Some(later.to_string_lossy().into_owned());
    let stale = service.add_server(stale).await.unwrap();
    assert!(!stale.is_valid, "stamped invalid when added");
    std::fs::create_dir(&later).unwrap();
    let tavily = service.add_server(stand_in("tavily")).await.unwrap();

    let offered = service.opened_tools(&names(&["stale", "tavily"])).await;

    assert_eq!(offered, [format!("{}:echo", tavily.id)]);
    assert!(
        !running(&service, stale.id).await,
        "an invalid server stays stopped"
    );
    service.shutdown().await;
}

/// Two servers sharing a name open neither, and neither is started.
#[cfg(unix)]
#[tokio::test]
async fn a_shared_name_opens_neither() {
    let (service, repo) = service();
    // An older database can hold two of one name; the service refuses it now.
    let first = repo.insert(stand_in("tavily")).await.unwrap();
    let second = repo.insert(stand_in("tavily")).await.unwrap();
    service.initialize().await.unwrap();

    let offered = service.opened_tools(&names(&["tavily"])).await;

    assert_eq!(offered, Vec::<String>::new());
    assert!(!running(&service, first.id).await);
    assert!(!running(&service, second.id).await);
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
    service.add_server(stand_in("tavily")).await.unwrap();
    let service = std::sync::Arc::new(service);

    let offered = service.opened_tools(&names(&["tavily"])).await;
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

/// Servers that cannot be read open none: the read says so, and the names
/// a status shows are none.
#[tokio::test]
async fn unreadable_servers_open_none_to_devices() {
    let service = McpService::new(Arc::new(Unreadable));

    assert!(service.open_to_devices().await.is_err());
    assert_eq!(service.device_server_names().await, Vec::<String>::new());
}
