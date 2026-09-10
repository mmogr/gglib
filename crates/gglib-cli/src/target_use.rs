//! The use side of ADR 0013: commands that use the paired machine rather
//! than talk to a model on it.
//!
//! A `#[path]` child of `target.rs`. A turn has one shape on both machines
//! and the decisions inside it are the parent's methods; the commands here
//! — load a model, list the catalogue, watch the dashboard, stop the daemon
//! — are two whole commands each, with their own words and their own
//! rendering, and what the target owes them is only the choice. [`run`]
//! is that choice, taken once here so no command takes it itself.
//!
//! [`run`]: Target::run

use anyhow::Result;

use super::Target;
use crate::bootstrap::CliContext;

impl Target {
    /// Run `here` on this machine, or `far` on the paired one.
    pub(crate) async fn run<T>(
        self,
        here: impl AsyncFnOnce() -> Result<T>,
        far: impl AsyncFnOnce() -> Result<T>,
    ) -> Result<T> {
        match self {
            Self::Local => here().await,
            Self::Remote => far().await,
        }
    }

    /// Where a proxy client command connects, and with what.
    ///
    /// `proxy dashboard` and `proxy cache-clear` take a host, a port and a
    /// key because they connect to a proxy directly; on this machine those
    /// are the flags as typed (the key falling back to the stored
    /// `proxy_api_key`), and on the paired machine they are the tunnel's
    /// loopback port and the key this machine received when it paired —
    /// the flags then name a proxy that is not the one being asked about,
    /// and are refused rather than quietly replaced.
    ///
    /// # Errors
    ///
    /// On the paired machine, whatever [`far`](Self::far) says; and a host
    /// or port given beside `--remote`.
    pub(crate) async fn proxy_endpoint(
        self,
        ctx: &CliContext,
        host: String,
        port: u16,
        api_key: Option<String>,
    ) -> Result<(String, u16, Option<String>)> {
        match self {
            Self::Local => Ok((host, port, client_api_key(ctx, api_key).await)),
            Self::Remote => {
                if host != "127.0.0.1" || port != 8080 || api_key.is_some() {
                    anyhow::bail!(
                        "--remote names the paired machine's proxy, so --host, --port and \
                         --api-key would name a different one; drop them"
                    );
                }
                let far = self.far(ctx).await?;
                Ok(("127.0.0.1".to_owned(), far.port, Some(far.key)))
            }
        }
    }
}

/// The key a proxy client sends to a proxy on this machine: the flag, or
/// the stored `proxy_api_key` when there is one and it is not blank.
async fn client_api_key(ctx: &CliContext, flag: Option<String>) -> Option<String> {
    if flag.is_some() {
        return flag;
    }
    ctx.app
        .settings()
        .get()
        .await
        .ok()
        .and_then(|s| s.proxy_api_key)
        .filter(|key| !key.trim().is_empty())
}
