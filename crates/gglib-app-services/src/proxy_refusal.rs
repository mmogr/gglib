//! Why the endpoint is off when a start could not bind its address, in
//! words, built here once for the status and for every refusal that says it.
//!
//! A port that is taken (`AddrInUse`) is said with what holds it when that
//! can be told: another gglib service answers an unknown route with gglib's
//! own refusal, `unknown_route`; anything else is another program. Any other
//! refused bind (an address this machine does not have, a port it may not
//! open) is said with the system's own reason, and blames no one.

use std::io::ErrorKind;
use std::net::SocketAddr;
use std::time::Duration;

/// How long the holder of a taken port has to say it is a gglib.
const ASK_WHO: Duration = Duration::from_secs(1);

/// Why a start could not bind the endpoint's address: what happened, and
/// what to do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Unbound {
    /// What happened, as a clause: "port 8080 is in use by another program".
    cause: String,
    /// What to do about it, as a sentence without its full stop.
    fix: &'static str,
}

impl Unbound {
    /// The sentence the endpoint's status carries while it is off, and the
    /// refusal a start that could not bind gives: one sentence, said alike
    /// by every surface.
    pub(crate) fn off(&self) -> String {
        format!(
            "The endpoint is off: {}. {}, then turn the endpoint on.",
            self.cause, self.fix
        )
    }

    /// The same fact without saying the endpoint is off, for a switch that
    /// put the endpoint back on as it was.
    pub(crate) fn fact(&self) -> String {
        let mut cause = self.cause.chars();
        let first = cause.next().map(|c| c.to_uppercase().collect::<String>());
        format!(
            "{}{}. {}.",
            first.unwrap_or_default(),
            cause.as_str(),
            self.fix
        )
    }
}

/// Why `address` could not be bound, by the system's `kind` and its words
/// for it, `reason`.
pub(crate) async fn unbound(address: &str, kind: ErrorKind, reason: &str) -> Unbound {
    if kind != ErrorKind::AddrInUse {
        return Unbound {
            cause: format!("{address} cannot be opened ({reason})"),
            fix: "Choose an address this machine has, and a port it may open \
                  (Settings \u{203a} Endpoint, or `gglib config set proxy-host` and `proxy-port`)",
        };
    }
    let port = address.rsplit(':').next().unwrap_or(address);
    let holder = if held_by_gglib(address).await {
        "another gglib service"
    } else {
        "another program"
    };
    Unbound {
        cause: format!("port {port} is in use by {holder}"),
        fix: "Stop it, or choose another port \
              (Settings \u{203a} Endpoint, or `gglib config set proxy-port`)",
    }
}

/// Whether what listens at `address` answers as a gglib endpoint does.
async fn held_by_gglib(address: &str) -> bool {
    let Ok(at) = address.parse::<SocketAddr>() else {
        return false;
    };
    let url = format!(
        "{}/gglib/who-holds-this-port",
        gglib_core::access::endpoint_base_url(at)
    );
    let Ok(client) = gglib_proxy::loopback::client_builder()
        .timeout(ASK_WHO)
        .build()
    else {
        return false;
    };
    let Ok(answer) = client.get(url).send().await else {
        return false;
    };
    answer
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|body| body["error"]["code"].as_str().map(str::to_owned))
        .is_some_and(|code| code == "unknown_route")
}

#[cfg(test)]
#[path = "proxy_refusal_tests.rs"]
mod tests;
