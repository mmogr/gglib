//! `gglib config settings unset <key>` — return one setting to unset.
//!
//! Unset is a real state, not a synonym for zero or for a default. The one that
//! forced this subcommand is `default-context-size`: leaving it unset is what
//! lets each launch fit the context to the machine, and a stored number outranks
//! that fit. `settings set` cannot express it — `handle_set` builds
//! `args.default_context_size.map(Some)`, which produces `None` (leave alone) or
//! `Some(Some(v))` (write a value) and never `Some(None)` (clear) — so the only
//! way back was `settings reset`, which clears every other setting too.
//!
//! The valid keys are read from [`UpdateSettingsRequest`] rather than listed
//! here. A hand-written list is a second place to forget a field, and this
//! subcommand exists because a field was unreachable from one surface already.

use anyhow::{Result, bail};
use gglib_app_services::types::UpdateSettingsRequest;
use serde_json::{Map, Value};

use crate::bootstrap::CliContext;

/// `default-context-size` → `defaultContextSize`.
///
/// The inverse of the conversion `settings show` prints with, so the key a
/// person reads out of that output is the key this accepts.
fn kebab_to_camel(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    let mut upper_next = false;
    for ch in key.chars() {
        if ch == '-' {
            upper_next = true;
        } else if upper_next {
            out.push(ch.to_ascii_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Every settable key, as the wire type spells them.
///
/// Serialising a default request emits one `null` per field, because
/// `double_option` has no `skip_serializing_if` — so the type is its own
/// manifest and cannot drift from what `update` will accept.
fn known_keys() -> Map<String, Value> {
    let probe = serde_json::to_value(UpdateSettingsRequest::default())
        .expect("UpdateSettingsRequest always serialises");
    match probe {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// Convert a camelCase key back to the kebab-case a person typed.
fn camel_to_kebab(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i != 0 {
            out.push('-');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

pub(super) async fn handle_unset(ctx: &CliContext, key: &str) -> Result<()> {
    let camel = kebab_to_camel(key);
    let known = known_keys();
    if !known.contains_key(&camel) {
        let mut names: Vec<String> = known.keys().map(|k| camel_to_kebab(k)).collect();
        names.sort();
        bail!(
            "Unknown setting '{key}'. Settable keys are:\n  {}",
            names.join("\n  ")
        );
    }

    // An explicit `null` is what `double_option` reads as "clear this"; an
    // omitted key means "leave it alone". Building the request through serde
    // rather than by field keeps this generic over all of them.
    let body = Value::Object([(camel, Value::Null)].into_iter().collect());
    let request: UpdateSettingsRequest =
        serde_json::from_value(body).expect("a known key with a null value always deserialises");

    // The CLI reaches the core service, so the wire type converts on the way
    // in. Same conversion the HTTP handler uses, so both surfaces clear a
    // setting by the identical path.
    ctx.app.settings().update(request.into()).await?;
    println!("✓ {key} is now unset.");
    Ok(())
}

#[cfg(test)]
#[path = "unset_tests.rs"]
mod tests;
