//! A data directory for a test to point `GGLIB_DATA_DIR` at, with its
//! settings written and read, a chat saved and read back, an MCP server
//! stored, and a model added to its catalogue, through the stores the
//! binary's bootstrap wires.
//!
//! Lives in a subdirectory because anything directly under `tests/` is built
//! as its own test binary; `#[path]`-included from the suites that need it.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gglib_bootstrap::{BootstrapConfig, BuiltCore, CoreBootstrap};
use gglib_core::domain::NewModel;
use gglib_core::domain::chat::{Conversation, NewConversation};
use gglib_core::domain::mcp::NewMcpServer;
use gglib_core::{NoopEmitter, Settings};

/// The database the binary opens when `GGLIB_DATA_DIR` is `root`.
pub(crate) fn database(root: &Path) -> PathBuf {
    root.join("data").join("gglib.db")
}

/// Apply `change` to the settings in `root`'s database, creating the
/// database first if there is none.
pub(crate) fn write_settings(root: &Path, change: impl Fn(&mut Settings) + Send + Sync) {
    runtime().block_on(async {
        let built = open(root).await;
        built
            .repos
            .settings
            .modify(&|settings: &mut Settings| {
                change(settings);
                Ok(())
            })
            .await
            .expect("the settings are written");
        built.pool.close().await;
    });
}

/// The settings stored in `root`'s database.
pub(crate) fn read_settings(root: &Path) -> Settings {
    runtime().block_on(async {
        let built = open(root).await;
        let settings = built
            .repos
            .settings
            .load()
            .await
            .expect("the settings load");
        built.pool.close().await;
        settings
    })
}

/// Save `chat` in `root`'s database, creating the database first if there
/// is none: the conversation's id.
pub(crate) fn save_chat(root: &Path, chat: NewConversation) -> i64 {
    runtime().block_on(async {
        let built = open(root).await;
        let saved = built.app.chat_history().create_conversation(chat).await;
        built.pool.close().await;
        saved.expect("the chat is saved")
    })
}

/// The chat `id` as `root`'s database stores it.
pub(crate) fn read_chat(root: &Path, id: i64) -> Conversation {
    runtime().block_on(async {
        let built = open(root).await;
        let read = built.app.chat_history().get_conversation(id).await;
        built.pool.close().await;
        read.expect("the chat is read").expect("the chat is there")
    })
}

/// Store `server` in `root`'s database, creating the database first if there
/// is none. The row is written by the repository, past what `gglib mcp add`
/// refuses.
pub(crate) fn store_mcp_server(root: &Path, server: NewMcpServer) {
    runtime().block_on(async {
        let built = open(root).await;
        let stored = built.repos.mcp_servers.insert(server).await;
        built.pool.close().await;
        stored.expect("the server is stored");
    });
}

/// Add `model` to the catalogue in `root`'s database, creating the database
/// first if there is none.
pub(crate) fn add_model(root: &Path, model: NewModel) {
    runtime().block_on(async {
        let built = open(root).await;
        let added = built.app.models().add(model).await;
        built.pool.close().await;
        added.expect("the model is added");
    });
}

/// `root`'s database, opened through `CoreBootstrap::build` as the binary's
/// own bootstrap opens it.
async fn open(root: &Path) -> BuiltCore {
    let config = BootstrapConfig {
        db_path: database(root),
    };
    CoreBootstrap::build(config, Arc::new(NoopEmitter::new()))
        .await
        .expect("the database opens")
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
}
