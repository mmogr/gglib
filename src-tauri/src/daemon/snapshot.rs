//! What the daemon is doing, as of the last poll.
//!
//! Pure: parsing and derivation only, no `AppHandle` and no I/O, so every rule
//! below is testable without a running daemon or a running app.

use serde_json::Value;

/// A point-in-time reading of the daemon.
///
/// One struct behind one lock. Proxy state used to be two separate `RwLock`s
/// that `sync_all_state` took one at a time, so a reader could observe a
/// running proxy with no port; a single snapshot cannot tear that way.
///
/// `PartialEq` is load-bearing rather than a convenience. A repaint re-decodes
/// the icon PNG on macOS and makes `ksni` rebuild the entire menu over D-Bus on
/// Linux, so the watcher compares before it paints and does nothing at all when
/// nothing has changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DaemonSnapshot {
    /// Whether the daemon answered at all. Everything below is meaningless
    /// when this is false.
    pub reachable: bool,
    /// Whether the OpenAI-compatible proxy is listening.
    pub proxy_running: bool,
    /// The port it is listening on. Only ever `Some` while running, so a port
    /// left over from an earlier run cannot make a stopped proxy look
    /// reachable.
    pub proxy_port: Option<u16>,
    /// Model ids with a live llama-server, ascending.
    ///
    /// Sorted on construction, and that is not tidiness: `GET /api/servers`
    /// makes no ordering promise, and an order that varied between polls would
    /// compare unequal every time and repaint the tray forever.
    pub resident: Vec<i64>,
}

impl DaemonSnapshot {
    /// Read a snapshot from `/api/proxy/status` and `/api/servers` bodies.
    ///
    /// Missing or malformed fields read as "not running" rather than failing.
    /// A snapshot that cannot be built is indistinguishable from an
    /// unreachable daemon, and the tray has to show something either way.
    #[must_use]
    pub fn from_responses(proxy: &Value, servers: &Value) -> Self {
        let proxy_running = proxy
            .get("running")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let mut resident: Vec<i64> = servers
            .as_array()
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.get("model_id").and_then(Value::as_i64))
                    .collect()
            })
            .unwrap_or_default();
        resident.sort_unstable();
        resident.dedup();

        Self {
            reachable: true,
            proxy_running,
            proxy_port: if proxy_running { port_of(proxy) } else { None },
            resident,
        }
    }

    /// The OpenAI-compatible endpoint to hand to another client.
    ///
    /// `None` unless the proxy is actually listening. Both menus already grey
    /// out the copy action while it is stopped, so there is nothing to fall
    /// back to — and a guessed default port would hand out a dead URL, which
    /// is worse than the button doing nothing.
    #[must_use]
    pub fn endpoint_url(&self) -> Option<String> {
        self.proxy_port
            .map(|port| format!("http://127.0.0.1:{port}/v1"))
    }

    /// Whether a given model has a live server.
    ///
    /// Binary search because [`Self::resident`] is sorted; this is what lets
    /// the macOS menu answer the question without its own HTTP call.
    #[must_use]
    pub fn serves(&self, model_id: i64) -> bool {
        self.resident.binary_search(&model_id).is_ok()
    }
}

/// The proxy's port, if it reported one that fits.
fn port_of(proxy: &Value) -> Option<u16> {
    proxy
        .get("port")
        .and_then(Value::as_u64)
        .and_then(|port| u16::try_from(port).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_running_proxy_reports_its_port() {
        let snap =
            DaemonSnapshot::from_responses(&json!({"running": true, "port": 8080}), &json!([]));

        assert!(snap.reachable);
        assert!(snap.proxy_running);
        assert_eq!(snap.proxy_port, Some(8080));
    }

    /// A port from an earlier run must not survive the proxy stopping, or
    /// every surface that shows an endpoint hands out a dead one.
    #[test]
    fn a_stopped_proxy_keeps_no_port() {
        let snap =
            DaemonSnapshot::from_responses(&json!({"running": false, "port": 8080}), &json!([]));

        assert!(!snap.proxy_running);
        assert_eq!(snap.proxy_port, None);
    }

    /// The whole point of comparing snapshots is skipping repaints, so two
    /// readings of an unchanged daemon must be equal however the server list
    /// happened to be ordered.
    #[test]
    fn server_order_does_not_count_as_a_change() {
        let first = DaemonSnapshot::from_responses(
            &json!({"running": false}),
            &json!([{"model_id": 7}, {"model_id": 2}]),
        );
        let second = DaemonSnapshot::from_responses(
            &json!({"running": false}),
            &json!([{"model_id": 2}, {"model_id": 7}]),
        );

        assert_eq!(first, second);
        assert_eq!(first.resident, vec![2, 7]);
    }

    /// A model resident with the proxy off is exactly the case the old
    /// proxy-only tray could not see: VRAM held, nothing on screen saying so.
    #[test]
    fn residents_are_tracked_with_the_proxy_stopped() {
        let snap =
            DaemonSnapshot::from_responses(&json!({"running": false}), &json!([{"model_id": 1}]));

        assert!(snap.serves(1));
        assert!(!snap.serves(2));
    }

    #[test]
    fn the_endpoint_url_needs_a_listening_proxy() {
        let running =
            DaemonSnapshot::from_responses(&json!({"running": true, "port": 9000}), &json!([]));
        let stopped = DaemonSnapshot::from_responses(&json!({"running": false}), &json!([]));

        assert_eq!(
            running.endpoint_url().as_deref(),
            Some("http://127.0.0.1:9000/v1")
        );
        assert_eq!(stopped.endpoint_url(), None);
    }

    /// The default stands for "we could not reach the daemon", so it must not
    /// look like an idle one that answered.
    #[test]
    fn the_default_is_unreachable() {
        let idle = DaemonSnapshot::from_responses(&json!({"running": false}), &json!([]));

        assert!(idle.reachable);
        assert!(!DaemonSnapshot::default().reachable);
    }

    /// Garbage in the body must not panic or invent state — the daemon may be
    /// a different build than this app.
    #[test]
    fn unexpected_bodies_read_as_nothing_running() {
        let snap = DaemonSnapshot::from_responses(&json!("nonsense"), &json!({"not": "an array"}));

        assert!(!snap.proxy_running);
        assert_eq!(snap.proxy_port, None);
        assert!(snap.resident.is_empty());
    }
}
