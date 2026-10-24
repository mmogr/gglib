//! The step that flags the servers the remote tunnel's list named, on a
//! database file of the test's own: which rows it flags, which it never
//! does, and that it runs once.

use super::*;
use crate::setup_database;

/// A database as the build before this step left it: this build's schema,
/// stamped at the recipes step's version, holding `servers` as
/// `(name, type)` and `settings` as `(key, raw value)`.
async fn left_by_an_older_build(
    path: &std::path::Path,
    servers: &[(&str, &str)],
    settings: &[(&str, &str)],
) -> SqlitePool {
    let pool = setup_database(path).await.expect("a fresh database");
    for (name, kind) in servers {
        sqlx::query("INSERT INTO mcp_servers (name, type) VALUES (?, ?)")
            .bind(name)
            .bind(kind)
            .execute(&pool)
            .await
            .expect("a planted server");
    }
    for (key, value) in settings {
        sqlx::query("INSERT INTO settings_kv (key, value, updated_at) VALUES (?, ?, 'then')")
            .bind(key)
            .bind(value)
            .execute(&pool)
            .await
            .expect("a planted row");
    }
    sqlx::query("PRAGMA user_version = 3")
        .execute(&pool)
        .await
        .expect("a stamp");
    pool
}

/// The flagged servers as `(name, type)`, in name order.
async fn flagged(pool: &SqlitePool) -> Vec<(String, String)> {
    sqlx::query_as("SELECT name, type FROM mcp_servers WHERE allow_remote = 1 ORDER BY name, id")
        .fetch_all(pool)
        .await
        .expect("the rows")
}

async fn version(pool: &SqlitePool) -> i64 {
    user_version(pool).await.expect("a version")
}

fn stdio(name: &str) -> (String, String) {
    (name.to_owned(), "stdio".to_owned())
}

const ON: (&str, &str) = ("remote_enabled", "true");

#[tokio::test]
async fn the_listed_server_is_flagged_and_no_other() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let pool = left_by_an_older_build(
        &dir.path().join("gglib.db"),
        &[("tavily", "stdio"), ("fs", "stdio")],
        &[ON, ("remote_serve", r#"{"mcpServers":["tavily"]}"#)],
    )
    .await;

    flag_listed_servers(&pool).await.expect("the step");

    assert_eq!(flagged(&pool).await, [stdio("tavily")]);
    assert_eq!(version(&pool).await, REMOTE_GRANT_SCHEMA_VERSION);
}

/// A machine whose tunnel was off reset its list at its next `enable`, so
/// the list opens nothing, however it reads.
#[tokio::test]
async fn a_tunnel_that_was_off_opens_nothing() {
    for enabled in [Some("false"), Some("null"), None] {
        let dir = tempfile::tempdir().expect("a temp dir");
        let mut settings = vec![("remote_serve", r#"{"mcpServers":["tavily"]}"#)];
        if let Some(enabled) = enabled {
            settings.push(("remote_enabled", enabled));
        }
        let pool = left_by_an_older_build(
            &dir.path().join("gglib.db"),
            &[("tavily", "stdio")],
            &settings,
        )
        .await;

        flag_listed_servers(&pool).await.expect("the step");

        assert_eq!(flagged(&pool).await, [], "remote_enabled {enabled:?}");
        assert_eq!(version(&pool).await, REMOTE_GRANT_SCHEMA_VERSION);
    }
}

/// Two servers sharing a name opened neither under the list, and neither is
/// flagged; an SSE server of a listed name never ran, and is not either.
#[tokio::test]
async fn a_shared_name_and_an_sse_server_are_not_flagged() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let pool = left_by_an_older_build(
        &dir.path().join("gglib.db"),
        &[
            ("tavily", "stdio"),
            ("tavily", "stdio"),
            ("remote", "sse"),
            ("fs", "stdio"),
        ],
        &[
            ON,
            ("remote_serve", r#"{"mcpServers":["tavily","remote","fs"]}"#),
        ],
    )
    .await;

    flag_listed_servers(&pool).await.expect("the step");

    assert_eq!(flagged(&pool).await, [stdio("fs")]);
}

/// `builtin` is gglib's own tools: a server stored under it before the name
/// was kept is not opened by a list that names it.
#[tokio::test]
async fn builtin_is_not_flagged() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let pool = left_by_an_older_build(
        &dir.path().join("gglib.db"),
        &[("builtin", "stdio")],
        &[ON, ("remote_serve", r#"{"mcpServers":["builtin"]}"#)],
    )
    .await;

    flag_listed_servers(&pool).await.expect("the step");

    assert_eq!(flagged(&pool).await, []);
}

/// A name is matched trimmed and whole: a padded entry opens its server, a
/// blank one and another case of a name open nothing.
#[tokio::test]
async fn a_name_is_trimmed_and_matched_whole() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let pool = left_by_an_older_build(
        &dir.path().join("gglib.db"),
        &[("tavily", "stdio"), ("Fs", "stdio"), ("", "stdio")],
        &[
            ON,
            ("remote_serve", r#"{"mcpServers":[" tavily ","fs","  ",7]}"#),
        ],
    )
    .await;

    flag_listed_servers(&pool).await.expect("the step");

    assert_eq!(flagged(&pool).await, [stdio("tavily")]);
}

/// Rows that cannot be read copy nothing, and the step is stamped all the
/// same, so the boot goes on and no later boot tries again.
#[tokio::test]
async fn rows_that_cannot_be_read_open_nothing_and_are_stamped() {
    let cases: &[&[(&str, &str)]] = &[
        &[ON, ("remote_serve", r#"{"mcpServers":["tavily""#)],
        &[
            ("remote_enabled", "tru"),
            ("remote_serve", r#"{"mcpServers":["tavily"]}"#),
        ],
        &[ON, ("remote_serve", r#"{"mcpServers":"tavily"}"#)],
        &[ON, ("remote_serve", r#"["tavily"]"#)],
        &[ON],
    ];
    for (case, settings) in cases.iter().enumerate() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let pool = left_by_an_older_build(
            &dir.path().join("gglib.db"),
            &[("tavily", "stdio")],
            settings,
        )
        .await;

        flag_listed_servers(&pool).await.expect("the step");

        assert_eq!(flagged(&pool).await, [], "case {case}");
        assert_eq!(
            version(&pool).await,
            REMOTE_GRANT_SCHEMA_VERSION,
            "case {case}"
        );
    }
}

/// Only the two rows are read: a row of another setting that is not JSON
/// does not close what the list opened.
#[tokio::test]
async fn another_rows_bad_value_does_not_stop_the_copy() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let pool = left_by_an_older_build(
        &dir.path().join("gglib.db"),
        &[("tavily", "stdio")],
        &[
            ON,
            ("remote_serve", r#"{"mcpServers":["tavily"]}"#),
            ("bind_host", "0.0.0.0"),
        ],
    )
    .await;

    flag_listed_servers(&pool).await.expect("the step");

    assert_eq!(flagged(&pool).await, [stdio("tavily")]);
}

/// Once stamped, the list is not read again: a server unflagged since stays
/// unflagged.
#[tokio::test]
async fn the_step_runs_once() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let pool = left_by_an_older_build(
        &dir.path().join("gglib.db"),
        &[("tavily", "stdio")],
        &[ON, ("remote_serve", r#"{"mcpServers":["tavily"]}"#)],
    )
    .await;
    flag_listed_servers(&pool).await.expect("the step");
    sqlx::query("UPDATE mcp_servers SET allow_remote = 0")
        .execute(&pool)
        .await
        .expect("a deny");

    flag_listed_servers(&pool).await.expect("a second open");

    assert_eq!(flagged(&pool).await, []);
}

/// A new database has no list and no server: it is stamped and nothing else.
#[tokio::test]
async fn a_fresh_database_is_stamped_and_flags_nothing() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let pool = setup_database(&dir.path().join("gglib.db"))
        .await
        .expect("a fresh database");

    assert_eq!(flagged(&pool).await, []);
    assert_eq!(version(&pool).await, REMOTE_GRANT_SCHEMA_VERSION);
}

/// The step as a boot runs it: an older file opened by this build has its
/// listed server flagged by `setup_database` itself.
#[tokio::test]
async fn a_boot_runs_the_step() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("gglib.db");
    left_by_an_older_build(
        &path,
        &[("tavily", "stdio")],
        &[ON, ("remote_serve", r#"{"mcpServers":["tavily"]}"#)],
    )
    .await
    .close()
    .await;

    let pool = setup_database(&path).await.expect("the boot");

    assert_eq!(flagged(&pool).await, [stdio("tavily")]);
}
