//! Unit tests for chat history repository.
//!
//! Tests the `SqliteChatHistoryRepository` implementation through the
//! `ChatHistoryRepository` trait interface.

use crate::common::database::setup_test_pool;
use gglib_core::domain::chat::{ConversationUpdate, MessageRole, NewConversation, NewMessage};
use gglib_core::ports::ChatHistoryRepository;
use gglib_db::SqliteChatHistoryRepository;

/// Test creating a basic conversation
#[tokio::test]
async fn test_create_conversation() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteChatHistoryRepository::new(pool);

    let id = repo
        .create_conversation(NewConversation {
            title: "Test Chat".to_string(),
            model_id: None,
            system_prompt: None,
        })
        .await
        .unwrap();

    assert!(id > 0);
}

/// Test creating a conversation with system prompt
#[tokio::test]
async fn test_create_conversation_with_system_prompt() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteChatHistoryRepository::new(pool);

    let system_prompt = "You are a helpful assistant.".to_string();
    let id = repo
        .create_conversation(NewConversation {
            title: "Chat with Prompt".to_string(),
            model_id: None,
            system_prompt: Some(system_prompt.clone()),
        })
        .await
        .unwrap();

    let conv = repo.get_conversation(id).await.unwrap().unwrap();
    assert_eq!(conv.system_prompt, Some(system_prompt));
}

/// Test listing all conversations
#[tokio::test]
async fn test_list_conversations() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteChatHistoryRepository::new(pool);

    // Create multiple conversations with delays to ensure different timestamps
    repo.create_conversation(NewConversation {
        title: "First".to_string(),
        model_id: None,
        system_prompt: None,
    })
    .await
    .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    repo.create_conversation(NewConversation {
        title: "Second".to_string(),
        model_id: None,
        system_prompt: None,
    })
    .await
    .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    repo.create_conversation(NewConversation {
        title: "Third".to_string(),
        model_id: None,
        system_prompt: None,
    })
    .await
    .unwrap();

    let conversations = repo.list_conversations().await.unwrap();

    assert_eq!(conversations.len(), 3);
    // Should be ordered by updated_at DESC (most recent first)
    assert_eq!(conversations[0].title, "Third");
    assert_eq!(conversations[1].title, "Second");
    assert_eq!(conversations[2].title, "First");
}

/// Test listing empty conversations list
#[tokio::test]
async fn test_list_conversations_empty() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteChatHistoryRepository::new(pool);

    let conversations = repo.list_conversations().await.unwrap();
    assert!(conversations.is_empty());
}

/// Test getting a specific conversation by ID
#[tokio::test]
async fn test_get_conversation_by_id() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteChatHistoryRepository::new(pool);

    let id = repo
        .create_conversation(NewConversation {
            title: "Test".to_string(),
            model_id: None,
            system_prompt: Some("Prompt".to_string()),
        })
        .await
        .unwrap();

    let conv = repo.get_conversation(id).await.unwrap();
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
    let repo = SqliteChatHistoryRepository::new(pool);

    let conv = repo.get_conversation(999).await.unwrap();
    assert!(conv.is_none());
}

/// Test saving a user message
#[tokio::test]
async fn test_save_user_message() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteChatHistoryRepository::new(pool);

    let conv_id = repo
        .create_conversation(NewConversation {
            title: "Test".to_string(),
            model_id: None,
            system_prompt: None,
        })
        .await
        .unwrap();

    let msg_id = repo
        .save_message(NewMessage {
            conversation_id: conv_id,
            role: MessageRole::User,
            content: "Hello!".to_string(),
            metadata: None,
        })
        .await
        .unwrap();

    assert!(msg_id > 0);

    let messages = repo.get_messages(conv_id).await.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role, MessageRole::User);
    assert_eq!(messages[0].content, "Hello!");
}

/// Test saving an assistant message
#[tokio::test]
async fn test_save_assistant_message() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteChatHistoryRepository::new(pool);

    let conv_id = repo
        .create_conversation(NewConversation {
            title: "Test".to_string(),
            model_id: None,
            system_prompt: None,
        })
        .await
        .unwrap();

    let msg_id = repo
        .save_message(NewMessage {
            conversation_id: conv_id,
            role: MessageRole::Assistant,
            content: "Hi there!".to_string(),
            metadata: None,
        })
        .await
        .unwrap();

    assert!(msg_id > 0);

    let messages = repo.get_messages(conv_id).await.unwrap();
    assert_eq!(messages[0].role, MessageRole::Assistant);
}

/// Test saving a system message
#[tokio::test]
async fn test_save_system_message() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteChatHistoryRepository::new(pool);

    let conv_id = repo
        .create_conversation(NewConversation {
            title: "Test".to_string(),
            model_id: None,
            system_prompt: None,
        })
        .await
        .unwrap();

    let msg_id = repo
        .save_message(NewMessage {
            conversation_id: conv_id,
            role: MessageRole::System,
            content: "You are helpful.".to_string(),
            metadata: None,
        })
        .await
        .unwrap();

    assert!(msg_id > 0);
}

/// Test getting messages for a conversation
#[tokio::test]
async fn test_get_messages_ordering() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteChatHistoryRepository::new(pool);

    let conv_id = repo
        .create_conversation(NewConversation {
            title: "Test".to_string(),
            model_id: None,
            system_prompt: None,
        })
        .await
        .unwrap();

    repo.save_message(NewMessage {
        conversation_id: conv_id,
        role: MessageRole::User,
        content: "First".to_string(),
        metadata: None,
    })
    .await
    .unwrap();

    repo.save_message(NewMessage {
        conversation_id: conv_id,
        role: MessageRole::Assistant,
        content: "Second".to_string(),
        metadata: None,
    })
    .await
    .unwrap();

    repo.save_message(NewMessage {
        conversation_id: conv_id,
        role: MessageRole::User,
        content: "Third".to_string(),
        metadata: None,
    })
    .await
    .unwrap();

    let messages = repo.get_messages(conv_id).await.unwrap();

    // Should be chronological order
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].content, "First");
    assert_eq!(messages[1].content, "Second");
    assert_eq!(messages[2].content, "Third");
}

/// Test getting messages from non-existent conversation
#[tokio::test]
async fn test_get_messages_empty() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteChatHistoryRepository::new(pool);

    let conv_id = repo
        .create_conversation(NewConversation {
            title: "Empty".to_string(),
            model_id: None,
            system_prompt: None,
        })
        .await
        .unwrap();

    let messages = repo.get_messages(conv_id).await.unwrap();
    assert!(messages.is_empty());
}

/// Test updating a conversation's title
#[tokio::test]
async fn test_update_conversation_title() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteChatHistoryRepository::new(pool);

    let id = repo
        .create_conversation(NewConversation {
            title: "Original".to_string(),
            model_id: None,
            system_prompt: None,
        })
        .await
        .unwrap();

    repo.update_conversation(
        id,
        ConversationUpdate {
            title: Some("Updated".to_string()),
            system_prompt: None,
        },
    )
    .await
    .unwrap();

    let conv = repo.get_conversation(id).await.unwrap().unwrap();
    assert_eq!(conv.title, "Updated");
}

/// Test updating a conversation's system prompt
#[tokio::test]
async fn test_update_conversation_system_prompt() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteChatHistoryRepository::new(pool);

    let id = repo
        .create_conversation(NewConversation {
            title: "Test".to_string(),
            model_id: None,
            system_prompt: None,
        })
        .await
        .unwrap();

    repo.update_conversation(
        id,
        ConversationUpdate {
            title: None,
            system_prompt: Some(Some("New prompt".to_string())),
        },
    )
    .await
    .unwrap();

    let conv = repo.get_conversation(id).await.unwrap().unwrap();
    assert_eq!(conv.system_prompt, Some("New prompt".to_string()));
}

/// Test clearing system prompt
#[tokio::test]
async fn test_clear_system_prompt() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteChatHistoryRepository::new(pool);

    let id = repo
        .create_conversation(NewConversation {
            title: "Test".to_string(),
            model_id: None,
            system_prompt: Some("Initial prompt".to_string()),
        })
        .await
        .unwrap();

    repo.update_conversation(
        id,
        ConversationUpdate {
            title: None,
            system_prompt: Some(None), // Clear the prompt
        },
    )
    .await
    .unwrap();

    let conv = repo.get_conversation(id).await.unwrap().unwrap();
    assert_eq!(conv.system_prompt, None);
}

/// Test deleting a conversation
#[tokio::test]
async fn test_delete_conversation() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteChatHistoryRepository::new(pool);

    let id = repo
        .create_conversation(NewConversation {
            title: "To Delete".to_string(),
            model_id: None,
            system_prompt: None,
        })
        .await
        .unwrap();

    repo.delete_conversation(id).await.unwrap();

    let conv = repo.get_conversation(id).await.unwrap();
    assert!(conv.is_none());
}

/// Test deleting conversation removes its messages
#[tokio::test]
async fn test_delete_conversation_cascades_messages() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteChatHistoryRepository::new(pool);

    let id = repo
        .create_conversation(NewConversation {
            title: "Test".to_string(),
            model_id: None,
            system_prompt: None,
        })
        .await
        .unwrap();

    repo.save_message(NewMessage {
        conversation_id: id,
        role: MessageRole::User,
        content: "Hello".to_string(),
        metadata: None,
    })
    .await
    .unwrap();

    repo.delete_conversation(id).await.unwrap();

    // Messages should be deleted with the conversation
    let messages = repo.get_messages(id).await.unwrap();
    assert!(messages.is_empty());
}

/// Test getting conversation count
#[tokio::test]
async fn test_conversation_count() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteChatHistoryRepository::new(pool);

    assert_eq!(repo.get_conversation_count().await.unwrap(), 0);

    repo.create_conversation(NewConversation {
        title: "First".to_string(),
        model_id: None,
        system_prompt: None,
    })
    .await
    .unwrap();

    assert_eq!(repo.get_conversation_count().await.unwrap(), 1);

    repo.create_conversation(NewConversation {
        title: "Second".to_string(),
        model_id: None,
        system_prompt: None,
    })
    .await
    .unwrap();

    assert_eq!(repo.get_conversation_count().await.unwrap(), 2);
}

/// Test getting message count
#[tokio::test]
async fn test_message_count() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteChatHistoryRepository::new(pool);

    let conv_id = repo
        .create_conversation(NewConversation {
            title: "Test".to_string(),
            model_id: None,
            system_prompt: None,
        })
        .await
        .unwrap();

    assert_eq!(repo.get_message_count(conv_id).await.unwrap(), 0);

    repo.save_message(NewMessage {
        conversation_id: conv_id,
        role: MessageRole::User,
        content: "Hello".to_string(),
        metadata: None,
    })
    .await
    .unwrap();

    assert_eq!(repo.get_message_count(conv_id).await.unwrap(), 1);

    repo.save_message(NewMessage {
        conversation_id: conv_id,
        role: MessageRole::Assistant,
        content: "Hi".to_string(),
        metadata: None,
    })
    .await
    .unwrap();

    assert_eq!(repo.get_message_count(conv_id).await.unwrap(), 2);
}

/// Test updating message content
#[tokio::test]
async fn test_update_message_content() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteChatHistoryRepository::new(pool);

    let conv_id = repo
        .create_conversation(NewConversation {
            title: "Test".to_string(),
            model_id: None,
            system_prompt: None,
        })
        .await
        .unwrap();

    let msg_id = repo
        .save_message(NewMessage {
            conversation_id: conv_id,
            role: MessageRole::User,
            content: "Original".to_string(),
            metadata: None,
        })
        .await
        .unwrap();

    repo.update_message(msg_id, "Updated".to_string(), None)
        .await
        .unwrap();

    let messages = repo.get_messages(conv_id).await.unwrap();
    assert_eq!(messages[0].content, "Updated");
}

/// Test deleting a message and subsequent messages
#[tokio::test]
async fn test_delete_message_and_subsequent() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteChatHistoryRepository::new(pool);

    let conv_id = repo
        .create_conversation(NewConversation {
            title: "Test".to_string(),
            model_id: None,
            system_prompt: None,
        })
        .await
        .unwrap();

    repo.save_message(NewMessage {
        conversation_id: conv_id,
        role: MessageRole::User,
        content: "First".to_string(),
        metadata: None,
    })
    .await
    .unwrap();

    let msg2_id = repo
        .save_message(NewMessage {
            conversation_id: conv_id,
            role: MessageRole::Assistant,
            content: "Second".to_string(),
            metadata: None,
        })
        .await
        .unwrap();

    repo.save_message(NewMessage {
        conversation_id: conv_id,
        role: MessageRole::User,
        content: "Third".to_string(),
        metadata: None,
    })
    .await
    .unwrap();

    // Delete from second message onward
    let deleted = repo.delete_message_and_subsequent(msg2_id).await.unwrap();
    assert_eq!(deleted, 2); // Second and Third

    let messages = repo.get_messages(conv_id).await.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "First");
}

/// Test multiple conversations isolation
#[tokio::test]
async fn test_multiple_conversations_isolation() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteChatHistoryRepository::new(pool);

    let conv1 = repo
        .create_conversation(NewConversation {
            title: "Conv 1".to_string(),
            model_id: None,
            system_prompt: None,
        })
        .await
        .unwrap();

    let conv2 = repo
        .create_conversation(NewConversation {
            title: "Conv 2".to_string(),
            model_id: None,
            system_prompt: None,
        })
        .await
        .unwrap();

    repo.save_message(NewMessage {
        conversation_id: conv1,
        role: MessageRole::User,
        content: "In conv 1".to_string(),
        metadata: None,
    })
    .await
    .unwrap();

    repo.save_message(NewMessage {
        conversation_id: conv2,
        role: MessageRole::User,
        content: "In conv 2".to_string(),
        metadata: None,
    })
    .await
    .unwrap();

    repo.save_message(NewMessage {
        conversation_id: conv2,
        role: MessageRole::Assistant,
        content: "Also in conv 2".to_string(),
        metadata: None,
    })
    .await
    .unwrap();

    let messages1 = repo.get_messages(conv1).await.unwrap();
    let messages2 = repo.get_messages(conv2).await.unwrap();

    assert_eq!(messages1.len(), 1);
    assert_eq!(messages2.len(), 2);
    assert_eq!(messages1[0].content, "In conv 1");
}
