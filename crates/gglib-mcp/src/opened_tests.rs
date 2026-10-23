//! Which names `--allow-mcp` may be given: exactly one configured server
//! each, and never gglib's own tools.

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::ports::{McpRepositoryError, McpServerRepository};
use gglib_core::{McpServer, NewMcpServer};

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
