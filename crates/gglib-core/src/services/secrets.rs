//! The one reveal of a stored secret (ADR 0027).

use std::sync::Arc;

use crate::ports::{
    CoreError, McpRepositoryError, McpServerRepository, RepositoryError, SettingsRepository,
};
use crate::secret::{SecretRef, SettingSecret, held};

/// Hands back a stored secret. Nothing else hands one back to a person or a
/// client: every other read says only whether it is held.
pub struct SecretsService {
    settings: Arc<dyn SettingsRepository>,
    mcp_servers: Arc<dyn McpServerRepository>,
}

impl SecretsService {
    /// A service over the stores the secrets are kept in.
    pub fn new(
        settings: Arc<dyn SettingsRepository>,
        mcp_servers: Arc<dyn McpServerRepository>,
    ) -> Self {
        Self {
            settings,
            mcp_servers,
        }
    }

    /// The secret `which` names, or `None` when nothing is stored there. A
    /// blank is nothing ([`held`]).
    ///
    /// # Errors
    ///
    /// When a store cannot be read, and not found for an MCP server that
    /// does not exist.
    pub async fn reveal(&self, which: &SecretRef) -> Result<Option<String>, CoreError> {
        let value = match which {
            SecretRef::Setting { secret } => {
                let settings = self.settings.load().await?;
                match secret {
                    SettingSecret::ProxyApiKey => settings.proxy_api_key,
                    SettingSecret::RemotePairingKey => settings.remote_pairing.map(|p| p.api_key),
                }
            }
            SecretRef::McpEnv { server_id, key } => self
                .mcp_servers
                .get_by_id(*server_id)
                .await
                .map_err(|e| match e {
                    McpRepositoryError::NotFound(what) => RepositoryError::NotFound(what),
                    McpRepositoryError::Internal(why) => RepositoryError::Storage(why),
                })?
                .env
                .into_iter()
                .find(|entry| entry.key == *key)
                .map(|entry| entry.value),
        };
        Ok(held(value.as_deref()).map(str::to_owned))
    }
}

#[cfg(test)]
#[path = "secrets_tests.rs"]
mod tests;
