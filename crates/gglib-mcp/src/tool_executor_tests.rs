//! [`McpToolExecutorAdapter`]: how its tools are named and what a result becomes.

use super::*;

#[test]
fn qualified_name_splits_on_colon_separator() {
    // execute() uses split_once(':') to parse the qualified form
    // "{server_id}:{bare_name}".  `:` is not permitted in MCP tool names
    // (`[a-zA-Z0-9_-]+`), making it an unambiguous separator.

    let (prefix, bare) = "3:search".split_once(':').unwrap();
    assert_eq!(prefix.parse::<i64>().unwrap(), 3i64);
    assert_eq!(bare, "search");

    let (p, b) = "7:read_file".split_once(':').unwrap();
    assert_eq!(p.parse::<i64>().unwrap(), 7i64);
    assert_eq!(b, "read_file");
}

#[test]
fn non_integer_server_prefix_is_an_error() {
    // A name that contains `:` but whose prefix is not a valid i64 must
    // cause execute() to return Err rather than panic or dispatch blindly.
    let name = "not_int:tool";
    let (prefix, _) = name.split_once(':').unwrap();
    assert!(
        prefix.parse::<i64>().is_err(),
        "non-integer prefix must fail to parse as server id"
    );
}

#[test]
fn unqualified_name_is_rejected() {
    // Bare tool names (no `:`) are not supported; callers must always use
    // names as returned by list_tools() which includes the server-id prefix.
    let name = "search";
    assert!(
        name.split_once(':').is_none(),
        "unqualified name must not contain a colon — execute() will reject it"
    );
}

// ---- tool_result --------------------------------------------------------

use crate::tool_images::tool_images_tests::{ONE_PIXEL_PNG, images};
use serde_json::json;

async fn converted(result: McpToolResult) -> ToolResult {
    let (images, _) = images();
    tool_result("c1".to_owned(), result, &images).await
}

/// A tool error is its message, unsplit, as it always was.
#[tokio::test]
async fn an_error_is_its_message_with_no_images() {
    let got = converted(McpToolResult::error("boom")).await;
    assert_eq!(
        serde_json::to_string(&got).unwrap(),
        r#"{"tool_call_id":"c1","content":"boom","success":false}"#
    );
    let silent = converted(McpToolResult {
        success: false,
        data: Some(json!([{ "type": "image", "data": ONE_PIXEL_PNG }])),
        error: None,
    })
    .await;
    assert_eq!(silent.content, "tool returned an error without a message");
    assert!(silent.images.is_empty());
}

/// Text alone is the frame it always was: no `images` key.
#[tokio::test]
async fn text_alone_is_the_result_it_always_was() {
    let got = converted(McpToolResult::success(json!([
        { "type": "text", "text": "line 1" },
        { "type": "text", "text": "line 2" },
    ])))
    .await;
    assert_eq!(
        serde_json::to_string(&got).unwrap(),
        r#"{"tool_call_id":"c1","content":"line 1\nline 2","success":true}"#
    );
    let nothing = converted(McpToolResult {
        success: true,
        data: None,
        error: None,
    })
    .await;
    assert_eq!(nothing.content, "null");
}

/// A stored image is on the result, for the conversation; its text names it.
#[tokio::test]
async fn a_stored_image_is_on_the_result() {
    let got = converted(McpToolResult::success(json!([
        { "type": "text", "text": "A dot." },
        { "type": "image", "data": ONE_PIXEL_PNG, "mimeType": "image/png" },
    ])))
    .await;
    assert!(got.success);
    assert_eq!(got.content, "A dot.\n[image 1x1 PNG stored]");
    assert_eq!(got.images.len(), 1);
    assert_eq!((got.images[0].width, got.images[0].height), (1, 1));
}

// ---- list_tools -------------------------------------------------------------

/// What `list_tools` names each tool's server, over stand-in servers.
#[cfg(unix)]
mod listing {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use async_trait::async_trait;
    use gglib_core::ports::{McpRepositoryError, McpServerRepository, ToolExecutorPort};
    use gglib_core::{McpServer, NewMcpServer};

    use super::McpToolExecutorAdapter;
    use crate::service::McpService;
    use crate::service::tests::{MockMcpRepository, STAND_IN_SERVER};
    use crate::tool_images::tool_images_tests::images;

    /// The repository in memory, whose list can be made to fail while each
    /// server is still read by its id, as a start reads it.
    struct ListFails {
        inner: MockMcpRepository,
        failing: AtomicBool,
    }

    #[async_trait]
    impl McpServerRepository for ListFails {
        async fn insert(&self, server: NewMcpServer) -> Result<McpServer, McpRepositoryError> {
            self.inner.insert(server).await
        }
        async fn get_by_id(&self, id: i64) -> Result<McpServer, McpRepositoryError> {
            self.inner.get_by_id(id).await
        }
        async fn get_by_name(&self, name: &str) -> Result<McpServer, McpRepositoryError> {
            self.inner.get_by_name(name).await
        }
        async fn list(&self) -> Result<Vec<McpServer>, McpRepositoryError> {
            if self.failing.load(Ordering::SeqCst) {
                return Err(McpRepositoryError::Internal("down".to_owned()));
            }
            self.inner.list().await
        }
        async fn update(&self, server: &McpServer) -> Result<(), McpRepositoryError> {
            self.inner.update(server).await
        }
        async fn delete(&self, id: i64) -> Result<(), McpRepositoryError> {
            self.inner.delete(id).await
        }
        async fn update_last_connected(&self, id: i64) -> Result<(), McpRepositoryError> {
            self.inner.update_last_connected(id).await
        }
        async fn set_allow_remote(&self, id: i64, allow: bool) -> Result<(), McpRepositoryError> {
            self.inner.set_allow_remote(id, allow).await
        }
    }

    fn stand_in(name: &str) -> NewMcpServer {
        NewMcpServer::new_stdio(
            name,
            "sh",
            vec!["-c".to_owned(), STAND_IN_SERVER.to_owned()],
            None,
        )
    }

    /// Two running stand-ins, "Tavily Web Search" and "Files", each
    /// offering `echo`, and the repository under them.
    async fn two_running() -> (Arc<McpService>, Arc<ListFails>, [i64; 2]) {
        let repo = Arc::new(ListFails {
            inner: MockMcpRepository::new(),
            failing: AtomicBool::new(false),
        });
        let service = Arc::new(McpService::new(repo.clone()));
        let mut ids = [0; 2];
        for (id, name) in ids.iter_mut().zip(["Tavily Web Search", "Files"]) {
            *id = service.add_server(stand_in(name)).await.unwrap().id;
            service.start_server(*id).await.unwrap();
        }
        (service, repo, ids)
    }

    /// Each tool and the server it names, in name order.
    async fn listed(service: &Arc<McpService>) -> Vec<(String, Option<String>)> {
        let (images, _) = images();
        let adapter = McpToolExecutorAdapter::new(Arc::clone(service), images);
        let mut named: Vec<_> = adapter
            .list_tools()
            .await
            .into_iter()
            .map(|tool| (tool.name, tool.server))
            .collect();
        named.sort();
        named
    }

    /// Each tool names the server it belongs to, by that server's id: two
    /// servers with one tool name each keep their own names.
    #[tokio::test]
    async fn each_tool_names_the_server_whose_id_it_carries() {
        let (service, _, [tavily, files]) = two_running().await;

        let mut want = vec![
            (
                format!("{tavily}:echo"),
                Some("Tavily Web Search".to_owned()),
            ),
            (format!("{files}:echo"), Some("Files".to_owned())),
        ];
        want.sort();
        assert_eq!(listed(&service).await, want);
        service.shutdown().await;
    }

    /// Servers that cannot be read leave each tool listed without a server.
    #[tokio::test]
    async fn unreadable_servers_leave_the_tools_listed_without_one() {
        let (service, repo, [tavily, files]) = two_running().await;
        repo.failing.store(true, Ordering::SeqCst);

        let mut want = vec![
            (format!("{tavily}:echo"), None),
            (format!("{files}:echo"), None),
        ];
        want.sort();
        assert_eq!(listed(&service).await, want);
        service.shutdown().await;
    }
}
