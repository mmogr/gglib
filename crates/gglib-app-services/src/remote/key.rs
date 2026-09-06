//! Which key the tunnel enforces, whether one has to be minted first, and
//! when that mint is written down.
//!
//! The decision is pure — what the proxy currently demands against what
//! settings hold. The write is not, and the two are separate here because
//! *when* it happens is the whole point: minting a key puts a bearer
//! requirement on the local proxy that outlives the tunnel, so it is
//! deliberately the last thing `enable` does before the pairing, not the
//! first (see [`Settled::commit`]).

use gglib_core::ApiKeySource;
use gglib_core::SettingsUpdate;
use gglib_core::access::generate_api_key;
use gglib_core::services::{AppCore, SETTINGS_CACHE_TTL};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::error::GuiError;
use crate::proxy::ProxyOps;

/// The key the tunnel will enforce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum KeyDecision {
    /// Enforce this key; the proxy already demands it. Nothing to write.
    Use {
        /// The key.
        key: String,
        /// Whether it came from a flag or environment variable, in which
        /// case a settings rotation will never change it and the poller has
        /// nothing to watch.
        pinned: bool,
    },
    /// Nothing is enforced anywhere yet. Persist this freshly minted key to
    /// `proxy_api_key`, wait for the proxy's tracking policy to pick it up,
    /// then enforce it.
    Mint(String),
}

/// Decide the key, in the order ADR 0012 gives.
///
/// 1. What the running proxy demands, when it demands something. A key from
///    `--api-key`/`GGLIB_API_KEY` is pinned: it never appears in settings,
///    and handing the tunnel the stored value would give it a credential
///    the proxy refuses.
/// 2. The stored `proxy_api_key`, when the proxy has not (yet) resolved one
///    — it will, within a cache window, because the tracking policy reads
///    the same setting.
/// 3. A fresh key, to be persisted. This is the loopback default, where
///    nothing minted a key because nothing was reachable. Enabling the
///    tunnel is exactly the moment that stops being true.
pub(super) fn decide(
    effective: Option<(String, ApiKeySource)>,
    stored: Option<&str>,
) -> KeyDecision {
    if let Some((key, source)) = effective {
        return KeyDecision::Use {
            key,
            pinned: source == ApiKeySource::Flag,
        };
    }
    if let Some(stored) = stored.map(str::trim).filter(|s| !s.is_empty()) {
        return KeyDecision::Use {
            key: stored.to_owned(),
            pinned: false,
        };
    }
    KeyDecision::Mint(generate_api_key())
}

/// The key this tunnel will enforce, and the write it may still owe.
pub(super) struct Settled {
    /// The key, ready to hand to `modelpipe::serve` and to the pairing.
    pub(super) key: String,
    /// Whether it came from a flag or environment variable, in which case a
    /// settings rotation will never change it and the poller has nothing to
    /// watch.
    pub(super) pinned: bool,
    /// Whether it was minted here and is therefore not in settings yet.
    minted: bool,
}

/// Settle the key the tunnel will enforce — and write nothing.
///
/// Reads what the running proxy demands and what settings hold, and mints
/// when neither has anything. A minted key is carried in the returned value
/// until [`Settled::commit`] puts it in settings.
///
/// # Errors
///
/// `Internal` when settings cannot be read.
pub(super) async fn settle(proxy: &ProxyOps, core: &AppCore) -> Result<Settled, GuiError> {
    let settings = core
        .settings()
        .get()
        .await
        .map_err(|e| GuiError::Internal(format!("could not read settings: {e}")))?;
    Ok(
        match decide(proxy.effective_api_key(), settings.proxy_api_key.as_deref()) {
            KeyDecision::Use { key, pinned } => Settled {
                key,
                pinned,
                minted: false,
            },
            KeyDecision::Mint(key) => Settled {
                key,
                pinned: false,
                minted: true,
            },
        },
    )
}

impl Settled {
    /// Write a minted key down, and wait for the local proxy to pick it up.
    /// A no-op for a key that was already being enforced somewhere.
    ///
    /// **Called once the tunnel is up, and that is the point.** This write
    /// is not undoable in practice: it makes the loopback proxy demand a
    /// bearer token from then on, `disable` deliberately leaves it in place
    /// (ADR 0012, decision 2), and clearing it again would reopen the local
    /// proxy — `/mcp` included — for anything that adopted it in between.
    /// So it must not happen for a tunnel that never came up. Ahead of the
    /// bind, as it used to be, every `modelpipe::serve` failure left the
    /// machine authenticating with nothing to show for it and nothing said:
    /// the error is the CLI's `?`, so `print_notice` — the one thing that
    /// tells the operator the local door just locked, promised by
    /// `docs/remote.md` "every time it runs" — never ran.
    ///
    /// The wait stays: the proxy's tracking policy reads settings through a
    /// cache, and handing out a ticket before the local door is locked would
    /// open the window the whole design exists to close. It is `enable`'s
    /// return, not `serve`, that has to be behind it — nothing can reach a
    /// tunnel whose ticket has not left this process.
    ///
    /// The cost of coming last is that
    /// [`backend::refuse_if_gone`](super::backend::refuse_if_gone) now runs
    /// before this wait rather than after it, so on a first enable its answer
    /// can be a cache window old. That is the better half of the trade:
    /// asking it afterwards would keep the answer fresh to the last instant
    /// and pay for it with a key written for a tunnel that is then refused,
    /// which is precisely the undisclosed state above. The staleness leaves
    /// nothing hidden — the watcher takes the tunnel down as soon as the
    /// install spawns it, exactly as it does for a proxy that exits a second
    /// later, and the key and its notice both reached the operator.
    ///
    /// The wait is cut short by `cancel`, which is not a shortcut: the key is
    /// already written and the caller is about to give up. Without this the
    /// serve side would hold a `disable` for the whole window, which is the
    /// one thing reserving the slot rather than locking it exists to stop.
    ///
    /// # Errors
    ///
    /// `Internal` when the key cannot be stored.
    pub(super) async fn commit(
        &self,
        core: &AppCore,
        cancel: &CancellationToken,
    ) -> Result<(), GuiError> {
        if !self.minted {
            return Ok(());
        }
        core.settings()
            .update(SettingsUpdate {
                proxy_api_key: Some(Some(self.key.clone())),
                ..SettingsUpdate::default()
            })
            .await
            .map_err(|e| GuiError::Internal(format!("could not store the API key: {e}")))?;
        info!("minted an API key for the proxy; waiting for it to take effect");
        tokio::select! {
            () = cancel.cancelled() => {}
            () = tokio::time::sleep(SETTINGS_CACHE_TTL) => {}
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "key_tests.rs"]
mod key_tests;
