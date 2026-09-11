//! The remote tunnel's calls on a [`DaemonHandle`] (ADR 0012).

use std::time::Duration;

use anyhow::Result;

use super::wire::{
    RemoteConnectBody, RemoteConnectDto, RemoteDeviceDto, RemoteEnableBody, RemoteEnableDto,
    RemoteForgottenDto, RemoteStatusDto,
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
            .post(paths::REMOTE_ENABLE_PATH)
            .json(body)
            .timeout(Duration::from_secs(45))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Take the tunnel down (idempotent on the daemon side).
    pub(crate) async fn remote_disable(&self) -> Result<RemoteStatusDto> {
        let response = self
            .post(paths::REMOTE_DISABLE_PATH)
            .json(&serde_json::json!({}))
            .timeout(Duration::from_secs(15))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Mint a key for one new device and offer a code that hands it over.
    ///
    /// The same response `enable` gives, because it is the same answer: the
    /// service hands back the whole session, and the ticket in it is half of
    /// what the pairing screen draws.
    ///
    /// Shorter timeout than `remote_enable`'s. Nothing here waits on a relay
    /// — the tunnel is already up or this is refused — but two stores are
    /// written and the edge is told, so it is not instant either.
    pub(crate) async fn remote_invite(&self) -> Result<RemoteEnableDto> {
        let response = self
            .post(paths::REMOTE_INVITE_PATH)
            .json(&serde_json::json!({}))
            .timeout(Duration::from_secs(20))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Every device this machine has issued a key to.
    pub(crate) async fn remote_devices(&self) -> Result<Vec<RemoteDeviceDto>> {
        let response = self
            .get(paths::REMOTE_DEVICES_PATH)
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Stop admitting one device. `forgotten` is false when this machine
    /// held nothing under that name, which is an answer rather than an error.
    pub(crate) async fn remote_forget(&self, device: &str) -> Result<RemoteForgottenDto> {
        let response = self
            .delete(&paths::remote_forget_path(device))
            .timeout(Duration::from_secs(15))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// The tunnel's status.
    pub(crate) async fn remote_status(&self) -> Result<RemoteStatusDto> {
        let response = self
            .get(paths::REMOTE_STATUS_PATH)
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
            .post(paths::REMOTE_CONNECT_PATH)
            .json(body)
            .timeout(Duration::from_secs(60))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Close the loopback port (idempotent on the daemon side).
    pub(crate) async fn remote_disconnect(&self) -> Result<RemoteStatusDto> {
        let response = self
            .post(paths::REMOTE_DISCONNECT_PATH)
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
            .post(paths::REMOTE_KILL_PATH)
            .json(&serde_json::json!({ "confirm": "shutdown" }))
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }
}
