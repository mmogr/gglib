//! The secrets gglib stores, and the one way a stored value comes back
//! (ADR 0027).
//!
//! A read says whether a secret is held and never what it is: the CLI prints
//! [`SHOWN_AS`], the HTTP DTOs carry a flag or a list of keys. The value is
//! handed back only by [`SecretsService::reveal`](crate::services::SecretsService::reveal),
//! which `gglib config settings reveal` and `POST /api/secrets/reveal` call.

use serde::{Deserialize, Serialize};

/// What a held secret is shown as wherever gglib would otherwise print it.
pub const SHOWN_AS: &str = "(set, not shown)";

/// `value` when it holds a secret: `None` when it is absent or blank.
///
/// A blank is no secret. Settings validation refuses a blank proxy key, so
/// only a database written before it did can hold one; every reader of a
/// secret, stored or from the environment, asks this rather than spelling
/// the rule again.
#[must_use]
pub fn held(value: Option<&str>) -> Option<&str> {
    value.filter(|v| !v.trim().is_empty())
}

/// A secret kept in settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum SettingSecret {
    /// `proxy_api_key`: the bearer token the proxy, and a daemon shared on
    /// the network, demand.
    ProxyApiKey,
    /// The key in `remote_pairing`: this device's credential for the machine
    /// `gglib remote join` paired it with.
    RemotePairingKey,
}

impl SettingSecret {
    /// Every secret settings keep.
    pub const ALL: [Self; 2] = [Self::ProxyApiKey, Self::RemotePairingKey];
}

/// Each secret's name, written once: [`SettingSecret::name`] returns it and
/// [`SettingSecret::reveal_command`] is built from it at compile time, so a
/// hint cannot name a key `settings show` does not print.
macro_rules! named {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        impl SettingSecret {
            /// The dotted name `gglib config settings show` prints the
            /// secret under, and `reveal` takes.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name),+
                }
            }

            /// The command that prints the secret, bare. A caller adds the
            /// backticks or the `$(…)` its sentence needs.
            #[must_use]
            pub const fn reveal_command(self) -> &'static str {
                match self {
                    $(Self::$variant => concat!("gglib config settings reveal ", $name)),+
                }
            }
        }
    };
}

named! {
    ProxyApiKey => "proxy-api-key",
    RemotePairingKey => "remote-pairing.api-key",
}

/// Which stored secret to reveal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SecretRef {
    /// A secret kept in settings.
    Setting {
        /// Which one.
        secret: SettingSecret,
    },
    /// The value of one environment variable of an MCP server.
    McpEnv {
        /// The server's id.
        #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
        server_id: i64,
        /// The variable's name.
        key: String,
    },
}

/// What a reveal answers: the value, or `null` when nothing is stored there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct RevealedSecret {
    /// The stored value.
    pub value: Option<String>,
}

#[cfg(test)]
#[path = "secret_tests.rs"]
mod tests;
