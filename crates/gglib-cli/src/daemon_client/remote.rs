//! The remote tunnel's calls on a [`DaemonHandle`] (ADR 0012).

use std::time::Duration;

use anyhow::Result;

use super::wire::{
    RemoteConnectBody, RemoteConnectDto, RemoteEnableBody, RemoteEnableDto, RemoteStatusDto,
};
use super::{DaemonHandle, paths};

impl DaemonHandle {
    /// Bring the tunnel up and arm a pairing. The response is the only time
    /// the ticket and the code are ever handed out.
    ///
    /// Long timeout: the daemon waits for the endpoint to find a relay before
    /// minting the ticket, and may wait one settings-cache window for a
    /// freshly minted key to take effect on the local proxy.
    pub(crate) async fn remote_enable(&self, body: &RemoteEnableBody) -> Result<RemoteEnableDto> {
        let response = self
            .client
            .post(self.url(paths::REMOTE_ENABLE_PATH))
            .json(body)
            .timeout(Duration::from_secs(45))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Take the tunnel down (idempotent on the daemon side).
    pub(crate) async fn remote_disable(&self) -> Result<RemoteStatusDto> {
        let response = self
            .client
            .post(self.url(paths::REMOTE_DISABLE_PATH))
            .json(&serde_json::json!({}))
            .timeout(Duration::from_secs(15))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// The tunnel's status.
    pub(crate) async fn remote_status(&self) -> Result<RemoteStatusDto> {
        let response = self
            .client
            .get(self.url(paths::REMOTE_STATUS_PATH))
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Reach another machine: bind a loopback port here that is its proxy.
    ///
    /// Long timeout: dialling may wait for a hole punch, and a first pairing
    /// makes one more request through the tunnel before answering.
    pub(crate) async fn remote_connect(
        &self,
        body: &RemoteConnectBody,
    ) -> Result<RemoteConnectDto> {
        let response = self
            .client
            .post(self.url(paths::REMOTE_CONNECT_PATH))
            .json(body)
            .timeout(Duration::from_secs(60))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Close the loopback port (idempotent on the daemon side).
    pub(crate) async fn remote_disconnect(&self) -> Result<RemoteStatusDto> {
        let response = self
            .client
            .post(self.url(paths::REMOTE_DISCONNECT_PATH))
            .json(&serde_json::json!({}))
            .timeout(Duration::from_secs(15))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Stop the far daemon through the tunnel, then disconnect. The
    /// confirmation word is the daemon route's contract, not this client's
    /// idea: the CLI has already asked the person.
    pub(crate) async fn remote_kill(&self) -> Result<RemoteStatusDto> {
        let response = self
            .client
            .post(self.url(paths::REMOTE_KILL_PATH))
            .json(&serde_json::json!({ "confirm": "shutdown" }))
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }
}
