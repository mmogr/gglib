//! The daemon's startup repairs: what they change, and who calls them.

use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::setup::setup_test_database;

#[tokio::test]
async fn the_repairs_fail_a_stuck_benchmark_run_and_delete_an_old_unlinked_image() {
    let pool = setup_test_database().await.unwrap();
    sqlx::query(
        "INSERT INTO benchmark_runs (run_type, status, model_ids, created_at) \
         VALUES ('perf', 'running', '[]', datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO attachments (id, mime, width, height, data, created_at) \
         VALUES ('orphan', 'image/png', 1, 1, x'00', datetime('now', '-2 days'))",
    )
    .execute(&pool)
    .await
    .unwrap();

    repair_at_daemon_start(&pool).await;

    let status: String = sqlx::query_scalar("SELECT status FROM benchmark_runs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "failed");
    let images: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attachments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(images, 0);
}

/// The order `gglib q --image` runs in when no daemon is up: the CLI stores
/// the image through its own pool, the daemon it then starts runs its
/// repairs on the same file, and the turn reads the image and saves the
/// message that carries it.
#[tokio::test]
async fn an_image_stored_before_the_daemon_starts_is_still_there_for_its_turn() {
    use gglib_core::domain::attachment::{AttachmentId, AttachmentInfo};
    use gglib_core::domain::chat::{MessageRole, NewConversation, NewMessage};
    use gglib_core::ports::{AttachmentStore, ChatHistoryRepository};

    use crate::repositories::{SqliteAttachmentStore, SqliteChatHistoryRepository};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gglib.db");
    let cli = crate::setup_database(&path).await.unwrap();
    let store = SqliteAttachmentStore::new(cli.clone());
    let bytes = b"the image as it is on disk".to_vec();
    let image = AttachmentInfo {
        id: AttachmentId::of(&bytes),
        mime: "image/png".to_owned(),
        width: 64,
        height: 32,
    };
    store.put(&image, &bytes).await.unwrap();

    let daemon = crate::setup_database(&path).await.unwrap();
    repair_at_daemon_start(&daemon).await;

    assert_eq!(store.blob(&image.id).await.unwrap().unwrap().data, bytes);
    let history = SqliteChatHistoryRepository::new(cli.clone());
    let conversation_id = history
        .create_conversation(NewConversation {
            title: "c".to_owned(),
            model_id: None,
            system_prompt: None,
            settings: None,
        })
        .await
        .unwrap();
    history
        .save_message(NewMessage {
            conversation_id,
            role: MessageRole::User,
            content: "look".to_owned(),
            metadata: None,
            images: vec![image.id.clone()],
        })
        .await
        .unwrap();
    let saved = history.get_messages(conversation_id).await.unwrap();
    assert_eq!(saved[0].images, [image]);
    daemon.close().await;
    cli.close().await;
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read a source directory") {
        let path = entry.expect("a directory entry").path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if name != "target" {
                rust_sources(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs") && !name.ends_with("_tests.rs") {
            out.push(path);
        }
    }
}

/// Every file under the workspace's crates, by its path from the root, that
/// has a line calling `function`.
fn callers_of(function: &str) -> Vec<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let mut files = Vec::new();
    for krate in fs::read_dir(root.join("crates")).expect("read crates/") {
        let src = krate.expect("a crate").path().join("src");
        if src.is_dir() {
            rust_sources(&src, &mut files);
        }
    }
    rust_sources(&root.join("src-tauri").join("src"), &mut files);

    let call = format!("{function}(");
    let mut callers: Vec<String> = files
        .iter()
        .filter(|path| {
            let text = fs::read_to_string(path).expect("read a source file");
            // A call: not the definition, a re-export or a mention in a doc.
            text.lines().any(|line| {
                line.contains(&call)
                    && !line.contains("fn ")
                    && !line.trim_start().starts_with("//")
            })
        })
        .map(|path| {
            let within = path.strip_prefix(&root).expect("a file under the root");
            let parts: Vec<_> = within
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect();
            parts.join("/")
        })
        .collect();
    callers.sort();
    callers
}

/// The sweep is called by the daemon's startup repairs, and those by the
/// daemon's bootstrap, and by nothing else: no CLI command, and no other
/// host of the database, deletes an image.
#[test]
fn only_the_daemons_bootstrap_reaches_the_sweep() {
    assert_eq!(
        callers_of("sweep_unlinked_attachments"),
        ["crates/gglib-db/src/daemon_startup.rs"]
    );
    assert_eq!(
        callers_of("repair_at_daemon_start"),
        ["crates/gglib-axum/src/bootstrap.rs"]
    );
}
