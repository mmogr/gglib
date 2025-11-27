//! Unit tests for chat history service.
//!
//! Tests conversation and message CRUD operations, validation,
//! and database interactions.

#[path = "../../common/mod.rs"]
mod common;

use common::database::setup_test_pool;
use gglib::services::chat_history::{
    create_conversation, delete_conversation, get_conversation, get_conversation_count,
    get_conversations, get_message_count, get_messages, save_message, update_conversation,
};

/// Test creating a basic conversation
#[tokio::test]
async fn test_create_conversation() {
    let pool = setup_test_pool().await.unwrap();

    let id = create_conversation(&pool, "Test Chat".to_string(), None, None)
        .await
        .unwrap();

    assert!(id > 0);
}

/// Test creating a conversation with model_id
/// Note: model_id is a foreign key, so we need to create a model first
#[tokio::test]
async fn test_create_conversation_with_model() {
    let pool = setup_test_pool().await.unwrap();

    // First create a model to reference
    let model = gglib::models::Gguf::new(
        "test-model".to_string(),
        std::path::PathBuf::from("/test/model.gguf"),
        7.0,
        chrono::Utc::now(),
    );
    gglib::services::database::add_model(&pool, &model).await.unwrap();
    let models = gglib::services::database::list_models(&pool).await.unwrap();
    let model_id = models[0].id.unwrap() as i64;

    let id = create_conversation(&pool, "Chat with Model".to_string(), Some(model_id), None)
        .await
        .unwrap();

    let conv = get_conversation(&pool, id).await.unwrap().unwrap();
    assert_eq!(conv.model_id, Some(model_id));
}

/// Test creating a conversation with system prompt
#[tokio::test]
async fn test_create_conversation_with_system_prompt() {
    let pool = setup_test_pool().await.unwrap();

    let system_prompt = "You are a helpful assistant.".to_string();
    let id = create_conversation(
        &pool,
        "Chat with Prompt".to_string(),
        None,
        Some(system_prompt.clone()),
    )
    .await
    .unwrap();

    let conv = get_conversation(&pool, id).await.unwrap().unwrap();
    assert_eq!(conv.system_prompt, Some(system_prompt));
}

/// Test getting all conversations
#[tokio::test]
async fn test_get_conversations() {
    let pool = setup_test_pool().await.unwrap();

    // Create multiple conversations with delays to ensure different timestamps
    // SQLite datetime has second precision, so we need > 1s delays
    create_conversation(&pool, "First".to_string(), None, None)
        .await
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    create_conversation(&pool, "Second".to_string(), None, None)
        .await
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    create_conversation(&pool, "Third".to_string(), None, None)
        .await
        .unwrap();

    let conversations = get_conversations(&pool).await.unwrap();

    assert_eq!(conversations.len(), 3);
    // Should be ordered by updated_at DESC (most recent first)
    assert_eq!(conversations[0].title, "Third");
    assert_eq!(conversations[1].title, "Second");
    assert_eq!(conversations[2].title, "First");
}

/// Test getting empty conversations list
#[tokio::test]
async fn test_get_conversations_empty() {
    let pool = setup_test_pool().await.unwrap();

    let conversations = get_conversations(&pool).await.unwrap();
    assert!(conversations.is_empty());
}

/// Test getting a specific conversation by ID
#[tokio::test]
async fn test_get_conversation_by_id() {
    let pool = setup_test_pool().await.unwrap();

    let id = create_conversation(&pool, "Test".to_string(), None, Some("Prompt".to_string()))
        .await
        .unwrap();

    let conv = get_conversation(&pool, id).await.unwrap();
    assert!(conv.is_some());

    let conv = conv.unwrap();
    assert_eq!(conv.id, id);
    assert_eq!(conv.title, "Test");
    assert_eq!(conv.model_id, None);
    assert_eq!(conv.system_prompt, Some("Prompt".to_string()));
}

/// Test getting non-existent conversation
#[tokio::test]
async fn test_get_conversation_not_found() {
    let pool = setup_test_pool().await.unwrap();

    let conv = get_conversation(&pool, 999).await.unwrap();
    assert!(conv.is_none());
}

/// Test saving a user message
#[tokio::test]
async fn test_save_user_message() {
    let pool = setup_test_pool().await.unwrap();

    let conv_id = create_conversation(&pool, "Test".to_string(), None, None)
        .await
        .unwrap();

    let msg_id = save_message(&pool, conv_id, "user".to_string(), "Hello!".to_string())
        .await
        .unwrap();

    assert!(msg_id > 0);

    let messages = get_messages(&pool, conv_id).await.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role, "user");
    assert_eq!(messages[0].content, "Hello!");
}

/// Test saving an assistant message
#[tokio::test]
async fn test_save_assistant_message() {
    let pool = setup_test_pool().await.unwrap();

    let conv_id = create_conversation(&pool, "Test".to_string(), None, None)
        .await
        .unwrap();

    let msg_id = save_message(
        &pool,
        conv_id,
        "assistant".to_string(),
        "Hi there!".to_string(),
    )
    .await
    .unwrap();

    assert!(msg_id > 0);

    let messages = get_messages(&pool, conv_id).await.unwrap();
    assert_eq!(messages[0].role, "assistant");
}

/// Test saving a system message
#[tokio::test]
async fn test_save_system_message() {
    let pool = setup_test_pool().await.unwrap();

    let conv_id = create_conversation(&pool, "Test".to_string(), None, None)
        .await
        .unwrap();

    let msg_id = save_message(
        &pool,
        conv_id,
        "system".to_string(),
        "You are helpful.".to_string(),
    )
    .await
    .unwrap();

    assert!(msg_id > 0);
}

/// Test invalid role validation
#[tokio::test]
async fn test_save_message_invalid_role() {
    let pool = setup_test_pool().await.unwrap();

    let conv_id = create_conversation(&pool, "Test".to_string(), None, None)
        .await
        .unwrap();

    let result = save_message(&pool, conv_id, "invalid_role".to_string(), "Test".to_string()).await;

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid role: must be system, user, or assistant"));
}

/// Test message ordering (chronological)
#[tokio::test]
async fn test_messages_ordered_chronologically() {
    let pool = setup_test_pool().await.unwrap();

    let conv_id = create_conversation(&pool, "Test".to_string(), None, None)
        .await
        .unwrap();

    save_message(&pool, conv_id, "user".to_string(), "First".to_string())
        .await
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    save_message(
        &pool,
        conv_id,
        "assistant".to_string(),
        "Second".to_string(),
    )
    .await
    .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    save_message(&pool, conv_id, "user".to_string(), "Third".to_string())
        .await
        .unwrap();

    let messages = get_messages(&pool, conv_id).await.unwrap();

    assert_eq!(messages.len(), 3);
    // Should be ordered by created_at ASC (oldest first)
    assert_eq!(messages[0].content, "First");
    assert_eq!(messages[1].content, "Second");
    assert_eq!(messages[2].content, "Third");
}

/// Test conversation timestamp updates on message save
#[tokio::test]
async fn test_conversation_updated_at_on_message() {
    let pool = setup_test_pool().await.unwrap();

    let conv_id = create_conversation(&pool, "Test".to_string(), None, None)
        .await
        .unwrap();

    let conv_before = get_conversation(&pool, conv_id).await.unwrap().unwrap();
    let updated_before = conv_before.updated_at.clone();

    // SQLite datetime has second precision, so we need > 1s delay
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    save_message(&pool, conv_id, "user".to_string(), "Message".to_string())
        .await
        .unwrap();

    let conv_after = get_conversation(&pool, conv_id).await.unwrap().unwrap();

    // updated_at should have changed
    assert_ne!(conv_after.updated_at, updated_before);
}

/// Test updating conversation title
#[tokio::test]
async fn test_update_conversation_title() {
    let pool = setup_test_pool().await.unwrap();

    let conv_id = create_conversation(&pool, "Original".to_string(), None, None)
        .await
        .unwrap();

    update_conversation(&pool, conv_id, Some("Updated Title".to_string()), None)
        .await
        .unwrap();

    let conv = get_conversation(&pool, conv_id).await.unwrap().unwrap();
    assert_eq!(conv.title, "Updated Title");
}

/// Test updating conversation system prompt
#[tokio::test]
async fn test_update_conversation_system_prompt() {
    let pool = setup_test_pool().await.unwrap();

    let conv_id = create_conversation(&pool, "Test".to_string(), None, None)
        .await
        .unwrap();

    update_conversation(
        &pool,
        conv_id,
        None,
        Some(Some("New system prompt".to_string())),
    )
    .await
    .unwrap();

    let conv = get_conversation(&pool, conv_id).await.unwrap().unwrap();
    assert_eq!(conv.system_prompt, Some("New system prompt".to_string()));
}

/// Test clearing conversation system prompt
#[tokio::test]
async fn test_clear_conversation_system_prompt() {
    let pool = setup_test_pool().await.unwrap();

    let conv_id = create_conversation(
        &pool,
        "Test".to_string(),
        None,
        Some("Initial prompt".to_string()),
    )
    .await
    .unwrap();

    update_conversation(&pool, conv_id, None, Some(None))
        .await
        .unwrap();

    let conv = get_conversation(&pool, conv_id).await.unwrap().unwrap();
    assert_eq!(conv.system_prompt, None);
}

/// Test update with no changes does nothing
#[tokio::test]
async fn test_update_conversation_no_changes() {
    let pool = setup_test_pool().await.unwrap();

    let conv_id = create_conversation(&pool, "Test".to_string(), None, None)
        .await
        .unwrap();

    // This should succeed without error
    update_conversation(&pool, conv_id, None, None)
        .await
        .unwrap();

    let conv = get_conversation(&pool, conv_id).await.unwrap().unwrap();
    assert_eq!(conv.title, "Test");
}

/// Test deleting a conversation
#[tokio::test]
async fn test_delete_conversation() {
    let pool = setup_test_pool().await.unwrap();

    let conv_id = create_conversation(&pool, "To Delete".to_string(), None, None)
        .await
        .unwrap();

    // Add some messages
    save_message(&pool, conv_id, "user".to_string(), "Message 1".to_string())
        .await
        .unwrap();
    save_message(
        &pool,
        conv_id,
        "assistant".to_string(),
        "Message 2".to_string(),
    )
    .await
    .unwrap();

    delete_conversation(&pool, conv_id).await.unwrap();

    // Conversation should be gone
    let conv = get_conversation(&pool, conv_id).await.unwrap();
    assert!(conv.is_none());

    // Messages should also be deleted (cascade)
    let messages = get_messages(&pool, conv_id).await.unwrap();
    assert!(messages.is_empty());
}

/// Test deleting non-existent conversation (should not error)
#[tokio::test]
async fn test_delete_nonexistent_conversation() {
    let pool = setup_test_pool().await.unwrap();

    // Should not error even though ID doesn't exist
    let result = delete_conversation(&pool, 999).await;
    assert!(result.is_ok());
}

/// Test conversation count
#[tokio::test]
async fn test_conversation_count() {
    let pool = setup_test_pool().await.unwrap();

    assert_eq!(get_conversation_count(&pool).await.unwrap(), 0);

    create_conversation(&pool, "One".to_string(), None, None)
        .await
        .unwrap();
    assert_eq!(get_conversation_count(&pool).await.unwrap(), 1);

    create_conversation(&pool, "Two".to_string(), None, None)
        .await
        .unwrap();
    assert_eq!(get_conversation_count(&pool).await.unwrap(), 2);
}

/// Test message count for conversation
#[tokio::test]
async fn test_message_count() {
    let pool = setup_test_pool().await.unwrap();

    let conv_id = create_conversation(&pool, "Test".to_string(), None, None)
        .await
        .unwrap();

    assert_eq!(get_message_count(&pool, conv_id).await.unwrap(), 0);

    save_message(&pool, conv_id, "user".to_string(), "One".to_string())
        .await
        .unwrap();
    assert_eq!(get_message_count(&pool, conv_id).await.unwrap(), 1);

    save_message(&pool, conv_id, "assistant".to_string(), "Two".to_string())
        .await
        .unwrap();
    assert_eq!(get_message_count(&pool, conv_id).await.unwrap(), 2);
}

/// Test message count for non-existent conversation
#[tokio::test]
async fn test_message_count_empty() {
    let pool = setup_test_pool().await.unwrap();

    let count = get_message_count(&pool, 999).await.unwrap();
    assert_eq!(count, 0);
}

/// Test unicode content in messages
#[tokio::test]
async fn test_unicode_message_content() {
    let pool = setup_test_pool().await.unwrap();

    let conv_id = create_conversation(&pool, "Unicode Test".to_string(), None, None)
        .await
        .unwrap();

    let content = "Hello! 你好 🦙 émojis работает 日本語".to_string();
    save_message(&pool, conv_id, "user".to_string(), content.clone())
        .await
        .unwrap();

    let messages = get_messages(&pool, conv_id).await.unwrap();
    assert_eq!(messages[0].content, content);
}

/// Test unicode in conversation title
#[tokio::test]
async fn test_unicode_conversation_title() {
    let pool = setup_test_pool().await.unwrap();

    let title = "Chat about 日本語 and émojis 🦙".to_string();
    let conv_id = create_conversation(&pool, title.clone(), None, None)
        .await
        .unwrap();

    let conv = get_conversation(&pool, conv_id).await.unwrap().unwrap();
    assert_eq!(conv.title, title);
}

/// Test empty message content
#[tokio::test]
async fn test_empty_message_content() {
    let pool = setup_test_pool().await.unwrap();

    let conv_id = create_conversation(&pool, "Test".to_string(), None, None)
        .await
        .unwrap();

    let msg_id = save_message(&pool, conv_id, "user".to_string(), "".to_string())
        .await
        .unwrap();

    assert!(msg_id > 0);

    let messages = get_messages(&pool, conv_id).await.unwrap();
    assert_eq!(messages[0].content, "");
}

/// Test very long message content
#[tokio::test]
async fn test_long_message_content() {
    let pool = setup_test_pool().await.unwrap();

    let conv_id = create_conversation(&pool, "Test".to_string(), None, None)
        .await
        .unwrap();

    let long_content = "a".repeat(100_000); // 100KB message
    save_message(&pool, conv_id, "user".to_string(), long_content.clone())
        .await
        .unwrap();

    let messages = get_messages(&pool, conv_id).await.unwrap();
    assert_eq!(messages[0].content.len(), 100_000);
}

/// Test concurrent message saves
#[tokio::test]
async fn test_concurrent_message_saves() {
    let pool = setup_test_pool().await.unwrap();

    let conv_id = create_conversation(&pool, "Concurrent".to_string(), None, None)
        .await
        .unwrap();

    let mut handles = vec![];

    for i in 0..10 {
        let pool_clone = pool.clone();
        let handle = tokio::spawn(async move {
            save_message(
                &pool_clone,
                conv_id,
                "user".to_string(),
                format!("Message {}", i),
            )
            .await
        });
        handles.push(handle);
    }

    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }

    let messages = get_messages(&pool, conv_id).await.unwrap();
    assert_eq!(messages.len(), 10);
}

/// Test multiple conversations with messages
#[tokio::test]
async fn test_multiple_conversations_isolation() {
    let pool = setup_test_pool().await.unwrap();

    let conv1 = create_conversation(&pool, "Conv 1".to_string(), None, None)
        .await
        .unwrap();
    let conv2 = create_conversation(&pool, "Conv 2".to_string(), None, None)
        .await
        .unwrap();

    save_message(&pool, conv1, "user".to_string(), "In conv 1".to_string())
        .await
        .unwrap();
    save_message(&pool, conv2, "user".to_string(), "In conv 2".to_string())
        .await
        .unwrap();
    save_message(
        &pool,
        conv2,
        "assistant".to_string(),
        "Also in conv 2".to_string(),
    )
    .await
    .unwrap();

    let messages1 = get_messages(&pool, conv1).await.unwrap();
    let messages2 = get_messages(&pool, conv2).await.unwrap();

    assert_eq!(messages1.len(), 1);
    assert_eq!(messages2.len(), 2);
    assert_eq!(messages1[0].content, "In conv 1");
}
