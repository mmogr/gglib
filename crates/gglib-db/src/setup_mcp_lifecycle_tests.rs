//! The step that retires the `manual` lifecycle, on servers an older build
//! stored: which rows it changes, which it leaves, and that a second run
//! changes nothing.

use sqlx::SqlitePool;

use super::super::create_schema;
use super::retire_manual_lifecycle;

/// Every server as `(name, lifecycle, enabled)`, in name order.
async fn servers(pool: &SqlitePool) -> Vec<(String, String, bool)> {
    sqlx::query_as("SELECT name, lifecycle, enabled FROM mcp_servers ORDER BY name")
        .fetch_all(pool)
        .await
        .expect("the rows")
}

/// A database with this build's schema holding `planted` as
/// `(name, lifecycle, enabled)`, as an older build could have stored them:
/// the schema still admits `manual`.
async fn stored_by_an_older_build(planted: &[(&str, &str, bool)]) -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    create_schema(&pool).await.unwrap();
    for (name, lifecycle, enabled) in planted {
        sqlx::query(
            "INSERT INTO mcp_servers (name, type, lifecycle, enabled) VALUES (?, 'stdio', ?, ?)",
        )
        .bind(name)
        .bind(lifecycle)
        .bind(enabled)
        .execute(&pool)
        .await
        .expect("a planted server");
    }
    pool
}

fn row(name: &str, lifecycle: &str, enabled: bool) -> (String, String, bool) {
    (name.to_owned(), lifecycle.to_owned(), enabled)
}

const PLANTED: &[(&str, &str, bool)] = &[
    ("eager-off", "eager", false),
    ("eager-on", "eager", true),
    ("lazy-off", "lazy", false),
    ("lazy-on", "lazy", true),
    ("manual-off", "manual", false),
    ("manual-on", "manual", true),
];

/// What every planted server is once the step has run.
fn retired() -> Vec<(String, String, bool)> {
    vec![
        row("eager-off", "eager", false),
        row("eager-on", "eager", true),
        row("lazy-off", "lazy", false),
        row("lazy-on", "lazy", true),
        row("manual-off", "lazy", false),
        row("manual-on", "lazy", false),
    ]
}

/// A `manual` server, enabled or not, opens as a disabled `lazy` one, so a
/// server that never ran unasked still does not; an eager or lazy one keeps
/// its lifecycle and its switch.
#[tokio::test]
async fn opening_the_database_makes_a_manual_server_lazy_and_disabled_and_leaves_the_rest() {
    let pool = stored_by_an_older_build(PLANTED).await;

    create_schema(&pool).await.unwrap();

    assert_eq!(servers(&pool).await, retired());
}

#[tokio::test]
async fn retiring_the_manual_lifecycle_twice_changes_nothing_more() {
    let pool = stored_by_an_older_build(PLANTED).await;

    retire_manual_lifecycle(&pool).await.unwrap();
    retire_manual_lifecycle(&pool).await.unwrap();
    create_schema(&pool).await.unwrap();

    assert_eq!(servers(&pool).await, retired());
}
