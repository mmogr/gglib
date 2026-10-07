//! `McpService` over a repository held in memory.

use super::*;
use async_trait::async_trait;
use std::sync::Mutex;

/// A repository in memory. It keeps whatever it is given, two servers of one
/// name included, as the `SQLite` one does: unique names are the service's
/// rule.
struct MockMcpRepository {
    servers: Mutex<Vec<McpServer>>,
    next_id: Mutex<i64>,
    /// How many updates were written.
    updates: Mutex<usize>,
}

impl MockMcpRepository {
    fn new() -> Self {
        Self {
            servers: Mutex::new(Vec::new()),
            next_id: Mutex::new(1),
            updates: Mutex::new(0),
        }
    }
}

#[async_trait]
impl McpServerRepository for MockMcpRepository {
    async fn insert(&self, new_server: NewMcpServer) -> Result<McpServer, McpRepositoryError> {
        let id = {
            let mut next_id = self.next_id.lock().unwrap();
            let id = *next_id;
            *next_id += 1;
            id
        };

        let server = McpServer {
            id,
            name: new_server.name,
            server_type: new_server.server_type,
            config: new_server.config,
            enabled: new_server.enabled,
            lifecycle: new_server.lifecycle,
            env: new_server.env,
            created_at: chrono::Utc::now(),
            last_connected_at: None,
            is_valid: false,
            last_error: None,
        };

        self.servers.lock().unwrap().push(server.clone());
        Ok(server)
    }

    async fn get_by_id(&self, id: i64) -> Result<McpServer, McpRepositoryError> {
        let servers = self.servers.lock().unwrap();
        servers
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or_else(|| McpRepositoryError::NotFound(id.to_string()))
    }

    /// Answers from what was stored when it was asked, a turn of the
    /// scheduler later, as a query that has to wait for the database does.
    async fn get_by_name(&self, name: &str) -> Result<McpServer, McpRepositoryError> {
        let found = {
            let servers = self.servers.lock().unwrap();
            servers.iter().find(|s| s.name == name).cloned()
        };
        tokio::task::yield_now().await;
        found.ok_or_else(|| McpRepositoryError::NotFound(name.to_string()))
    }

    async fn list(&self) -> Result<Vec<McpServer>, McpRepositoryError> {
        let servers = self.servers.lock().unwrap();
        Ok(servers.clone())
    }

    async fn update(&self, server: &McpServer) -> Result<(), McpRepositoryError> {
        *self.updates.lock().unwrap() += 1;
        let mut servers = self.servers.lock().unwrap();
        servers.iter_mut().find(|s| s.id == server.id).map_or_else(
            || Err(McpRepositoryError::NotFound(server.id.to_string())),
            |s| {
                *s = server.clone();
                Ok(())
            },
        )
    }

    async fn delete(&self, id: i64) -> Result<(), McpRepositoryError> {
        let mut servers = self.servers.lock().unwrap();
        let len_before = servers.len();
        servers.retain(|s| s.id != id);
        if servers.len() < len_before {
            Ok(())
        } else {
            Err(McpRepositoryError::NotFound(id.to_string()))
        }
    }

    async fn update_last_connected(&self, id: i64) -> Result<(), McpRepositoryError> {
        let mut servers = self.servers.lock().unwrap();
        if let Some(s) = servers.iter_mut().find(|s| s.id == id) {
            s.last_connected_at = Some(chrono::Utc::now());
            Ok(())
        } else {
            Err(McpRepositoryError::NotFound(id.to_string()))
        }
    }
}

/// A service and the repository under it, for a test that has to store what
/// the service would refuse.
fn service() -> (McpService, Arc<MockMcpRepository>) {
    let repo = Arc::new(MockMcpRepository::new());
    (McpService::new(repo.clone()), repo)
}

fn stdio(name: &str, command: &str) -> NewMcpServer {
    NewMcpServer::new_stdio(name, command, vec![], None)
}

/// An MCP server for `sh -c`: it answers `initialize` and `tools/list` with
/// the id it was asked under, and offers one tool.
#[cfg(unix)]
const STAND_IN_SERVER: &str = r#"while IFS= read -r line; do
  id=${line#*\"id\":}
  id=${id%%,*}
  case "$line" in
    *'"method":"initialize"'*)
      printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{\"tools\":{}},\"serverInfo\":{\"name\":\"stand-in\"}}}" ;;
    *'"method":"tools/list"'*)
      printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"tools\":[{\"name\":\"echo\",\"description\":\"says it back\"}]}}" ;;
  esac
done"#;

#[tokio::test]
async fn test_add_and_list_servers() {
    let (service, _) = service();

    let new_server = NewMcpServer::new_stdio("Test", "echo", vec!["hello".to_string()], None);
    let saved = service.add_server(new_server).await.unwrap();
    assert!(saved.id > 0);

    let servers = service.list_servers().await.unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].name, "Test");
}

#[tokio::test]
async fn test_remove_server() {
    let (service, _) = service();

    let saved = service.add_server(stdio("Test", "echo")).await.unwrap();

    service.remove_server(saved.id).await.unwrap();

    let result = service.get_server(saved.id).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_server_status_when_not_running() {
    let (service, _) = service();

    let saved = service.add_server(stdio("Test", "echo")).await.unwrap();

    let status = service.get_server_status(saved.id).await;
    assert_eq!(status, McpServerStatus::Stopped);
}

#[tokio::test]
async fn adding_a_server_under_a_taken_name_is_refused_and_stores_nothing() {
    let (service, _) = service();
    service.add_server(stdio("files", "one")).await.unwrap();

    let refused = service.add_server(stdio("files", "two")).await.unwrap_err();

    assert!(matches!(&refused, McpServiceError::NameTaken(name) if name == "files"));
    assert_eq!(
        refused.to_string(),
        "An MCP server named 'files' already exists; choose another name"
    );
    let servers = service.list_servers().await.unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].config.command, Some("one".to_string()));
}

#[tokio::test]
async fn two_adds_of_one_name_at_once_store_one_server() {
    let (service, _) = service();

    let (first, second) = tokio::join!(
        service.add_server(stdio("files", "one")),
        service.add_server(stdio("files", "two")),
    );

    assert!(first.is_ok());
    assert!(matches!(second, Err(McpServiceError::NameTaken(_))));
    assert_eq!(service.list_servers().await.unwrap().len(), 1);
}

#[tokio::test]
async fn renaming_a_server_to_a_taken_name_is_refused_and_changes_nothing() {
    let (service, _) = service();
    service.add_server(stdio("files", "one")).await.unwrap();
    let mut other = service.add_server(stdio("search", "two")).await.unwrap();

    other.name = "files".to_string();
    other.enabled = false;
    let refused = service.update_server(other.clone()).await.unwrap_err();

    assert!(matches!(&refused, McpServiceError::NameTaken(name) if name == "files"));
    let stored = service.get_server(other.id).await.unwrap();
    assert_eq!(stored.name, "search");
    assert!(stored.enabled, "a refused rename writes none of the update");
}

#[tokio::test]
async fn a_server_is_renamed_to_a_free_name_and_edited_under_its_own() {
    let (service, _) = service();
    let mut server = service.add_server(stdio("files", "one")).await.unwrap();

    server.enabled = false;
    service.update_server(server.clone()).await.unwrap();
    server.name = "documents".to_string();
    service.update_server(server.clone()).await.unwrap();

    let stored = service.get_server(server.id).await.unwrap();
    assert_eq!(stored.name, "documents");
    assert!(!stored.enabled);
}

/// A database written before names were refused can hold two of one name.
/// Each must still be editable, and renaming one of them is the way out.
#[tokio::test]
async fn two_servers_already_sharing_a_name_are_each_still_edited_and_renamed() {
    let (service, repo) = service();
    let mut first = repo.insert(stdio("twin", "one")).await.unwrap();
    let mut second = repo.insert(stdio("twin", "two")).await.unwrap();

    service.initialize().await.unwrap();
    assert_eq!(service.list_servers().await.unwrap().len(), 2);

    first.enabled = false;
    service.update_server(first.clone()).await.unwrap();
    assert!(!service.get_server(first.id).await.unwrap().enabled);

    second.name = "single".to_string();
    service.update_server(second.clone()).await.unwrap();
    assert_eq!(service.get_server(second.id).await.unwrap().name, "single");
}

#[tokio::test]
async fn updating_a_server_that_is_not_there_is_not_found() {
    let (service, _) = service();
    let mut server = service.add_server(stdio("files", "one")).await.unwrap();

    server.id += 100;
    let result = service.update_server(server).await;

    assert!(matches!(
        result,
        Err(McpServiceError::Repository(McpRepositoryError::NotFound(_)))
    ));
}

/// The row is added with a bare command and no resolved path, as
/// `gglib mcp add` leaves it. The test has to resolve it before it starts
/// it: the client refuses a path that is not absolute.
#[cfg(unix)]
#[tokio::test]
async fn testing_a_server_resolves_its_executable_then_starts_what_is_stored() {
    let (service, _) = service();
    let added = service
        .add_server(NewMcpServer::new_stdio(
            "stand-in",
            "sh",
            vec!["-c".to_string(), STAND_IN_SERVER.to_string()],
            None,
        ))
        .await
        .unwrap();
    assert_eq!(added.config.resolved_path_cache, None);

    let tools = service.test_server(added.id).await.unwrap();

    let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_str()).collect();
    assert_eq!(names, ["echo"]);
    let resolved = service
        .get_server(added.id)
        .await
        .unwrap()
        .config
        .resolved_path_cache
        .expect("the test stores the path it resolved");
    assert!(
        std::path::Path::new(&resolved).is_absolute() && resolved.ends_with("/sh"),
        "resolved to {resolved}"
    );
    assert_eq!(
        service.get_server_status(added.id).await,
        McpServerStatus::Stopped,
        "the test runs a throwaway instance, not the server itself"
    );
}

#[tokio::test]
async fn testing_a_server_whose_command_cannot_be_resolved_reports_the_failed_start() {
    let (service, _) = service();
    let added = service
        .add_server(stdio("ghost", "gglib-no-such-command"))
        .await
        .unwrap();

    let failed = service.test_server(added.id).await.unwrap_err();

    assert!(
        matches!(&failed, McpServiceError::StartFailed(why)
            if why.contains("Executable path must be absolute: gglib-no-such-command")),
        "got {failed}"
    );
    let stored = service.get_server(added.id).await.unwrap();
    assert!(
        !stored.is_valid,
        "the resolution ran, and recorded its failure"
    );
    assert!(stored.last_error.is_some());
}

#[tokio::test]
async fn testing_a_server_that_is_not_there_is_not_found() {
    let (service, _) = service();

    let result = service.test_server(42).await;

    assert!(matches!(
        result,
        Err(McpServiceError::Repository(McpRepositoryError::NotFound(_)))
    ));
}

/// What a command with a space in it is refused with.
const SPACED_COMMAND: &str = "Command must be an executable name/path only (e.g., 'npx'). \
                              Put flags and arguments in the 'args' field.";

async fn stored_verdict(service: &McpService, id: i64) -> (bool, Option<String>) {
    let stored = service.get_server(id).await.unwrap();
    (stored.is_valid, stored.last_error)
}

#[tokio::test]
async fn a_server_is_stamped_valid_or_not_when_it_is_added_and_when_it_is_updated() {
    let (service, _) = service();

    let mut server = service.add_server(stdio("good", "echo")).await.unwrap();
    assert_eq!((server.is_valid, server.last_error.clone()), (true, None));
    assert_eq!(stored_verdict(&service, server.id).await, (true, None));

    let spaced = service
        .add_server(stdio("spaced", "echo hello"))
        .await
        .unwrap();
    let refused = (false, Some(SPACED_COMMAND.to_string()));
    assert_eq!((spaced.is_valid, spaced.last_error.clone()), refused);
    assert_eq!(stored_verdict(&service, spaced.id).await, refused);

    server.config.command = Some("echo hello".to_string());
    service.update_server(server.clone()).await.unwrap();
    assert_eq!(stored_verdict(&service, server.id).await, refused);

    // The verdict is the service's: what the caller's copy claims is not kept.
    server.config.command = Some("echo".to_string());
    server.is_valid = false;
    server.last_error = Some("stale".to_string());
    service.update_server(server.clone()).await.unwrap();
    assert_eq!(stored_verdict(&service, server.id).await, (true, None));
}

/// Both rows are stored by hand, as the repository hands them back: not yet
/// valid, with no error. Neither is eager, so nothing is started.
#[tokio::test]
async fn starting_up_restamps_each_server_whose_verdict_changed_and_writes_no_other() {
    let (service, repo) = service();
    let fresh = repo.insert(stdio("fresh", "echo")).await.unwrap();
    let spaced = repo.insert(stdio("spaced", "echo hello")).await.unwrap();

    service.initialize().await.unwrap();

    assert_eq!(stored_verdict(&service, fresh.id).await, (true, None));
    assert_eq!(
        stored_verdict(&service, spaced.id).await,
        (false, Some(SPACED_COMMAND.to_string()))
    );
    assert_eq!(*repo.updates.lock().unwrap(), 2);

    service.initialize().await.unwrap();

    assert_eq!(
        *repo.updates.lock().unwrap(),
        2,
        "a verdict that has not changed is not written again"
    );
}

/// A server's info as its name, its status and the names of its tools.
#[cfg(unix)]
fn summary(info: McpServerInfo) -> (String, McpServerStatus, Vec<String>) {
    let tools = info.tools.into_iter().map(|tool| tool.name).collect();
    (info.server.name, info.status, tools)
}

#[cfg(unix)]
#[tokio::test]
async fn a_servers_info_is_its_status_and_its_tools_while_it_runs() {
    let (service, _) = service();
    let stand_in = service
        .add_server(NewMcpServer::new_stdio(
            "stand-in",
            "sh",
            vec!["-c".to_string(), STAND_IN_SERVER.to_string()],
            None,
        ))
        .await
        .unwrap();
    service.add_server(stdio("idle", "echo")).await.unwrap();
    let idle = ("idle".to_string(), McpServerStatus::Stopped, vec![]);

    service.start_server(stand_in.id).await.unwrap();

    let running = (
        "stand-in".to_string(),
        McpServerStatus::Running,
        vec!["echo".to_string()],
    );
    assert_eq!(
        service.get_server_status(stand_in.id).await,
        McpServerStatus::Running
    );
    let info = service.get_server_info(stand_in.id).await.unwrap();
    assert_eq!(summary(info), running);
    let listed = service.list_servers_with_status().await.unwrap();
    let listed: Vec<_> = listed.into_iter().map(summary).collect();
    assert_eq!(listed, [running, idle.clone()]);

    service.stop_server(stand_in.id).await.unwrap();

    let stopped = ("stand-in".to_string(), McpServerStatus::Stopped, vec![]);
    assert_eq!(
        service.get_server_status(stand_in.id).await,
        McpServerStatus::Stopped
    );
    let info = service.get_server_info(stand_in.id).await.unwrap();
    assert_eq!(summary(info), stopped);
    let listed = service.list_servers_with_status().await.unwrap();
    let listed: Vec<_> = listed.into_iter().map(summary).collect();
    assert_eq!(listed, [stopped, idle]);
}

#[tokio::test]
async fn a_tool_calls_arguments_are_an_object_or_nothing_for_an_mcp_tool_as_for_a_builtin_one() {
    use gglib_core::ToolCall;
    use gglib_core::ports::ToolExecutorPort;

    let (service, _) = service();
    let executor = crate::CombinedToolExecutor::new(Arc::new(service));
    let call = |name: &str, arguments: serde_json::Value| ToolCall {
        id: "call".to_string(),
        name: name.to_string(),
        arguments,
    };

    for name in ["7:echo", "builtin:get_current_time"] {
        let refused = executor
            .execute(&call(name, serde_json::json!([1])))
            .await
            .unwrap_err();
        assert_eq!(
            refused.to_string(),
            format!("tool '{name}' arguments must be a JSON object; got [1]")
        );
    }

    // No arguments is not a refusal: the builtin runs, and the MCP call gets as
    // far as asking for its server, which is not there.
    let ran = executor
        .execute(&call("builtin:get_current_time", serde_json::Value::Null))
        .await
        .unwrap();
    assert!(ran.success);
    let no_server = executor
        .execute(&call("7:echo", serde_json::Value::Null))
        .await
        .unwrap_err();
    assert!(
        no_server.to_string().starts_with("MCP call_tool failed: "),
        "got {no_server}"
    );
}

#[tokio::test]
async fn a_servers_extra_path_is_split_on_the_platforms_separator() {
    let (_, repo) = service();
    let (extra, expected) = if cfg!(windows) {
        (r"C:\tools;D:\bin", [r"C:\tools", r"D:\bin"])
    } else {
        (
            "/gglib-test/one:/gglib-test/two",
            ["/gglib-test/one", "/gglib-test/two"],
        )
    };
    let added = NewMcpServer::new_stdio("files", "npx", vec![], Some(extra.to_string()));
    let server = repo.insert(added).await.unwrap();

    assert_eq!(McpService::extract_user_search_paths(&server), expected);
}
