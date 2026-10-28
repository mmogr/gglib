//! What [`SecretsService::reveal`] hands back, from each store.

use std::sync::Arc;

use async_trait::async_trait;

use super::SecretsService;
use crate::domain::mcp::{McpEnvEntry, McpServer, NewMcpServer};
use crate::ports::{CoreError, InMemorySettings, McpRepositoryError, McpServerRepository};
use crate::secret::{SecretRef, SettingSecret};
use crate::settings::{RemotePairing, Settings};

/// One stored server, id 1, with `env`.
struct OneServer(Vec<McpEnvEntry>);

#[async_trait]
impl McpServerRepository for OneServer {
    async fn insert(&self, _server: NewMcpServer) -> Result<McpServer, McpRepositoryError> {
        unimplemented!()
    }
    async fn get_by_id(&self, id: i64) -> Result<McpServer, McpRepositoryError> {
        if id != 1 {
            return Err(McpRepositoryError::NotFound(format!("id={id}")));
        }
        let new = NewMcpServer::new_stdio("files", "npx", Vec::new(), None);
        Ok(McpServer {
            id,
            name: new.name,
            server_type: new.server_type,
            config: new.config,
            enabled: true,
            allow_remote: false,
            lifecycle: new.lifecycle,
            env: self.0.clone(),
            created_at: chrono::Utc::now(),
            last_connected_at: None,
            is_valid: true,
            last_error: None,
        })
    }
    async fn get_by_name(&self, name: &str) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::NotFound(name.to_owned()))
    }
    async fn list(&self) -> Result<Vec<McpServer>, McpRepositoryError> {
        unimplemented!()
    }
    async fn update(&self, _server: &McpServer) -> Result<(), McpRepositoryError> {
        unimplemented!()
    }
    async fn delete(&self, _id: i64) -> Result<(), McpRepositoryError> {
        unimplemented!()
    }
    async fn update_last_connected(&self, _id: i64) -> Result<(), McpRepositoryError> {
        unimplemented!()
    }
    async fn set_allow_remote(&self, _id: i64, _allow: bool) -> Result<(), McpRepositoryError> {
        unimplemented!()
    }
}

fn service(settings: Settings, env: &[(&str, &str)]) -> SecretsService {
    let env = env
        .iter()
        .map(|(key, value)| McpEnvEntry::new(*key, *value))
        .collect();
    SecretsService::new(
        Arc::new(InMemorySettings::with(settings)),
        Arc::new(OneServer(env)),
    )
}

const PROXY_KEY: SecretRef = SecretRef::Setting {
    secret: SettingSecret::ProxyApiKey,
};
const PAIRING_KEY: SecretRef = SecretRef::Setting {
    secret: SettingSecret::RemotePairingKey,
};

fn env(key: &str) -> SecretRef {
    SecretRef::McpEnv {
        server_id: 1,
        key: key.to_owned(),
    }
}

#[tokio::test]
async fn each_setting_secret_is_revealed_from_settings() {
    let settings = Settings {
        proxy_api_key: Some("proxy-key".to_owned()),
        remote_pairing: Some(RemotePairing {
            ticket: "ticket".to_owned(),
            api_key: "device-key".to_owned(),
            default_model: None,
            port: None,
            name: None,
        }),
        ..Settings::default()
    };
    let secrets = service(settings, &[]);

    assert_eq!(
        secrets.reveal(&PROXY_KEY).await.unwrap().as_deref(),
        Some("proxy-key")
    );
    assert_eq!(
        secrets.reveal(&PAIRING_KEY).await.unwrap().as_deref(),
        Some("device-key")
    );
}

/// Nothing stored and a blank stored are the same answer, so no caller sees
/// a blank key as one that is set.
#[tokio::test]
async fn an_absent_or_blank_setting_reveals_nothing() {
    for stored in [None, Some("   ")] {
        let settings = Settings {
            proxy_api_key: stored.map(str::to_owned),
            ..Settings::default()
        };
        let secrets = service(settings, &[]);

        assert_eq!(
            secrets.reveal(&PROXY_KEY).await.unwrap(),
            None,
            "{stored:?}"
        );
        assert_eq!(
            secrets.reveal(&PAIRING_KEY).await.unwrap(),
            None,
            "{stored:?}"
        );
    }
}

#[tokio::test]
async fn an_mcp_env_value_is_revealed_by_its_key_and_an_unknown_key_reveals_nothing() {
    let secrets = service(Settings::default(), &[("TOKEN", "t0k"), ("ROOT", "/srv")]);

    assert_eq!(
        secrets.reveal(&env("TOKEN")).await.unwrap().as_deref(),
        Some("t0k")
    );
    assert_eq!(
        secrets.reveal(&env("ROOT")).await.unwrap().as_deref(),
        Some("/srv")
    );
    assert_eq!(secrets.reveal(&env("MISSING")).await.unwrap(), None);
}

#[tokio::test]
async fn an_unknown_mcp_server_is_not_found() {
    let secrets = service(Settings::default(), &[("TOKEN", "t0k")]);
    let unknown = SecretRef::McpEnv {
        server_id: 2,
        key: "TOKEN".to_owned(),
    };

    let refusal = secrets.reveal(&unknown).await.unwrap_err();

    assert!(
        matches!(
            refusal,
            CoreError::Repository(crate::ports::RepositoryError::NotFound(_))
        ),
        "{refusal:?}"
    );
}
