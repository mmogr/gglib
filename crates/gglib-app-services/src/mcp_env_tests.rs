//! A server's environment over `McpOps`: no response carries a value, and an
//! update keeps a value it leaves out (ADR 0027).

use std::sync::Arc;

use gglib_core::{McpLifecycle, McpServerRepository};
use gglib_db::{SqliteMcpRepository, setup_test_database};

use super::*;
use crate::types::McpEnvEntryDto;

const TOKEN: &str = "t0k-81d0e4";

/// The ops, and the repository under them to read what was stored.
async fn ops_over() -> (McpOps, Arc<SqliteMcpRepository>) {
    let pool = setup_test_database().await.expect("in-memory DB");
    let repo = Arc::new(SqliteMcpRepository::new(pool));
    let mcp = Arc::new(McpService::new(repo.clone()));
    (McpOps::new(McpDeps { mcp }), repo)
}

fn entry(key: &str, value: &str) -> McpEnvEntryDto {
    McpEnvEntryDto {
        key: key.to_owned(),
        value: value.to_owned(),
    }
}

fn kept(key: &str) -> McpEnvUpdateDto {
    McpEnvUpdateDto {
        key: key.to_owned(),
        value: None,
    }
}

fn set(key: &str, value: &str) -> McpEnvUpdateDto {
    McpEnvUpdateDto {
        key: key.to_owned(),
        value: Some(value.to_owned()),
    }
}

fn create(env: Vec<McpEnvEntryDto>) -> CreateMcpServerRequest {
    CreateMcpServerRequest {
        name: "files".to_owned(),
        server_type: "stdio".to_owned(),
        command: Some("echo".to_owned()),
        args: vec![],
        working_dir: None,
        path_extra: None,
        url: None,
        env,
        lifecycle: McpLifecycle::Lazy,
    }
}

fn env_update(env: Vec<McpEnvUpdateDto>) -> UpdateMcpServerRequest {
    UpdateMcpServerRequest {
        env: Some(env),
        ..UpdateMcpServerRequest::default()
    }
}

/// A server with `TOKEN` and `ROOT` stored; its id.
async fn seeded(ops: &McpOps) -> i64 {
    let added = ops
        .add(create(vec![entry("TOKEN", TOKEN), entry("ROOT", "/srv")]))
        .await
        .expect("add");
    added.server.id
}

/// What the repository holds for server `id`, as `(key, value)`.
async fn stored(repo: &SqliteMcpRepository, id: i64) -> Vec<(String, String)> {
    let mut env: Vec<_> = repo
        .get_by_id(id)
        .await
        .expect("stored")
        .env
        .into_iter()
        .map(|e| (e.key, e.value))
        .collect();
    env.sort();
    env
}

fn pairs(env: &[(&str, &str)]) -> Vec<(String, String)> {
    env.iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

#[tokio::test]
async fn list_add_and_update_answer_with_the_keys_and_never_a_value() {
    let (ops, _) = ops_over().await;

    let added = ops
        .add(create(vec![entry("TOKEN", TOKEN)]))
        .await
        .expect("add");
    let listed = ops.list().await.expect("list");
    let updated = ops
        .update(added.server.id, env_update(vec![kept("TOKEN")]))
        .await
        .expect("update");

    for (route, server) in [
        ("add", &added.server),
        ("list", &listed[0].server),
        ("update", &updated.server),
    ] {
        assert_eq!(server.env_keys, ["TOKEN"], "{route}");
        let json = serde_json::to_string(server).unwrap();
        assert!(!json.contains(TOKEN), "{route} sent the value: {json}");
    }
}

#[tokio::test]
async fn an_entry_sent_without_a_value_keeps_the_stored_one() {
    let (ops, repo) = ops_over().await;
    let id = seeded(&ops).await;

    ops.update(id, env_update(vec![kept("TOKEN"), kept("ROOT")]))
        .await
        .expect("update");

    assert_eq!(
        stored(&repo, id).await,
        pairs(&[("ROOT", "/srv"), ("TOKEN", TOKEN)])
    );
}

#[tokio::test]
async fn a_new_value_replaces_and_a_dropped_entry_is_deleted() {
    let (ops, repo) = ops_over().await;
    let id = seeded(&ops).await;

    ops.update(
        id,
        env_update(vec![set("TOKEN", "rotated"), set("NEW", "1")]),
    )
    .await
    .expect("update");

    assert_eq!(
        stored(&repo, id).await,
        pairs(&[("NEW", "1"), ("TOKEN", "rotated")])
    );
}

/// No value to keep, or a blank one: refused, and nothing is written, the
/// other fields of the same update included.
#[tokio::test]
async fn an_unknown_key_with_no_value_or_a_blank_value_is_refused_and_stores_nothing() {
    let (ops, repo) = ops_over().await;
    let id = seeded(&ops).await;

    for env in [
        vec![kept("TOKEN"), kept("NEVER_STORED")],
        vec![kept("TOKEN"), set("ROOT", "   ")],
    ] {
        let request = UpdateMcpServerRequest {
            name: Some("renamed".to_owned()),
            ..env_update(env)
        };
        let result = ops.update(id, request).await;

        assert!(
            matches!(result, Err(GuiError::ValidationFailed(ref why)) if why.contains("enter a value")),
            "{result:?}"
        );
        assert_eq!(
            stored(&repo, id).await,
            pairs(&[("ROOT", "/srv"), ("TOKEN", TOKEN)])
        );
        assert_eq!(repo.get_by_id(id).await.unwrap().name, "files");
    }
}

/// A blank an older gglib stored is kept as it is: only a value sent is held
/// to the blank rule.
#[tokio::test]
async fn a_stored_blank_left_out_of_an_update_is_kept() {
    let (ops, repo) = ops_over().await;
    let id = repo
        .insert(
            NewMcpServer::new_stdio("files", "echo", vec![], None)
                .with_env("TOKEN", TOKEN)
                .with_env("ROOT", ""),
        )
        .await
        .expect("insert")
        .id;

    ops.update(id, env_update(vec![kept("TOKEN"), kept("ROOT")]))
        .await
        .expect("update");

    assert_eq!(
        stored(&repo, id).await,
        pairs(&[("ROOT", ""), ("TOKEN", TOKEN)])
    );
}

#[tokio::test]
async fn a_blank_value_is_refused_on_add_and_stores_nothing() {
    let (ops, _) = ops_over().await;

    let result = ops
        .add(create(vec![entry("TOKEN", TOKEN), entry("ROOT", "")]))
        .await;

    assert!(
        matches!(result, Err(GuiError::ValidationFailed(ref why)) if why.contains("ROOT")),
        "{result:?}"
    );
    assert!(ops.list().await.unwrap().is_empty());
}
