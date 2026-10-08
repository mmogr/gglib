//! The `SQLite` MCP repository against the schema production creates.
//!
//! Every table here is made by `setup::create_schema`, the code that makes
//! them for a real database, so a column the repository and the schema
//! disagree on fails a test and not a user's `gglib mcp add`.

use chrono::{TimeZone, Utc};

use crate::setup::{setup_database, setup_test_database};

use super::*;

async fn repository() -> SqliteMcpRepository {
    let pool = setup_test_database().await.expect("setup_test_database");
    SqliteMcpRepository::new(pool)
}

/// How many env rows the database holds, whatever server they belong to.
async fn env_rows(repo: &SqliteMcpRepository) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM mcp_server_env")
        .fetch_one(&repo.pool)
        .await
        .unwrap()
}

/// A server whose env names one key twice. The second env row breaks the
/// table's `UNIQUE(server_id, key)`, so a write of it fails after the server
/// row and the first env row have gone in.
fn with_a_repeated_env_key(server: NewMcpServer) -> NewMcpServer {
    server
        .with_env("TOKEN", "first")
        .with_env("TOKEN", "second")
}

#[tokio::test]
async fn an_sse_server_is_added_and_read_back() {
    let repo = repository().await;

    let added = repo
        .insert(NewMcpServer::new_sse("remote", "http://localhost:3001/sse"))
        .await
        .expect("an SSE server has no args, and is stored all the same");

    let read = repo.get_by_id(added.id).await.unwrap();
    assert_eq!(read.server_type, McpServerType::Sse);
    assert_eq!(
        read.config.url,
        Some("http://localhost:3001/sse".to_string())
    );
    assert_eq!(read.config.command, None);

    let stored: (String, String) =
        sqlx::query_as("SELECT type, args FROM mcp_servers WHERE id = ?")
            .bind(added.id)
            .fetch_one(&repo.pool)
            .await
            .unwrap();
    assert_eq!(stored, ("sse".to_string(), "[]".to_string()));
}

#[tokio::test]
async fn a_stdio_server_reads_back_as_it_was_written() {
    let repo = repository().await;

    let new_server = NewMcpServer::new_stdio(
        "files",
        "npx",
        vec!["-y".to_string(), "mcp".to_string()],
        Some("/opt/tools/bin".to_string()),
    )
    .with_working_dir("/srv")
    .with_lifecycle(McpLifecycle::Manual)
    .with_enabled(false)
    .with_env("API_KEY", "secret123");

    let added = repo.insert(new_server).await.unwrap();

    for read in [
        repo.get_by_id(added.id).await.unwrap(),
        repo.get_by_name("files").await.unwrap(),
    ] {
        assert_eq!(read.id, added.id);
        assert_eq!(read.name, "files");
        assert_eq!(read.server_type, McpServerType::Stdio);
        assert_eq!(read.config.command, Some("npx".to_string()));
        assert_eq!(
            read.config.args,
            Some(vec!["-y".to_string(), "mcp".to_string()])
        );
        assert_eq!(read.config.working_dir, Some("/srv".to_string()));
        assert_eq!(read.config.path_extra, Some("/opt/tools/bin".to_string()));
        assert_eq!(read.lifecycle, McpLifecycle::Manual);
        assert!(!read.enabled);
        assert_eq!(read.env, vec![McpEnvEntry::new("API_KEY", "secret123")]);
        assert!(!read.is_valid, "a new server is not valid until checked");
        assert_eq!(read.last_error, None);
        assert_eq!(read.last_connected_at, None);
    }
}

#[tokio::test]
async fn servers_list_in_name_order() {
    let repo = repository().await;

    for name in ["server-b", "server-a"] {
        repo.insert(NewMcpServer::new_stdio(name, "cmd", vec![], None))
            .await
            .unwrap();
    }

    let names: Vec<String> = repo
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|server| server.name)
        .collect();
    assert_eq!(names, ["server-a", "server-b"]);
}

#[tokio::test]
async fn an_update_replaces_the_row_and_its_env() {
    let repo = repository().await;
    let mut server = repo
        .insert(
            NewMcpServer::new_stdio("updatable", "old-cmd", vec!["old".to_string()], None)
                .with_env("KEY", "old-value")
                .with_env("GONE", "dropped"),
        )
        .await
        .unwrap();

    server.name = "renamed".to_string();
    server.config.command = Some("new-cmd".to_string());
    server.config.args = Some(vec!["new".to_string()]);
    server.config.resolved_path_cache = Some("/usr/bin/new-cmd".to_string());
    server.env = vec![McpEnvEntry::new("KEY", "new-value")];
    server.enabled = false;
    server.is_valid = true;
    server.last_error = Some("was broken".to_string());
    repo.update(&server).await.unwrap();

    let read = repo.get_by_id(server.id).await.unwrap();
    assert_eq!(read.name, "renamed");
    assert_eq!(read.config.command, Some("new-cmd".to_string()));
    assert_eq!(read.config.args, Some(vec!["new".to_string()]));
    assert_eq!(
        read.config.resolved_path_cache,
        Some("/usr/bin/new-cmd".to_string())
    );
    assert_eq!(read.env, vec![McpEnvEntry::new("KEY", "new-value")]);
    assert!(!read.enabled);
    assert!(read.is_valid);
    assert_eq!(read.last_error, Some("was broken".to_string()));
}

#[tokio::test]
async fn an_update_touches_only_the_server_it_names() {
    let repo = repository().await;
    let mut first = repo
        .insert(NewMcpServer::new_stdio("first", "one", vec![], None).with_env("A", "1"))
        .await
        .unwrap();
    let second = repo
        .insert(NewMcpServer::new_stdio("second", "two", vec![], None).with_env("B", "2"))
        .await
        .unwrap();

    first.config.command = Some("uno".to_string());
    first.env = vec![];
    repo.update(&first).await.unwrap();

    let untouched = repo.get_by_id(second.id).await.unwrap();
    assert_eq!(untouched.config.command, Some("two".to_string()));
    assert_eq!(untouched.env, vec![McpEnvEntry::new("B", "2")]);
}

#[tokio::test]
async fn an_update_of_a_server_that_is_not_there_is_not_found() {
    let repo = repository().await;
    let mut server = repo
        .insert(NewMcpServer::new_stdio("only", "cmd", vec![], None).with_env("KEY", "kept"))
        .await
        .unwrap();
    let stored = server.id;

    server.id = stored + 100;
    server.env = vec![McpEnvEntry::new("KEY", "stray")];
    let result = repo.update(&server).await;

    assert!(matches!(result, Err(McpRepositoryError::NotFound(_))));
    assert_eq!(env_rows(&repo).await, 1, "no env row for a missing server");
    assert_eq!(
        repo.get_by_id(stored).await.unwrap().env,
        vec![McpEnvEntry::new("KEY", "kept")]
    );
}

/// `SQLite` counts a row an UPDATE matched as changed whatever was written
/// to it, so rewriting a server with what it already holds is not taken for
/// a server that is missing.
#[tokio::test]
async fn an_update_that_changes_nothing_still_finds_its_server() {
    let repo = repository().await;
    let server = repo
        .insert(NewMcpServer::new_stdio("same", "cmd", vec![], None).with_env("KEY", "v"))
        .await
        .unwrap();

    repo.update(&server).await.expect("the server is there");

    assert_eq!(repo.get_by_id(server.id).await.unwrap().env, server.env);
}

#[tokio::test]
async fn an_insert_that_fails_on_its_env_leaves_no_server() {
    let repo = repository().await;

    let result = repo
        .insert(with_a_repeated_env_key(NewMcpServer::new_stdio(
            "half",
            "cmd",
            vec![],
            None,
        )))
        .await;

    assert!(matches!(result, Err(McpRepositoryError::Internal(_))));
    assert!(
        repo.list().await.unwrap().is_empty(),
        "the server row must go when its env cannot be written"
    );
    assert_eq!(env_rows(&repo).await, 0);
}

#[tokio::test]
async fn an_update_that_fails_on_its_env_leaves_the_old_args_and_env() {
    let repo = repository().await;
    let stored = repo
        .insert(
            NewMcpServer::new_stdio("steady", "old-cmd", vec!["old".to_string()], None)
                .with_env("KEY", "old-value"),
        )
        .await
        .unwrap();

    let mut changed = stored.clone();
    changed.name = "moved".to_string();
    changed.config.command = Some("new-cmd".to_string());
    changed.config.args = Some(vec!["new".to_string()]);
    changed.env = vec![
        McpEnvEntry::new("TOKEN", "first"),
        McpEnvEntry::new("TOKEN", "second"),
    ];
    let result = repo.update(&changed).await;

    assert!(matches!(result, Err(McpRepositoryError::Internal(_))));
    let read = repo.get_by_id(stored.id).await.unwrap();
    assert_eq!(read.name, "steady");
    assert_eq!(read.config.command, Some("old-cmd".to_string()));
    assert_eq!(read.config.args, Some(vec!["old".to_string()]));
    assert_eq!(read.env, vec![McpEnvEntry::new("KEY", "old-value")]);
}

#[tokio::test]
async fn a_delete_takes_the_server_and_its_env_rows() {
    let repo = repository().await;
    let server = repo
        .insert(NewMcpServer::new_stdio("deletable", "cmd", vec![], None).with_env("KEY", "v"))
        .await
        .unwrap();

    repo.delete(server.id).await.unwrap();

    let result = repo.get_by_id(server.id).await;
    assert!(matches!(result, Err(McpRepositoryError::NotFound(_))));
    assert_eq!(env_rows(&repo).await, 0);
}

#[tokio::test]
async fn a_connection_is_stamped_on_the_server_it_names() {
    let repo = repository().await;
    let server = repo
        .insert(NewMcpServer::new_stdio("connectable", "cmd", vec![], None))
        .await
        .unwrap();
    assert!(server.last_connected_at.is_none());

    repo.update_last_connected(server.id).await.unwrap();

    let read = repo.get_by_id(server.id).await.unwrap();
    assert!(read.last_connected_at.is_some());
    assert!(matches!(
        repo.update_last_connected(server.id + 100).await,
        Err(McpRepositoryError::NotFound(_))
    ));
}

/// The strings the `type` column's CHECK admits are the strings production
/// has stored. Each is written here as SQL writes it, not through the
/// mapping under test.
#[tokio::test]
async fn every_type_string_the_schema_admits_reads_back_and_is_what_a_write_stores() {
    let repo = repository().await;

    for (stored, server_type) in [("stdio", McpServerType::Stdio), ("sse", McpServerType::Sse)] {
        sqlx::query(
            "INSERT INTO mcp_servers (name, type, created_at, last_connected_at) \
             VALUES (?, ?, '2020-01-02 03:04:05', '2021-06-07 08:09:10')",
        )
        .bind(format!("raw-{stored}"))
        .bind(stored)
        .execute(&repo.pool)
        .await
        .unwrap();

        let read = repo.get_by_name(&format!("raw-{stored}")).await.unwrap();
        assert_eq!(read.server_type, server_type);
        assert_eq!(
            read.created_at,
            Utc.with_ymd_and_hms(2020, 1, 2, 3, 4, 5).unwrap()
        );
        assert_eq!(
            read.last_connected_at,
            Some(Utc.with_ymd_and_hms(2021, 6, 7, 8, 9, 10).unwrap())
        );

        let mut written = read.clone();
        written.name = format!("written-{stored}");
        repo.update(&written).await.unwrap();
        let on_disk: String = sqlx::query_scalar("SELECT type FROM mcp_servers WHERE id = ?")
            .bind(read.id)
            .fetch_one(&repo.pool)
            .await
            .unwrap();
        assert_eq!(on_disk, stored);
    }

    let other = sqlx::query("INSERT INTO mcp_servers (name, type) VALUES ('raw-grpc', 'grpc')")
        .execute(&repo.pool)
        .await;
    assert!(other.is_err(), "the schema admits no third type string");
}

/// Names are kept unique by the service, not by an index: a database that
/// came to hold two servers of one name before it did must still open.
#[tokio::test]
async fn a_database_holding_two_servers_of_one_name_opens_and_lists_both() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gglib.db");

    let pool = setup_database(&path).await.unwrap();
    let repo = SqliteMcpRepository::new(pool.clone());
    for command in ["one", "two"] {
        repo.insert(NewMcpServer::new_stdio("twin", command, vec![], None))
            .await
            .unwrap();
    }
    pool.close().await;

    let reopened = setup_database(&path)
        .await
        .expect("a database with two servers of one name opens");
    let repo = SqliteMcpRepository::new(reopened);
    let mut commands: Vec<Option<String>> = repo
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|server| {
            assert_eq!(server.name, "twin");
            server.config.command
        })
        .collect();
    commands.sort();
    assert_eq!(commands, [Some("one".to_string()), Some("two".to_string())]);
}
