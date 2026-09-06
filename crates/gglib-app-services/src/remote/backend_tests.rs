//! Tests for [`super::Backend`] — the proxy's bind address as a backend
//! modelpipe will dial — and for when the tunnel stops fronting it.
//!
//! The verdicts asserted here are modelpipe's, restated: `locality::admits`
//! is `pub(crate)` over there, so this side cannot ask it and instead has to
//! predict it. Each case names the classification it is predicting, so a
//! modelpipe release that moves one of those lines fails against a test that
//! says what it believed rather than a URL that looks arbitrary.

use super::*;

fn addr(s: &str) -> SocketAddr {
    s.parse().expect("test address")
}

// ── Turning a bind address into a dial address ───────────────────────────

/// The case that made `enable` fail on a proxy someone had deliberately
/// made reachable: `0.0.0.0` classifies as `Unspecified`, which modelpipe
/// refuses whatever the flag says, and the refusal blamed the proxy the user
/// had just configured on purpose.
#[test]
fn a_wildcard_bind_is_dialled_on_loopback_with_its_own_port_kept() {
    let backend = Backend::at(addr("0.0.0.0:8080"));
    assert_eq!(backend.url, "http://127.0.0.1:8080");
    assert!(!backend.allow_private, "loopback needs no flag");

    // Both families, and a port that is not the default one, because
    // rewriting the host and then losing the port is the mistake a
    // hardcoded `127.0.0.1:8080` would make.
    let backend = Backend::at(addr("[::]:19099"));
    assert_eq!(backend.url, "http://[::1]:19099");
    assert!(!backend.allow_private);
}

/// The ordinary case has to stay untouched: a loopback bind is already the
/// address modelpipe wants, and the IPv6 spelling has to survive the trip
/// bracketed, which is what a URL authority needs and what modelpipe strips
/// back off before it resolves.
#[test]
fn a_loopback_bind_is_dialled_as_written_and_needs_no_flag() {
    for (bound, url) in [
        ("127.0.0.1:8080", "http://127.0.0.1:8080"),
        ("127.0.0.2:1234", "http://127.0.0.2:1234"),
        ("[::1]:8080", "http://[::1]:8080"),
    ] {
        let backend = Backend::at(addr(bound));
        assert_eq!(backend.url, url, "{bound}");
        assert!(!backend.allow_private, "{bound}");
    }
}

/// A LAN bind names a real interface that loopback would not reach, so it is
/// dialled as written — and modelpipe classifies it `Private`, which it
/// refuses unless it is told to expect one. Rewriting this to loopback would
/// be the wrong repair for the same symptom: it would silently dial a
/// different server than the operator pointed at.
#[test]
fn a_lan_bind_is_dialled_as_written_and_carries_the_flag_modelpipe_needs() {
    for (bound, url) in [
        ("192.168.1.5:8080", "http://192.168.1.5:8080"),
        ("10.0.0.1:8080", "http://10.0.0.1:8080"),
        ("172.16.0.1:8080", "http://172.16.0.1:8080"),
        ("[fd12:3456::1]:8080", "http://[fd12:3456::1]:8080"),
    ] {
        let backend = Backend::at(addr(bound));
        assert_eq!(backend.url, url, "{bound}");
        assert!(backend.allow_private, "{bound} must carry the flag");
    }
}

/// The flag is set from the same canonicalization modelpipe does, so an
/// IPv4-mapped LAN address gets it too. Asking an `Ipv6Addr` whether it is
/// RFC 1918 answers "no" — correctly, and uselessly.
#[test]
fn an_ipv4_mapped_lan_address_is_still_a_private_one() {
    let backend = Backend::at(addr("[::ffff:192.168.1.5]:8080"));
    assert!(backend.allow_private);

    // And the unwrapping is applied across the board rather than
    // special-cased: a mapped loopback address is loopback.
    assert!(!Backend::at(addr("[::ffff:127.0.0.1]:8080")).allow_private);
}

/// Link-local and public binds are left exactly as they are, with no flag,
/// so modelpipe refuses them — which is the correct outcome, not a gap.
/// `169.254.169.254` is cloud instance metadata and a tunnel that dialled it
/// on a stranger's behalf would be a credential-exfiltration primitive; a
/// routable address is a server this machine does not own. Neither is
/// something this side may quietly readmit, and the flag would not readmit
/// them anyway — it moves `Private` and nothing else.
#[test]
fn a_link_local_or_public_bind_is_left_for_modelpipe_to_refuse() {
    for bound in [
        "169.254.169.254:8080",
        "[fe80::1]:8080",
        "203.0.113.1:8080",
        "[2001:db8::1]:8080",
    ] {
        let backend = Backend::at(addr(bound));
        assert!(
            !backend.allow_private,
            "{bound} must not be handed a flag that cannot admit it"
        );
    }
}

// ── When the tunnel stops fronting its proxy ─────────────────────────────

/// Both exits count. `POST /api/proxy/stop` publishes `Stopped` and a proxy
/// task that fell over publishes `Crashed`; reacting only to the crash would
/// leave the deliberate stop — the one a person just asked for — with a
/// tunnel still forwarding into a port nobody owns.
#[test]
fn both_ways_the_proxy_exits_take_the_tunnel_down() {
    let backend = Backend::at(addr("127.0.0.1:8080"));
    assert!(!still_fronting(&ProxyStatus::Stopped, &backend));
    assert!(!still_fronting(&ProxyStatus::Crashed, &backend));
}

/// The ordinary poll: the proxy is up on the address the tunnel dials, so
/// nothing happens. This is the answer several times a minute for the whole
/// life of a session, and it must not cost the tunnel anything.
#[test]
fn a_proxy_still_on_the_address_the_tunnel_dials_is_left_alone() {
    let backend = Backend::at(addr("127.0.0.1:8080"));
    let status = ProxyStatus::Running {
        address: addr("127.0.0.1:8080"),
    };
    assert!(still_fronting(&status, &backend));
}

/// The address is re-read on every poll, and the rewrite is applied to what
/// comes back — otherwise a proxy on the wildcard would compare its own
/// `0.0.0.0:8080` against the `127.0.0.1:8080` the tunnel was given and read
/// as a stranger, tearing down a healthy tunnel every five seconds.
#[test]
fn a_wildcard_bind_still_matches_the_loopback_address_it_was_rewritten_to() {
    let backend = Backend::at(addr("0.0.0.0:8080"));
    let status = ProxyStatus::Running {
        address: addr("0.0.0.0:8080"),
    };
    assert!(still_fronting(&status, &backend));
}

/// A proxy that went away and came back on another port is running, and is
/// not this tunnel's backend. `modelpipe::serve` still holds the old port, so
/// leaving the tunnel up would forward to whatever holds it now.
#[test]
fn a_proxy_that_came_back_on_another_port_is_not_the_one_being_fronted() {
    let backend = Backend::at(addr("127.0.0.1:8080"));
    let status = ProxyStatus::Running {
        address: addr("127.0.0.1:9099"),
    };
    assert!(!still_fronting(&status, &backend));
}
