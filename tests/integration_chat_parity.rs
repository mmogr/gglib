//! Chat parity integration tests.
//!
//! Verifies that chat history operations work correctly across the stack.

use gglib_core::domain::chat::{MessageRole, NewMessage};
use gglib_core::ports::chat_history::ChatHistoryRepository;
use gglib_db::repositories::SqliteChatHistoryRepository;
use gglib_db::setup::setup_test_database;

/// Test creating a conversation and listing it.
#[tokio::test]
async fn test_create_and_list_conversation() {
    let pool = setup_test_database()
        .await
        .expect("Failed to setup test db");
    let repo = SqliteChatHistoryRepository::new(pool);

    // Create a conversation (without model_id to avoid FK constraint)
    let conv_id = repo
        .create_conversation(gglib_core::domain::chat::NewConversation {
            title: "Test Chat".to_string(),
            model_id: None,
            system_prompt: Some("You are a helpful assistant.".to_string()),
        })
        .await
        .expect("Failed to create conversation");

    assert!(conv_id > 0, "Conversation ID should be positive");

    // List conversations
    let conversations = repo
        .list_conversations()
        .await
        .expect("Failed to list conversations");

    assert_eq!(
        conversations.len(),
        1,
        "Should have exactly one conversation"
    );
    assert_eq!(conversations[0].id, conv_id);
    assert_eq!(conversations[0].title, "Test Chat");
    assert_eq!(conversations[0].model_id, None);
    assert_eq!(
        conversations[0].system_prompt,
        Some("You are a helpful assistant.".to_string())
    );
}

/// Test saving messages and retrieving them.
#[tokio::test]
async fn test_save_and_list_messages() {
    let pool = setup_test_database()
        .await
        .expect("Failed to setup test db");
    let repo = SqliteChatHistoryRepository::new(pool);

    // Create a conversation first
    let conv_id = repo
        .create_conversation(gglib_core::domain::chat::NewConversation {
            title: "Message Test".to_string(),
            model_id: None,
            system_prompt: None,
        })
        .await
        .expect("Failed to create conversation");

    // Save a user message
    let user_msg_id = repo
        .save_message(NewMessage {
            conversation_id: conv_id,
            role: MessageRole::User,
            content: "Hello, how are you?".to_string(),
            metadata: None,
        })
        .await
        .expect("Failed to save user message");

    // Save an assistant message
    let assistant_msg_id = repo
        .save_message(NewMessage {
            conversation_id: conv_id,
            role: MessageRole::Assistant,
            content: "I'm doing well, thank you!".to_string(),
            metadata: None,
        })
        .await
        .expect("Failed to save assistant message");

    assert!(user_msg_id > 0, "User message ID should be positive");
    assert!(
        assistant_msg_id > user_msg_id,
        "Assistant message ID should be greater"
    );

    // Get messages
    let messages = repo
        .get_messages(conv_id)
        .await
        .expect("Failed to get messages");

    assert_eq!(messages.len(), 2, "Should have exactly two messages");

    // First message should be user
    assert_eq!(messages[0].id, user_msg_id);
    assert_eq!(messages[0].role, MessageRole::User);
    assert_eq!(messages[0].content, "Hello, how are you?");

    // Second message should be assistant
    assert_eq!(messages[1].id, assistant_msg_id);
    assert_eq!(messages[1].role, MessageRole::Assistant);
    assert_eq!(messages[1].content, "I'm doing well, thank you!");
}

/// Test conversation update.
#[tokio::test]
async fn test_update_conversation() {
    let pool = setup_test_database()
        .await
        .expect("Failed to setup test db");
    let repo = SqliteChatHistoryRepository::new(pool);

    // Create a conversation
    let conv_id = repo
        .create_conversation(gglib_core::domain::chat::NewConversation {
            title: "Original Title".to_string(),
            model_id: None,
            system_prompt: None,
        })
        .await
        .expect("Failed to create conversation");

    // Update title
    repo.update_conversation(
        conv_id,
        gglib_core::domain::chat::ConversationUpdate {
            title: Some("Updated Title".to_string()),
            system_prompt: None,
        },
    )
    .await
    .expect("Failed to update conversation");

    // Verify update
    let conv = repo
        .get_conversation(conv_id)
        .await
        .expect("Failed to get conversation")
        .expect("Conversation should exist");

    assert_eq!(conv.title, "Updated Title");
}

/// Test deleting a conversation removes all messages.
#[tokio::test]
async fn test_delete_conversation_cascades() {
    let pool = setup_test_database()
        .await
        .expect("Failed to setup test db");
    let repo = SqliteChatHistoryRepository::new(pool);

    // Create a conversation with messages
    let conv_id = repo
        .create_conversation(gglib_core::domain::chat::NewConversation {
            title: "To Delete".to_string(),
            model_id: None,
            system_prompt: None,
        })
        .await
        .expect("Failed to create conversation");

    repo.save_message(NewMessage {
        conversation_id: conv_id,
        role: MessageRole::User,
        content: "Test message".to_string(),
        metadata: None,
    })
    .await
    .expect("Failed to save message");

    // Delete conversation
    repo.delete_conversation(conv_id)
        .await
        .expect("Failed to delete conversation");

    // Verify conversation is gone
    let conv = repo
        .get_conversation(conv_id)
        .await
        .expect("Failed to query conversation");
    assert!(conv.is_none(), "Conversation should be deleted");

    // Verify messages are gone (should not error)
    let messages = repo
        .get_messages(conv_id)
        .await
        .expect("Failed to query messages");
    assert!(messages.is_empty(), "Messages should be deleted");
}
