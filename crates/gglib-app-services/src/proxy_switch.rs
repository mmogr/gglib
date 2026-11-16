//! What a running endpoint was started as, how a start that asks for
//! something else differs from it, and the words for both.
//!
//! A start that finds the endpoint on is one of three things: the same
//! endpoint asked for again, which changes nothing; a different one, which
//! the caller is told about and may ask for again with `replace`; or that
//! replacement, which [`ProxyOps::start_as`](crate::ProxyOps::start_as)
//! carries out as one operation. The comparison and its words live here so
//! the service, the CLI's question and the app say the same thing.

use std::path::PathBuf;

use gglib_core::domain::InferenceConfig;
use gglib_core::ports::PinnedSpec;
use gglib_proxy::slot_eviction::DiskBudget;
use gglib_runtime::proxy::ProxyConfig;
use serde::{Deserialize, Serialize};

/// The parts of a start a running endpoint keeps for its whole run: what a
/// second start is compared by.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProxyShape {
    host: String,
    /// The port asked for, or the one bound for a running endpoint. `0`
    /// asks for any port.
    port: u16,
    default_context: Option<u64>,
    cache_enabled: bool,
    slot_dir: Option<PathBuf>,
    disk_budget: DiskBudget,
    inference_override: Option<InferenceConfig>,
    default_profile: Option<String>,
    api_key: Option<String>,
    allowed_hosts: Vec<String>,
    /// The pinned model's id, name and launch options (as JSON, which is
    /// how they compare); `None` serves every model.
    pin: Option<(i64, String, serde_json::Value)>,
}

impl ProxyShape {
    /// The shape `config` and `pin` ask for, at `port`.
    pub(crate) fn of(config: &ProxyConfig, pin: Option<&PinnedSpec>, port: u16) -> Self {
        Self {
            host: config.host.clone(),
            port,
            default_context: config.default_context,
            cache_enabled: config.cache_enabled,
            slot_dir: config.slot_dir.clone(),
            disk_budget: config.disk_budget,
            inference_override: config.inference_override.clone(),
            default_profile: config.default_profile.clone(),
            api_key: config.api_key.clone(),
            allowed_hosts: config.allowed_hosts.clone(),
            pin: pin.map(|p| {
                (
                    p.id,
                    p.name.clone(),
                    serde_json::to_value(&p.launch_overrides).unwrap_or_default(),
                )
            }),
        }
    }
}

/// How a start that found the endpoint on differs from what runs, in words:
/// what the endpoint is now and what switching it would do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointChange {
    /// What runs, after "The endpoint is": `on at port 8080, serving every
    /// model`.
    pub now: String,
    /// What switching does, after "to": `serve only Qwen3-4B`, `serve every
    /// model again`, `restart it with port 9929 (now 8080)`.
    pub action: String,
}

/// What a start did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum StartOutcome {
    /// The endpoint was off and this start turned it on.
    Started,
    /// The endpoint was already on as asked, and nothing changed.
    Unchanged,
    /// The endpoint was on otherwise and was switched to what was asked.
    Replaced,
}

/// What [`ProxyOps::start_as`](crate::ProxyOps::start_as) answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartAnswer {
    /// The endpoint is on as asked, at this address, by this outcome.
    On(std::net::SocketAddr, StartOutcome),
    /// The endpoint is on otherwise, and was left alone: a start without
    /// `replace`.
    Differs(EndpointChange),
}

/// What an endpoint on `port` serves, after "The endpoint is": `on at port
/// 8080, serving every model`, or `serving only <model>`.
#[must_use]
pub fn shape_words(port: u16, pinned: Option<&str>) -> String {
    pinned.map_or_else(
        || format!("on at port {port}, serving every model"),
        |model| format!("on at port {port}, serving only {model}"),
    )
}

/// How `asked` differs from the `running` shape, or `None` when it asks for
/// the endpoint as it is.
pub(crate) fn change(running: &ProxyShape, asked: &ProxyShape) -> Option<EndpointChange> {
    let pin = match (&running.pin, &asked.pin) {
        (None, Some((_, to, _))) => Some(format!("serve only {to}")),
        (Some(_), None) => Some("serve every model again".to_owned()),
        (Some((was_id, was, _)), Some((to_id, to, _))) if was_id != to_id => {
            Some(format!("serve only {to} instead of {was}"))
        }
        (Some((_, _, was)), Some((_, to, options))) if was != options => {
            Some(format!("serve only {to} with other launch options"))
        }
        _ => None,
    };
    let rest = differences(running, asked);
    let action = match (pin, rest.is_empty()) {
        (None, true) => return None,
        (Some(pin), true) => pin,
        (Some(pin), false) => format!("{pin}, with {}", joined(&rest)),
        (None, false) => format!("restart it with {}", joined(&rest)),
    };
    Some(EndpointChange {
        now: shape_words(
            running.port,
            running.pin.as_ref().map(|(_, name, _)| name.as_str()),
        ),
        action,
    })
}

/// Each way `asked` differs from `running` but the pin, in words. A key is
/// never named, only that it differs.
fn differences(running: &ProxyShape, asked: &ProxyShape) -> Vec<String> {
    let mut out = Vec::new();
    if asked.host != running.host {
        out.push(format!("address {} (now {})", asked.host, running.host));
    }
    if asked.port != 0 && asked.port != running.port {
        out.push(format!("port {} (now {})", asked.port, running.port));
    }
    if asked.default_context != running.default_context {
        out.push(format!(
            "default context {} (now {})",
            context_words(asked.default_context),
            context_words(running.default_context)
        ));
    }
    if asked.cache_enabled != running.cache_enabled {
        out.push(format!(
            "prompt cache on disk {} (now {})",
            on_off(asked.cache_enabled),
            on_off(running.cache_enabled)
        ));
    } else if asked.cache_enabled {
        if asked.slot_dir != running.slot_dir {
            out.push("a different cache folder".to_owned());
        }
        if asked.disk_budget != running.disk_budget {
            out.push("a different cache budget".to_owned());
        }
    }
    if asked.inference_override != running.inference_override {
        out.push("different sampling".to_owned());
    }
    if asked.default_profile != running.default_profile {
        out.push(format!(
            "default profile {}",
            asked.default_profile.as_deref().unwrap_or("none")
        ));
    }
    if asked.api_key != running.api_key {
        out.push("a different API key".to_owned());
    }
    if asked.allowed_hosts != running.allowed_hosts {
        out.push(if asked.allowed_hosts.is_empty() {
            "no allowed hosts".to_owned()
        } else {
            format!("allowed hosts {}", asked.allowed_hosts.join(", "))
        });
    }
    out
}

fn context_words(context: Option<u64>) -> String {
    context.map_or_else(|| "sized per launch".to_owned(), |n| n.to_string())
}

const fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

/// `a`, `a and b`, `a, b and c`.
fn joined(parts: &[String]) -> String {
    match parts {
        [] => String::new(),
        [one] => one.clone(),
        [head @ .., last] => format!("{} and {last}", head.join(", ")),
    }
}

#[cfg(test)]
#[path = "proxy_switch_tests.rs"]
mod tests;
