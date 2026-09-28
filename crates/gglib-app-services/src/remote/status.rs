//! `RemoteOps::status`: both sides of the tunnel, what settings remember of
//! the last pairing, and the roster, read for the status surface.

use tracing::warn;

use super::gateway::RemoteGateway;
use super::wire::{RemotePeer, RemoteStatus};
use super::{Live, RemoteOps, connect_watch, device_keys, device_view, identity, stored_pairing};

impl RemoteOps {
    /// The status surface's answer: both sides, what settings remember of
    /// the last pairing (by fingerprint, never the ticket), and the roster,
    /// which rides this call because its read is already paid for — see
    /// [`RemoteStatus::devices`].
    pub async fn status(&self) -> RemoteStatus {
        // Every settings answer below — the switch, the stored pairing and
        // the roster — comes off one record, which is what makes them agree:
        // a key is held *for* the machine the fingerprint names, and there is
        // no shape in which they can describe two.
        //
        // Swallowed because `status` is what someone runs *because* something
        // is wrong and must not itself fail — but logged, because the
        // fallback is an empty roster, and "no device has been paired with
        // this machine" is a confident wrong answer on the one surface a
        // person opens to decide what to revoke. `RemoteOps::list` returns
        // the error; this is the trade the two make differently, on purpose.
        let (settings, held) = {
            // Under `roster`, which `invite` and `forget` write both stores
            // under, so a device part-way through either is not one read of
            // a key with no row.
            let _guard = self.roster.lock().await;
            let settings = match self.core.settings().get().await {
                Ok(settings) => Some(settings),
                Err(e) => {
                    warn!("could not read settings for remote status; reporting none: {e}");
                    None
                }
            };
            (settings, device_keys::held_ids(self))
        };
        let remote_enabled = settings
            .as_ref()
            .is_some_and(|s| s.remote_enabled == Some(true));
        let (roster, stored) = settings
            .map(|s| (s.remote_devices.unwrap_or_default(), s.remote_pairing))
            .unwrap_or_default();
        let stored_ticket_fingerprint = stored.as_ref().and_then(stored_pairing::fingerprint);
        let has_remote_key = stored.is_some();
        let connected = self.connection().await;
        // Before the lock: this touches the filesystem — creating or
        // tightening the data directory — and `status` is the call everything else waits
        // behind. Nothing under the serve slot should be doing IO that has
        // nothing to do with the slot.
        let identity_path = identity::identity_path()
            .ok()
            .flatten()
            .map(|p| p.display().to_string());
        let live = self.live.lock().await;
        // Under the slot, like `list`'s: what the edge holds and whether the
        // tunnel is up have to be read at one instant or a row can come back
        // "not admitted" from a session that had already gone.
        let admitting = live.full().map(|l| l.handle.token_names());
        let now_ms = connect_watch::unix_ms();
        let mut status = RemoteStatus {
            devices: device_view::viewed(roster, &held, admitting.as_deref(), now_ms),
            enabled: live.full().is_some(),
            pairing_active: self.gateway.pairing.active(),
            paired: self.gateway.paired(),
            mcp_allowed: self.gateway.mcp_allowed_now(),
            tunnelled_requests: self.gateway.tunnelled_requests(),
            last_tunnelled_ms: self.gateway.last_tunnelled_ms(),
            last_peer: self.gateway.last_peer(),
            connected,
            stored_ticket_fingerprint,
            has_remote_key,
            remote_enabled,
            identity_path,
            ..RemoteStatus::default()
        };
        if let Some(Live { handle, .. }) = live.full() {
            status.ticket_fingerprint = Some(handle.ticket().fingerprint());
            status.path = Some(handle.status().as_str().to_owned());
            status.peers = handle
                .peers()
                .into_iter()
                .map(|peer| RemotePeer {
                    fingerprint: peer.fingerprint,
                    path: peer.path.as_str().to_owned(),
                })
                .collect();
        }
        status
    }
}

impl RemoteGateway {
    /// [`RemoteGatewayPort::mcp_allowed`](gglib_core::ports::RemoteGatewayPort::mcp_allowed),
    /// reachable without importing the trait.
    fn mcp_allowed_now(&self) -> bool {
        gglib_core::ports::RemoteGatewayPort::mcp_allowed(self)
    }
}
