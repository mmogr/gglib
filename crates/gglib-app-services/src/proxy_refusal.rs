//! Why the endpoint is off when its port is taken, in words, and what holds
//! the port when that can be told: another gglib service answers an unknown
//! route with gglib's own refusal, `unknown_route`; anything else is another
//! program.

use std::net::SocketAddr;
use std::time::Duration;

/// How long the holder of a taken port has to say it is a gglib.
const ASK_WHO: Duration = Duration::from_secs(1);

/// What holds `address`, a port a start could not bind, as the sentence the
/// endpoint's status carries while it is off.
pub(crate) async fn port_in_use(address: &str) -> String {
    let _ = held_by_gglib(address).await;
    String::new()
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
