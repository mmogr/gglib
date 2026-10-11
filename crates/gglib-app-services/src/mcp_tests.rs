//! `McpOps` over the `SQLite` repository, on the schema production creates.

use std::sync::Arc;

use gglib_core::{McpLifecycle, McpServerRepository};
use gglib_db::{SqliteMcpRepository, setup_test_database};

use super::*;

/// The ops, with the service and the repository under them for a test that
/// has to compare against the one or store what the other would refuse.
async fn make_ops_over() -> (McpOps, Arc<McpService>, Arc<SqliteMcpRepository>) {
    let pool = setup_test_database().await.expect("in-memory DB");
    let repo = Arc::new(SqliteMcpRepository::new(pool));
    let mcp = Arc::new(McpService::new(repo.clone()));
    (McpOps::new(McpDeps { mcp: mcp.clone() }), mcp, repo)
}

async fn make_ops() -> McpOps {
    make_ops_over().await.0
}

fn stdio_req(name: &str) -> CreateMcpServerRequest {
    CreateMcpServerRequest {
        name: name.to_string(),
        server_type: "stdio".to_string(),
        command: Some("echo".to_string()),
        args: vec![],
        working_dir: None,
        path_extra: None,
        url: None,
        env: vec![],
        lifecycle: McpLifecycle::Lazy,
    }
}

#[tokio::test]
async fn list_returns_empty_on_fresh_db() {
    let ops = make_ops().await;
    let servers = ops.list().await.expect("list should succeed");
    assert!(servers.is_empty());
}

#[tokio::test]
async fn add_server_appears_in_list() {
    let ops = make_ops().await;
    ops.add(stdio_req("test-server"))
        .await
        .expect("add should succeed");

    let servers = ops.list().await.unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].server.name, "test-server");
    assert_eq!(servers[0].server.server_type, "stdio");
}

/// What an SSE server is refused with, wherever it is refused.
const SSE_NOT_SUPPORTED: &str = "SSE servers are not supported yet; only stdio servers can be run";

/// Whether `result` is the refusal of an SSE server, as a request the caller
/// is to change.
fn is_the_sse_refusal<T>(result: &Result<T, GuiError>) -> bool {
    matches!(result, Err(GuiError::ValidationFailed(why)) if why == SSE_NOT_SUPPORTED)
}

#[tokio::test]
async fn adding_an_sse_server_is_a_validation_failure_with_the_reason_and_stores_nothing() {
    let ops = make_ops().await;
    let mut req = stdio_req("remote");
    req.server_type = "sse".to_string();
    req.command = None;
    req.url = Some("http://localhost:3001/sse".to_string());

    let result = ops.add(req).await;

    assert!(is_the_sse_refusal(&result), "got {result:?}");
    assert!(ops.list().await.unwrap().is_empty());
}

/// The row is stored by the repository, as a database written while SSE
/// servers were accepted holds one.
#[tokio::test]
async fn a_stored_sse_server_is_listed_as_unsupported_refused_a_run_and_an_edit_and_still_removed()
{
    let (ops, _, repo) = make_ops_over().await;
    let stored = NewMcpServer::new_sse("remote", "http://localhost:3001/sse");
    let id = repo.insert(stored).await.unwrap().id;

    let listed = ops.list().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].server.server_type, "sse");
    assert!(
        matches!(listed[0].status, McpServerStatusDto::Unsupported),
        "got {:?}",
        listed[0].status
    );

    let started = ops.start(id).await;
    assert!(is_the_sse_refusal(&started), "got {started:?}");
    let disable = UpdateMcpServerRequest {
        enabled: Some(false),
        ..UpdateMcpServerRequest::default()
    };
    let edited = ops.update(id, disable).await;
    assert!(is_the_sse_refusal(&edited), "got {edited:?}");
    assert!(ops.list().await.unwrap()[0].server.enabled);
    let tested = ops.test_connection(id).await.unwrap();
    assert!(!tested.ok);
    assert_eq!(tested.error, Some(SSE_NOT_SUPPORTED.to_string()));

    ops.remove(id).await.expect("remove should succeed");
    assert!(ops.list().await.unwrap().is_empty());
}

#[tokio::test]
async fn invalid_server_type_returns_validation_error() {
    let ops = make_ops().await;
    let mut req = stdio_req("bad");
    req.server_type = "grpc".to_string(); // unsupported
    let result = ops.add(req).await;
    assert!(
        matches!(&result, Err(GuiError::ValidationFailed(why))
            if why == "unknown server type 'grpc'; expected stdio or sse"),
        "expected ValidationFailed, got {result:?}"
    );
}

#[tokio::test]
async fn remove_server_deletes_it() {
    let ops = make_ops().await;
    let info = ops.add(stdio_req("to-delete")).await.unwrap();
    ops.remove(info.server.id)
        .await
        .expect("remove should succeed");
    let servers = ops.list().await.unwrap();
    assert!(servers.is_empty());
}

#[tokio::test]
async fn adding_a_server_under_a_taken_name_is_a_conflict_that_names_it() {
    let ops = make_ops().await;
    ops.add(stdio_req("files")).await.unwrap();

    let result = ops.add(stdio_req("files")).await;

    assert!(
        matches!(&result, Err(GuiError::Conflict(why))
            if why == "An MCP server named 'files' already exists; choose another name"),
        "expected Conflict, got {result:?}"
    );
    assert_eq!(ops.list().await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_server_named_builtin_is_a_validation_failure_on_add_and_on_rename() {
    const KEPT: &str = "'builtin' is a name gglib keeps for its own tools; choose another name";
    let ops = make_ops().await;

    let added = ops.add(stdio_req("builtin")).await;
    assert!(
        matches!(&added, Err(GuiError::ValidationFailed(why)) if why == KEPT),
        "expected ValidationFailed, got {added:?}"
    );
    assert!(ops.list().await.unwrap().is_empty());

    let files = ops.add(stdio_req("files")).await.unwrap();
    let renamed = ops
        .update(
            files.server.id,
            UpdateMcpServerRequest {
                name: Some("builtin".to_string()),
                ..UpdateMcpServerRequest::default()
            },
        )
        .await;
    assert!(
        matches!(&renamed, Err(GuiError::ValidationFailed(why)) if why == KEPT),
        "expected ValidationFailed, got {renamed:?}"
    );
    assert_eq!(ops.list().await.unwrap()[0].server.name, "files");
}

#[tokio::test]
async fn renaming_a_server_to_a_taken_name_is_a_conflict_and_changes_nothing() {
    let ops = make_ops().await;
    ops.add(stdio_req("files")).await.unwrap();
    let other = ops.add(stdio_req("search")).await.unwrap();

    let result = ops
        .update(
            other.server.id,
            UpdateMcpServerRequest {
                name: Some("files".to_string()),
                command: Some("changed".to_string()),
                ..UpdateMcpServerRequest::default()
            },
        )
        .await;

    assert!(
        matches!(&result, Err(GuiError::Conflict(why))
            if why == "An MCP server named 'files' already exists; choose another name"),
        "expected Conflict, got {result:?}"
    );
    let servers = ops.list().await.unwrap();
    let stored = servers
        .iter()
        .find(|info| info.server.id == other.server.id)
        .expect("the server is still there");
    assert_eq!(stored.server.name, "search");
    assert_eq!(stored.server.config.command, Some("echo".to_string()));
}

/// Two servers of one name, stored as a database from before names were
/// refused holds them: each is listed, edited under the name it has, and
/// renamed apart.
#[tokio::test]
async fn servers_already_sharing_a_name_are_listed_edited_and_renamed_apart() {
    let (ops, _, repo) = make_ops_over().await;
    let mut ids = Vec::new();
    for command in ["one", "two"] {
        let stored = repo
            .insert(NewMcpServer::new_stdio("twin", command, vec![], None))
            .await
            .unwrap();
        ids.push(stored.id);
    }

    let listed = ops.list().await.unwrap();
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().all(|info| info.server.name == "twin"));

    let disabled = ops
        .update(
            ids[0],
            UpdateMcpServerRequest {
                enabled: Some(false),
                ..UpdateMcpServerRequest::default()
            },
        )
        .await
        .expect("a server is edited under the name it already has");
    assert!(!disabled.server.enabled);

    let renamed = ops
        .update(
            ids[1],
            UpdateMcpServerRequest {
                name: Some("single".to_string()),
                ..UpdateMcpServerRequest::default()
            },
        )
        .await
        .expect("one of the two is renamed apart");
    assert_eq!(renamed.server.name, "single");
}

/// The GUI's Test and `gglib mcp test` are one call into the service, so
/// what this reports for a row is what the command prints for it.
#[tokio::test]
async fn a_failed_test_reports_the_failure_the_service_reports() {
    let (ops, mcp, _) = make_ops_over().await;
    let mut req = stdio_req("ghost");
    req.command = Some("gglib-no-such-command".to_string());
    let added = ops.add(req).await.unwrap();

    let result = ops.test_connection(added.server.id).await.unwrap();

    let from_the_service = mcp.test_server(added.server.id).await.unwrap_err();
    assert!(!result.ok);
    assert!(result.tools.is_empty());
    assert_eq!(result.error, Some(from_the_service.to_string()));
    assert!(
        from_the_service
            .to_string()
            .contains("Executable path must be absolute: gglib-no-such-command"),
        "got {from_the_service}"
    );
}

#[tokio::test]
async fn testing_a_server_that_is_not_there_is_not_found() {
    let ops = make_ops().await;

    let result = ops.test_connection(42).await;

    assert!(
        matches!(&result, Err(GuiError::NotFound { entity: "MCP server", id }) if id == "42"),
        "expected NotFound, got {result:?}"
    );
}
