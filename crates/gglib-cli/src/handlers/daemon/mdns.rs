//! mDNS/DNS-SD service advertising for `gglib daemon run --share-lan`.
//!
//! Registers `_gglib._tcp.local.` so the server is reachable at `gglib.local`
//! from any device on the LAN without anyone having to look up an IP address.
//!
//! Advertising is strictly opt-in: [`MdnsAdvertiser::start`] is only ever
//! called when LAN sharing resolved to `true`, so localhost-only runs (and
//! every test) never spawn a daemon. Every failure here is non-fatal — the HTTP
//! server is still perfectly usable by IP, so problems are logged and startup
//! continues.

use mdns_sd::{ServiceDaemon, ServiceInfo};

/// Whether `host` is a wildcard bind (`0.0.0.0` or `::`).
///
/// Moved here from the old `gglib web` bind resolver when LAN sharing became
/// a daemon concern.
fn is_wildcard(host: &str) -> bool {
    matches!(
        host.parse::<std::net::IpAddr>(),
        Ok(addr) if addr.is_unspecified()
    )
}

/// DNS-SD service type for GGLib web servers.
const SERVICE_TYPE: &str = "_gglib._tcp.local.";

/// Instance name, and the label of the advertised hostname.
///
/// Fixed rather than derived from the machine's hostname so the address is
/// predictably `gglib.local` regardless of what the host calls itself.
const INSTANCE_NAME: &str = "gglib";

/// Hostname the address record is published under → resolves as `gglib.local`.
const HOSTNAME: &str = "gglib.local.";

/// The name LAN clients actually type — [`HOSTNAME`] without the trailing
/// dot. Exposed so `daemon run` can add it to the Host allowlist: a name the
/// daemon advertises is plainly one it answers to.
pub(crate) const LAN_HOSTNAME: &str = "gglib.local";

/// An active mDNS registration, unregistered on [`shutdown`](Self::shutdown).
pub(super) struct MdnsAdvertiser {
    daemon: ServiceDaemon,
    fullname: String,
}

impl MdnsAdvertiser {
    /// Start advertising the web server on the local network.
    ///
    /// Returns `None` if the daemon could not be started or the service could
    /// not be registered; the caller carries on serving over HTTP either way.
    ///
    /// When `host` is the wildcard address, `enable_addr_auto` lets mdns-sd
    /// discover the machine's interface addresses and keep the record current
    /// as they change. A specific `host` is advertised verbatim, so a server
    /// narrowed to one interface does not advertise the others.
    pub(super) fn start(host: &str, port: u16) -> Option<Self> {
        let daemon = match ServiceDaemon::new() {
            Ok(daemon) => daemon,
            Err(e) => {
                tracing::warn!(
                    "mDNS: could not start the responder ({e}); \
                     the server is still reachable by IP address"
                );
                return None;
            }
        };

        // A wildcard address (`0.0.0.0` or `::`) means "every interface" — hand
        // address selection to mdns-sd rather than guessing a primary NIC.
        let service = if is_wildcard(host) {
            ServiceInfo::new(SERVICE_TYPE, INSTANCE_NAME, HOSTNAME, "", port, None)
                .map(ServiceInfo::enable_addr_auto)
        } else {
            ServiceInfo::new(SERVICE_TYPE, INSTANCE_NAME, HOSTNAME, host, port, None)
        };

        let service = match service {
            Ok(service) => service,
            Err(e) => {
                tracing::warn!("mDNS: could not build the service record ({e}); not advertising");
                return None;
            }
        };

        let fullname = service.get_fullname().to_owned();
        if let Err(e) = daemon.register(service) {
            tracing::warn!("mDNS: could not register {fullname} ({e}); not advertising");
            return None;
        }

        tracing::info!("mDNS: advertising {fullname} as http://{INSTANCE_NAME}.local:{port}");
        eprintln!("  \u{1f4e1} Discoverable at: http://{INSTANCE_NAME}.local:{port}");

        Some(Self { daemon, fullname })
    }

    /// Withdraw the record and stop the responder.
    ///
    /// Awaits the unregister acknowledgement before shutting the daemon down —
    /// dropping it immediately would leave the goodbye packet unsent and the
    /// stale record cached by every resolver on the network.
    pub(super) async fn shutdown(self) {
        match self.daemon.unregister(&self.fullname) {
            Ok(rx) => {
                if let Err(e) = rx.recv_async().await {
                    tracing::warn!(
                        "mDNS: unregister of {} was not confirmed ({e})",
                        self.fullname
                    );
                }
            }
            Err(e) => tracing::warn!("mDNS: could not unregister {} ({e})", self.fullname),
        }

        if let Err(e) = self.daemon.shutdown() {
            tracing::warn!("mDNS: responder shutdown failed ({e})");
        }
    }
}
