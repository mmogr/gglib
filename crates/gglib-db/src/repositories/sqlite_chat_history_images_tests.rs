//! The images a message carries: linked when it is saved, read back with
//! it, and unlinked when it goes.

use gglib_core::domain::attachment::{AttachmentId, AttachmentInfo};
use gglib_core::domain::chat::{MessageRole, NewConversation, NewMessage};
use gglib_core::ports::attachment_store::{AttachmentError, AttachmentStore};
use gglib_core::ports::chat_history::ChatHistoryRepository;

use crate::repositories::SqliteAttachmentStore;
use crate::setup::setup_test_database;

use super::*;

struct Fixture {
    repo: SqliteChatHistoryRepository,
    store: SqliteAttachmentStore,
    conversation: i64,
}

async fn fixture() -> Fixture {
    let pool = setup_test_database().await.expect("setup_test_database");
    let repo = SqliteChatHistoryRepository::new(pool.clone());
    let conversation = repo
        .create_conversation(NewConversation {
            title: "Screenshots".to_owned(),
            model_id: None,
            system_prompt: None,
            settings: None,
        })
        .await
        .unwrap();
    Fixture {
        repo,
        store: SqliteAttachmentStore::new(pool),
        conversation,
    }
}

impl Fixture {
    /// Store an image whose bytes are `name`, `width` pixels wide.
    async fn image(&self, name: &str, width: u32) -> AttachmentInfo {
        let info = AttachmentInfo {
            id: AttachmentId::of(name.as_bytes()),
            mime: "image/png".to_owned(),
            width,
            height: 100,
        };
        self.store.put(&info, name.as_bytes()).await.unwrap();
        info
    }

    fn message(&self, content: &str, images: &[&AttachmentInfo]) -> NewMessage {
        NewMessage {
            conversation_id: self.conversation,
            role: MessageRole::User,
            content: content.to_owned(),
            metadata: None,
            images: images.iter().map(|image| image.id.clone()).collect(),
        }
    }

    async fn count(&self, table: &str) -> i64 {
        sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&self.repo.pool)
            .await
            .unwrap()
    }

    async fn updated_at(&self) -> String {
        sqlx::query_scalar("SELECT updated_at FROM chat_conversations WHERE id = ?")
            .bind(self.conversation)
            .fetch_one(&self.repo.pool)
            .await
            .unwrap()
    }

    /// Set the conversation's timestamp to one no save would write, so a
    /// save that touched it shows.
    async fn stamp(&self) -> String {
        sqlx::query("UPDATE chat_conversations SET updated_at = '2001-01-01 00:00:00'")
            .execute(&self.repo.pool)
            .await
            .unwrap();
        self.updated_at().await
    }
}

/// Whether `error` is the typed refusal of `id`.
fn is_not_found(error: &ChatHistoryError, id: &AttachmentId) -> bool {
    matches!(
        error,
        ChatHistoryError::Attachment(AttachmentError::NotFound(missing)) if missing == id
    )
}

#[tokio::test]
async fn a_saved_message_reads_back_its_images_in_the_order_sent_without_bytes() {
    let f = fixture().await;
    let (a, b, c) = (
        f.image("a", 1).await,
        f.image("b", 2).await,
        f.image("c", 3).await,
    );
    // Not the order the images were stored in, nor the order of their ids,
    // and one image twice.
    let id = f
        .repo
        .save_message(f.message("look", &[&c, &a, &b, &a]))
        .await
        .unwrap();
    f.repo.save_message(f.message("words", &[])).await.unwrap();

    let messages = f.repo.get_messages(f.conversation).await.unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].id, id);
    assert_eq!(
        messages[0].images,
        vec![c.clone(), a.clone(), b.clone(), a.clone()]
    );
    assert!(messages[1].images.is_empty());
    // What a client is sent has the facts and nothing of the bytes.
    let wire = serde_json::to_value(&messages[0]).unwrap();
    assert_eq!(
        wire["images"][0],
        serde_json::json!({"id": c.id.as_str(), "mime": "image/png", "width": 3, "height": 100})
    );
}

#[tokio::test]
async fn a_message_naming_an_unknown_image_is_refused_and_nothing_is_saved() {
    let f = fixture().await;
    let a = f.image("a", 1).await;
    let missing = AttachmentId::of(b"never uploaded");
    let stamp = f.stamp().await;
    let mut message = f.message("look", &[&a]);
    message.images.push(missing.clone());

    let refusal = f.repo.save_message(message).await.unwrap_err();

    assert!(is_not_found(&refusal, &missing), "{refusal:?}");
    assert_eq!(f.count("chat_messages").await, 0);
    assert_eq!(f.count("message_attachments").await, 0);
    assert_eq!(f.updated_at().await, stamp);
}

#[tokio::test]
async fn a_batch_with_one_unknown_image_saves_none_of_its_messages() {
    let f = fixture().await;
    let a = f.image("a", 1).await;
    let missing = AttachmentId::of(b"never uploaded");
    let mut last = f.message("third", &[]);
    last.images.push(missing.clone());

    let refusal = f
        .repo
        .save_messages(vec![
            f.message("first", &[&a]),
            f.message("second", &[]),
            last,
        ])
        .await
        .unwrap_err();

    assert!(is_not_found(&refusal, &missing), "{refusal:?}");
    assert_eq!(f.count("chat_messages").await, 0);
    assert_eq!(f.count("message_attachments").await, 0);
}

#[tokio::test]
async fn a_batch_links_each_message_to_its_own_images() {
    let f = fixture().await;
    let (a, b) = (f.image("a", 1).await, f.image("b", 2).await);
    f.repo
        .save_messages(vec![
            f.message("first", &[&a]),
            f.message("second", &[]),
            f.message("third", &[&b, &a]),
        ])
        .await
        .unwrap();
    let messages = f.repo.get_messages(f.conversation).await.unwrap();
    let images: Vec<_> = messages.iter().map(|m| m.images.clone()).collect();
    assert_eq!(images, vec![vec![a.clone()], vec![], vec![b, a]]);
}

#[tokio::test]
async fn replace_from_drops_the_old_links_and_links_the_new_message() {
    let f = fixture().await;
    let (a, b) = (f.image("a", 1).await, f.image("b", 2).await);
    let first = f
        .repo
        .save_message(f.message("first", &[&a]))
        .await
        .unwrap();
    f.repo
        .save_message(f.message("second", &[&a, &b]))
        .await
        .unwrap();

    let id = f
        .repo
        .replace_from(first, f.message("again", &[&b]))
        .await
        .unwrap();

    let messages = f.repo.get_messages(f.conversation).await.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!((messages[0].id, &messages[0].images), (id, &vec![b]));
    assert_eq!(f.count("message_attachments").await, 1);
    // The images themselves stay until a daemon start sweeps them.
    assert_eq!(f.count("attachments").await, 2);
}

#[tokio::test]
async fn replace_from_naming_an_unknown_image_changes_nothing() {
    let f = fixture().await;
    let a = f.image("a", 1).await;
    let first = f
        .repo
        .save_message(f.message("first", &[&a]))
        .await
        .unwrap();
    f.repo
        .save_message(f.message("second", &[&a]))
        .await
        .unwrap();
    let missing = AttachmentId::of(b"never uploaded");
    let mut again = f.message("again", &[]);
    again.images.push(missing.clone());

    let refusal = f.repo.replace_from(first, again).await.unwrap_err();

    assert!(is_not_found(&refusal, &missing), "{refusal:?}");
    let messages = f.repo.get_messages(f.conversation).await.unwrap();
    let contents: Vec<_> = messages.iter().map(|m| m.content.as_str()).collect();
    assert_eq!(contents, ["first", "second"]);
    assert_eq!(f.count("message_attachments").await, 2);
}

#[tokio::test]
async fn deleting_messages_drops_their_links_and_keeps_the_rest() {
    let f = fixture().await;
    let (a, b) = (f.image("a", 1).await, f.image("b", 2).await);
    f.repo
        .save_message(f.message("first", &[&a]))
        .await
        .unwrap();
    let second = f
        .repo
        .save_message(f.message("second", &[&b]))
        .await
        .unwrap();
    f.repo
        .save_message(f.message("third", &[&a, &b]))
        .await
        .unwrap();

    f.repo.delete_message_and_subsequent(second).await.unwrap();

    let messages = f.repo.get_messages(f.conversation).await.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].images, vec![a]);
    assert_eq!(f.count("message_attachments").await, 1);
    assert_eq!(f.count("attachments").await, 2);
}

#[tokio::test]
async fn deleting_a_conversation_drops_every_link_of_its_messages() {
    let f = fixture().await;
    let a = f.image("a", 1).await;
    f.repo
        .save_message(f.message("first", &[&a]))
        .await
        .unwrap();
    f.repo
        .save_message(f.message("second", &[&a]))
        .await
        .unwrap();
    assert_eq!(f.count("message_attachments").await, 2);

    f.repo.delete_conversation(f.conversation).await.unwrap();

    assert_eq!(f.count("chat_messages").await, 0);
    assert_eq!(f.count("message_attachments").await, 0);
    assert_eq!(f.count("attachments").await, 1);
}
