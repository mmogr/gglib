//! `AppCore` - the primary application facade.
//!
//! This is the composition root for core services. Adapters (CLI, GUI, Web)
//! receive an `AppCore` instance and use it to access all functionality.

use crate::ports::{DownloadManagerPort, HfClientPort, Repos};
use std::sync::Arc;

use super::{
    AttachmentService, ChatHistoryService, ModelService, ModelVerificationService, SettingsService,
};

/// The core application facade.
///
/// `AppCore` provides access to all core services. It's constructed at the
/// adapter's composition root (main.rs or bootstrap.rs) with concrete
/// implementations of repositories.
///
/// # Example
///
/// ```ignore
/// let repos = Repos::new(models, model_files, settings, mcp_servers, chat_history, attachments);
/// let core = AppCore::new(repos, hf_client, downloads);
///
/// // Access services
/// let models = core.models().list().await?;
/// ```
pub struct AppCore {
    models: ModelService,
    settings: SettingsService,
    chat_history: ChatHistoryService,
    attachments: AttachmentService,
    verification: ModelVerificationService,
    hf_token: Option<String>,
}

impl AppCore {
    /// Create a new `AppCore` with the given repositories. Its verification
    /// service asks `hf_client` for updates, and a repair queues its download
    /// on `downloads`.
    pub fn new(
        repos: Repos,
        hf_client: Arc<dyn HfClientPort>,
        downloads: Arc<dyn DownloadManagerPort>,
    ) -> Self {
        Self {
            verification: ModelVerificationService::new(
                Arc::clone(&repos.models),
                repos.model_files,
                hf_client,
                downloads,
            ),
            settings: SettingsService::new(repos.settings).with_models(Arc::clone(&repos.models)),
            models: ModelService::new(repos.models),
            chat_history: ChatHistoryService::new(repos.chat_history),
            attachments: AttachmentService::new(repos.attachments),
            hf_token: None,
        }
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

    /// Access the attachment service.
    pub const fn attachments(&self) -> &AttachmentService {
        &self.attachments
    }

    /// Access the verification service.
    pub const fn verification(&self) -> &ModelVerificationService {
        &self.verification
    }

    /// This core, holding the token its process asks the Hub with.
    #[must_use]
    pub fn with_hf_token(mut self, token: Option<String>) -> Self {
        self.hf_token = token;
        self
    }

    /// The Hub token, for a request to `HuggingFace` that this core's own
    /// Hub client does not make: an upgrade's check and its download. `None`
    /// asks as nobody.
    #[must_use]
    pub fn hf_token(&self) -> Option<String> {
        self.hf_token.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::super::model_verification_remote::tests::Rows;
    use super::*;
    use crate::domain::chat::{
        Conversation, ConversationUpdate, Message, NewConversation, NewMessage,
    };
    use crate::domain::mcp::{McpServer, NewMcpServer};
    use crate::domain::{AttachmentBlob, AttachmentId, AttachmentInfo, Model, NewModel};
    use crate::ports::{
        AttachmentError, AttachmentStore, ChatHistoryError, ChatHistoryRepository,
        InMemorySettings, McpRepositoryError, McpServerRepository, ModelRepository,
        RepositoryError,
    };
    use async_trait::async_trait;

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
        async fn save_messages(&self, _msgs: Vec<NewMessage>) -> Result<(), ChatHistoryError> {
            Ok(())
        }
        async fn replace_from(
            &self,
            _from: i64,
            _msg: NewMessage,
        ) -> Result<i64, ChatHistoryError> {
            Ok(0)
        }
        async fn conversation_of_message(&self, _id: i64) -> Result<Option<i64>, ChatHistoryError> {
            Ok(None)
        }
        async fn delete_message_and_subsequent(&self, _id: i64) -> Result<i64, ChatHistoryError> {
            Ok(0)
        }
        async fn get_message_count(&self, _conversation_id: i64) -> Result<i64, ChatHistoryError> {
            Ok(0)
        }
        async fn fork(
            &self,
            _source: i64,
            _through: Option<i64>,
            _then: Option<NewMessage>,
        ) -> Result<i64, ChatHistoryError> {
            Ok(1)
        }
        async fn lineage(
            &self,
            _conversation_id: i64,
        ) -> Result<Vec<crate::domain::branching::LineChat>, ChatHistoryError> {
            Ok(vec![])
        }
    }

    struct MockAttachmentStore;

    #[async_trait]
    impl AttachmentStore for MockAttachmentStore {
        async fn put(&self, _info: &AttachmentInfo, _bytes: &[u8]) -> Result<(), AttachmentError> {
            Ok(())
        }
        async fn info(
            &self,
            _id: &AttachmentId,
        ) -> Result<Option<AttachmentInfo>, AttachmentError> {
            Ok(None)
        }
        async fn size(&self, _id: &AttachmentId) -> Result<Option<usize>, AttachmentError> {
            Ok(None)
        }
        async fn blob(
            &self,
            _id: &AttachmentId,
        ) -> Result<Option<AttachmentBlob>, AttachmentError> {
            Ok(None)
        }
        async fn ids_starting_with(
            &self,
            _prefix: &str,
        ) -> Result<Vec<AttachmentId>, AttachmentError> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn test_app_core_creation() {
        let repos = Repos {
            models: Arc::new(MockModelRepo),
            model_files: Arc::new(Rows(Vec::new())),
            settings: Arc::new(InMemorySettings::default()),
            mcp_servers: Arc::new(MockMcpRepo),
            chat_history: Arc::new(MockChatHistoryRepo),
            attachments: Arc::new(MockAttachmentStore),
        };

        let core = AppCore::bare(repos);

        // Verify services are accessible
        let models = core.models().list().await.unwrap();
        assert!(models.is_empty());

        let settings = core.settings().get().await.unwrap();
        // Unset, not the floor: nothing has chosen a global default here.
        assert_eq!(settings.default_context_size, None);
    }
}
