//! The remote tunnel's calls on a [`DaemonHandle`] (ADR 0012).

use std::time::Duration;

use anyhow::Result;
use gglib_app_services::{
    PairedModels, RemoteDevice, RemoteEnableBody, RemoteEnableResponse, RemoteForgotten,
    RemoteJoinBody, RemoteJoinResponse, RemoteStatus,
};
use gglib_core::domain::ModelLookup;
use gglib_proxy::LoadResponse;

use super::{DaemonHandle, paths};

impl DaemonHandle {
    /// Bring the tunnel up and arm a pairing. The response is the only time
    /// the ticket and the code are ever handed out.
    ///
    /// Long timeout: the daemon waits for the endpoint to find a relay before
    /// minting the ticket, and may wait one settings-cache window for a
    /// freshly minted key to take effect on the local proxy.
    pub(crate) async fn remote_enable(
        &self,
        body: &RemoteEnableBody,
    ) -> Result<RemoteEnableResponse> {
        let response = self
            .post(paths::REMOTE_ENABLE_PATH)
            .json(body)
            .timeout(Duration::from_secs(45))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Take the tunnel down (idempotent on the daemon side).
    pub(crate) async fn remote_disable(&self) -> Result<RemoteStatus> {
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
    /// The same timeout as `remote_enable`'s. An invite typed at a daemon
    /// that is still putting its tunnel back after a start waits for it, for
    /// up to twenty seconds, and only then are two stores written and the
    /// edge told.
    pub(crate) async fn remote_invite(&self) -> Result<RemoteEnableResponse> {
        let response = self
            .post(paths::REMOTE_INVITE_PATH)
            .json(&serde_json::json!({}))
            .timeout(Duration::from_secs(45))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Every device this machine has issued a key to.
    pub(crate) async fn remote_devices(&self) -> Result<Vec<RemoteDevice>> {
        let response = self
            .get(paths::REMOTE_DEVICES_PATH)
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Stop admitting one device. `forgotten` is false when this machine
    /// held nothing under that name, which is an answer rather than an error.
    pub(crate) async fn remote_forget(&self, device: &str) -> Result<RemoteForgotten> {
        let response = self
            .delete(&paths::remote_forget_path(device))
            .timeout(Duration::from_secs(15))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// The tunnel's status.
    pub(crate) async fn remote_status(&self) -> Result<RemoteStatus> {
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
    pub(crate) async fn remote_join(&self, body: &RemoteJoinBody) -> Result<RemoteJoinResponse> {
        let response = self
            .post(paths::REMOTE_JOIN_PATH)
            .json(body)
            .timeout(Duration::from_mins(1))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Close the loopback port (idempotent on the daemon side).
    pub(crate) async fn remote_disconnect(&self) -> Result<RemoteStatus> {
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
    pub(crate) async fn remote_kill(&self) -> Result<RemoteStatus> {
        let response = self
            .post(paths::REMOTE_KILL_PATH)
            .json(&serde_json::json!({ "confirm": "shutdown" }))
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// The paired machine's models, read through the tunnel by the daemon,
    /// with what may be done to them. Profile variants included.
    ///
    /// The daemon gives the far machine three seconds; this gives the daemon
    /// a little more.
    pub(crate) async fn paired_models(&self) -> Result<PairedModels> {
        let response = self
            .get(paths::REMOTE_MODELS_PATH)
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// One of the paired machine's models, by an identifier that machine
    /// resolves as it resolves a turn's: what it resolved to, and the profile
    /// it named. The identifier travels as one encoded path segment.
    pub(crate) async fn paired_model(&self, identifier: &str) -> Result<ModelLookup> {
        let response = self
            .get(&paths::remote_model_path(identifier))
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Have one of the paired machine's models resident now, by an
    /// identifier that machine resolves, launched with `num_ctx` when given.
    /// The identifier travels as one encoded path segment.
    ///
    /// Long timeout: the far machine's admission may queue the load for up
    /// to three minutes, and the daemon gives the far machine four.
    pub(crate) async fn paired_load(
        &self,
        identifier: &str,
        num_ctx: Option<u64>,
    ) -> Result<LoadResponse> {
        let response = self.paired_load_request(identifier, num_ctx).send().await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// The request [`Self::paired_load`] sends.
    fn paired_load_request(
        &self,
        identifier: &str,
        num_ctx: Option<u64>,
    ) -> reqwest::RequestBuilder {
        self.post(&paths::remote_model_load_path(identifier))
            .json(&serde_json::json!({ "num_ctx": num_ctx }))
            .timeout(Duration::from_mins(5))
    }
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
