//! `AppCore` - the primary application facade.
//!
//! This is the composition root for core services. Adapters (CLI, GUI, Web)
//! receive an `AppCore` instance and use it to access all functionality.

use crate::ports::Repos;
use std::sync::Arc;

use super::{ChatHistoryService, ModelService, ModelVerificationService, SettingsService};

/// The core application facade.
///
/// `AppCore` provides access to all core services. It's constructed at the
/// adapter's composition root (main.rs or bootstrap.rs) with concrete
/// implementations of repositories.
///
/// # Example
///
/// ```ignore
/// let repos = Repos { models: model_repo, settings: settings_repo };
/// let core = AppCore::new(repos);
///
/// // Access services
/// let models = core.models().list().await?;
/// ```
pub struct AppCore {
    models: ModelService,
    settings: SettingsService,
    chat_history: ChatHistoryService,
    verification: Option<Arc<ModelVerificationService>>,
}

impl AppCore {
    /// Create a new `AppCore` with the given repositories.
    pub fn new(repos: Repos) -> Self {
        Self {
            models: ModelService::new(repos.models),
            settings: SettingsService::new(repos.settings),
            chat_history: ChatHistoryService::new(repos.chat_history),
            verification: None,
        }
    }

    /// Set the verification service (optional).
    ///
    /// This should be called during bootstrap if verification features are needed.
    #[must_use]
    pub fn with_verification(mut self, verification: Arc<ModelVerificationService>) -> Self {
        self.verification = Some(verification);
        self
    }

    /// Access the model service.
    pub const fn models(&self) -> &ModelService {
        &self.models
    }

    /// Access the settings service.
    pub const fn settings(&self) -> &SettingsService {
        &self.settings
    }

    /// Access the chat history service.
    pub const fn chat_history(&self) -> &ChatHistoryService {
        &self.chat_history
    }

    /// Access the verification service (if available).
    pub fn verification(&self) -> Option<&ModelVerificationService> {
        self.verification.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::chat::{
        Conversation, ConversationUpdate, Message, NewConversation, NewMessage,
    };
    use crate::domain::mcp::{McpServer, NewMcpServer};
    use crate::domain::{Model, NewModel};
    use crate::ports::{
        ChatHistoryError, ChatHistoryRepository, McpRepositoryError, McpServerRepository,
        ModelRepository, RepositoryError, SettingsRepository,
    };
    use crate::settings::Settings;
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct MockModelRepo;

    #[async_trait]
    impl ModelRepository for MockModelRepo {
        async fn list(&self) -> Result<Vec<Model>, RepositoryError> {
            Ok(vec![])
        }
        async fn get_by_id(&self, id: i64) -> Result<Model, RepositoryError> {
            Err(RepositoryError::NotFound(format!("id={id}")))
        }
        async fn get_by_name(&self, name: &str) -> Result<Model, RepositoryError> {
            Err(RepositoryError::NotFound(format!("name={name}")))
        }
        async fn find_by_path(
            &self,
            _path: &std::path::Path,
        ) -> Result<Option<Model>, RepositoryError> {
            // `unimplemented!()` like its siblings, not `Ok(None)`. `Ok(None)`
            // reads as "no duplicate found", which is a specific and wrong
            // answer for a double that stores nothing.
            unimplemented!()
        }
        async fn insert(&self, _model: &NewModel) -> Result<Model, RepositoryError> {
            unimplemented!()
        }
        async fn update(&self, _model: &Model) -> Result<(), RepositoryError> {
            unimplemented!()
        }
        async fn delete(&self, _id: i64) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

    struct MockMcpRepo;

    #[async_trait]
    impl McpServerRepository for MockMcpRepo {
        async fn insert(&self, _server: NewMcpServer) -> Result<McpServer, McpRepositoryError> {
            unimplemented!()
        }
        async fn get_by_id(&self, id: i64) -> Result<McpServer, McpRepositoryError> {
            Err(McpRepositoryError::NotFound(format!("id={id}")))
        }
        async fn get_by_name(&self, name: &str) -> Result<McpServer, McpRepositoryError> {
            Err(McpRepositoryError::NotFound(format!("name={name}")))
        }
        async fn list(&self) -> Result<Vec<McpServer>, McpRepositoryError> {
            Ok(vec![])
        }
        async fn update(&self, _server: &McpServer) -> Result<(), McpRepositoryError> {
            unimplemented!()
        }
        async fn delete(&self, _id: i64) -> Result<(), McpRepositoryError> {
            Ok(())
        }
        async fn update_last_connected(&self, _id: i64) -> Result<(), McpRepositoryError> {
            Ok(())
        }
    }

    struct MockChatHistoryRepo;

    #[async_trait]
    impl ChatHistoryRepository for MockChatHistoryRepo {
        async fn create_conversation(
            &self,
            _conv: NewConversation,
        ) -> Result<i64, ChatHistoryError> {
            Ok(1)
        }
        async fn list_conversations(&self) -> Result<Vec<Conversation>, ChatHistoryError> {
            Ok(vec![])
        }
        async fn get_conversation(
            &self,
            _id: i64,
        ) -> Result<Option<Conversation>, ChatHistoryError> {
            Ok(None)
        }
        async fn update_conversation(
            &self,
            _id: i64,
            _update: ConversationUpdate,
        ) -> Result<(), ChatHistoryError> {
            Ok(())
        }
        async fn delete_conversation(&self, _id: i64) -> Result<(), ChatHistoryError> {
            Ok(())
        }
        async fn get_conversation_count(&self) -> Result<i64, ChatHistoryError> {
            Ok(0)
        }
        async fn get_messages(
            &self,
            _conversation_id: i64,
        ) -> Result<Vec<Message>, ChatHistoryError> {
            Ok(vec![])
        }
        async fn save_message(&self, _msg: NewMessage) -> Result<i64, ChatHistoryError> {
            Ok(1)
        }
        async fn update_message(
            &self,
            _id: i64,
            _content: String,
            _metadata: Option<serde_json::Value>,
        ) -> Result<(), ChatHistoryError> {
            Ok(())
        }
        async fn delete_message_and_subsequent(&self, _id: i64) -> Result<i64, ChatHistoryError> {
            Ok(0)
        }
        async fn get_message_count(&self, _conversation_id: i64) -> Result<i64, ChatHistoryError> {
            Ok(0)
        }
    }

    struct MockSettingsRepo {
        settings: Mutex<Settings>,
    }

    impl MockSettingsRepo {
        fn new() -> Self {
            Self {
                settings: Mutex::new(Settings::with_defaults()),
            }
        }
    }

    #[async_trait]
    impl SettingsRepository for MockSettingsRepo {
        async fn load(&self) -> Result<Settings, RepositoryError> {
            Ok(self.settings.lock().unwrap().clone())
        }
        async fn save(&self, settings: &Settings) -> Result<(), RepositoryError> {
            *self.settings.lock().unwrap() = settings.clone();
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_app_core_creation() {
        let repos = Repos {
            models: Arc::new(MockModelRepo),
            settings: Arc::new(MockSettingsRepo::new()),
            mcp_servers: Arc::new(MockMcpRepo),
            chat_history: Arc::new(MockChatHistoryRepo),
        };

        let core = AppCore::new(repos);

        // Verify services are accessible
        let models = core.models().list().await.unwrap();
        assert!(models.is_empty());

        let settings = core.settings().get().await.unwrap();
        assert_eq!(settings.default_context_size, Some(4096));
    }
}
