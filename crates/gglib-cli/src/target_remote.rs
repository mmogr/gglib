//! The paired machine, as this machine reaches it.
//!
//! A `#[path]` child of `target.rs`, split off when the decisions there
//! brought the file to the size budget. Everything here talks to the daemon
//! and reads the pairing, and [`Far`] is the result: one value that every
//! use-side command and every turn asks for, so "connected, holding a key"
//! is established once. Nothing here decides anything — which commands get
//! a `Far` at all is the parent's table.

use anyhow::{Result, anyhow, bail};
use gglib_runtime::FarMachine;

use super::Upstream;
use crate::bootstrap::CliContext;
use crate::daemon_client::{self, DaemonProbe};
use crate::handlers::agent_chat::config::BannerInfo;
use crate::handlers::agent_chat::upstream;
use crate::presentation::style;

/// The paired machine, as this machine can reach it: the tunnel's loopback
/// port, the key this machine received when it paired, and the name it
/// knows the machine by.
///
/// One value for every use-side command, so that "connected, holding a
/// key" is established once and refused with one set of sentences.
pub(crate) struct Far {
    /// `http://127.0.0.1:<port>/v1`, exactly as the daemon reports it.
    pub base_url: String,
    /// The tunnel's loopback port, for a command that takes host and port.
    pub port: u16,
    /// The key this machine received when it paired.
    pub key: String,
    /// The ticket fingerprint — the only name this side has for that one.
    pub fingerprint: String,
    /// How the far side is being reached: `direct`, `relayed`, `idle`.
    pub path: String,
    client: reqwest::Client,
}

impl Far {
    /// `GET {base_url}{path}` with the key, decoded as `T`.
    ///
    /// # Errors
    ///
    /// A refused key, in the sentence `docs/remote.md` documents; any other
    /// non-2xx with what the far side said; a body that is not `T`.
    pub(crate) async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let response = self
            .client
            .get(format!("{}{path}", self.base_url))
            .bearer_auth(&self.key)
            .send()
            .await?;
        self.decode(response).await
    }

    /// `POST {base_url}{path}` with the key and a JSON body, decoded as `T`.
    ///
    /// # Errors
    ///
    /// As [`get_json`](Self::get_json).
    pub(crate) async fn post_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T> {
        let response = self
            .client
            .post(format!("{}{path}", self.base_url))
            .bearer_auth(&self.key)
            .json(body)
            .send()
            .await?;
        self.decode(response).await
    }

    async fn decode<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T> {
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            // Not a rotation. Under per-device keys the machine's own
            // `proxy_api_key` is not what this key is, and rotating it
            // reaches no device — so telling someone to re-pair after a
            // rotation would have them spend an invite on a problem they do
            // not have, and miss the one they do.
            bail!(
                "the remote machine {} is not admitting this device's key — either it has \
                 stopped trusting this device, or a key rotation there is still reaching the \
                 tunnel, which clears itself within a few seconds. If waiting does not fix it, \
                 run `gglib remote invite` on that machine and redeem the fresh \
                 `<ticket>-<code>` here.",
                self.fingerprint
            );
        }
        if status == reqwest::StatusCode::FORBIDDEN {
            // The device gate, which only ever sees a request the tunnel let
            // in without naming a device — a pairing code presented as an API
            // key, most likely. Worth its own sentence: the generic branch
            // below would hand back the proxy's message with no hint that the
            // fix is to pair properly rather than to retry.
            let text = response.text().await?;
            if text.contains("device_not_paired") {
                bail!(
                    "the remote machine {} refused this request because the tunnel did not name \
                     a paired device. Run `gglib remote invite` there and redeem the \
                     `<ticket>-<code>` it prints.",
                    self.fingerprint
                );
            }
            bail!(
                "the remote machine {} answered {status}: {text}",
                self.fingerprint
            );
        }
        let text = response.text().await?;
        if !status.is_success() {
            let said = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| {
                    v.pointer("/error/message")
                        .or_else(|| v.get("error"))
                        .and_then(|e| e.as_str().map(str::to_owned))
                })
                .unwrap_or(text);
            bail!(
                "the remote machine {} answered {status}: {said}",
                self.fingerprint
            );
        }
        serde_json::from_str(&text).map_err(|e| {
            anyhow!(
                "the remote machine {} answered something this build cannot read: {e}",
                self.fingerprint
            )
        })
    }
}

impl super::Target {
    /// The paired machine, ready to be asked. Only [`Remote`](super::Target::Remote)
    /// has one; asking [`Local`](super::Target::Local) is a programming
    /// error and says so.
    ///
    /// # Errors
    ///
    /// A daemon that is not running or not connected, or a pairing this
    /// machine holds no key for — each in the sentence that names the
    /// command to run.
    pub(crate) async fn far(self, ctx: &CliContext) -> Result<Far> {
        if self == Self::Local {
            bail!("internal: the local target has no far machine");
        }
        let client = reqwest::Client::new();
        if !matches!(daemon_client::probe(&client).await, DaemonProbe::Running) {
            bail!(
                "--remote needs the daemon running and connected to the other machine: \
                 `gglib remote join` first"
            );
        }
        let handle = daemon_client::DaemonHandle {
            client: client.clone(),
            api_key: daemon_client::auth::daemon_api_key(ctx).await,
        };
        let status = handle.remote_status().await?;
        let Some(connection) = status.connected else {
            bail!(
                "not connected to a remote machine — `gglib remote join [<ticket>-<code>]` first"
            );
        };
        let key = ctx
            .app
            .settings()
            .get()
            .await
            .map_err(|e| anyhow!("failed to load settings: {e}"))?
            .remote_pairing
            .map(|stored| stored.api_key)
            .ok_or_else(|| {
                anyhow!(
                    "connected to a remote machine, but this one holds no key for it — pair again \
                     with the full `<ticket>-<code>` string from `gglib remote invite` there"
                )
            })?;
        let port = reqwest::Url::parse(&connection.base_url)
            .ok()
            .and_then(|url| url.port())
            .ok_or_else(|| {
                anyhow!(
                    "the daemon reported the remote at {}, which names no port",
                    connection.base_url
                )
            })?;
        Ok(Far {
            base_url: connection.base_url,
            port,
            key,
            fingerprint: connection.ticket_fingerprint,
            path: connection.path,
            client,
        })
    }
}

/// The machine on the other end of the tunnel, as a turn's upstream.
pub(super) async fn remote_upstream(ctx: &CliContext, banner: &BannerInfo) -> Result<Upstream> {
    let far = super::Target::Remote.far(ctx).await?;
    if !banner.quiet {
        style::print_info_banner("Info", "\u{2139}\u{fe0f}");
        eprintln!(
            "  Asking the remote machine {} at {} ({})",
            far.fingerprint, far.base_url, far.path
        );
        if let Some(ref s) = banner.sampling {
            upstream::print_sampling_lines(s);
        }
        style::print_banner_close();
    }
    Ok(Upstream {
        base_url: server_root(&far.base_url),
        // The banner above has just named this machine to the user; the key
        // carries that name onward so a later refusal of it can name the same
        // machine, which by then nothing downstream could look up.
        far_machine: Some(FarMachine {
            key: far.key,
            fingerprint: far.fingerprint,
        }),
    })
}

/// `http://127.0.0.1:41234/v1` → `http://127.0.0.1:41234`.
///
/// The daemon reports the URL a client pastes, with the `/v1`; the adapter
/// builds `/v1/chat/completions` from the server root itself.
fn server_root(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_owned()
}

#[cfg(test)]
mod tests {
    use super::server_root;

    #[test]
    fn the_v1_suffix_the_daemon_reports_is_removed_once() {
        assert_eq!(
            server_root("http://127.0.0.1:41234/v1"),
            "http://127.0.0.1:41234"
        );
        assert_eq!(
            server_root("http://127.0.0.1:41234/v1/"),
            "http://127.0.0.1:41234"
        );
        assert_eq!(
            server_root("http://127.0.0.1:41234"),
            "http://127.0.0.1:41234"
        );
    }
}
