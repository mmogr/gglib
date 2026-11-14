//! One key of `gglib config`, put on the wire type the settings API reads.
//!
//! `set` and `unset` both write through [`UpdateSettingsRequest`], the type
//! the HTTP handler reads, so the CLI and the app change a setting by the
//! identical path. The keys are read from that type rather than listed
//! here: serialising a default request emits one `null` per field, because
//! `double_option` has no `skip_serializing_if`, so the type is its own
//! manifest and cannot drift from what `update` accepts.

use anyhow::{Result, bail};
use gglib_app_services::types::UpdateSettingsRequest;
use serde_json::{Map, Value};

use super::settings_display::kebab_to_camel;
use crate::config_commands::SettingKey;

/// Every key the wire type carries, as it spells them.
pub(super) fn known_keys() -> Map<String, Value> {
    let probe = serde_json::to_value(UpdateSettingsRequest::default())
        .expect("UpdateSettingsRequest always serialises");
    match probe {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// The request that writes `value` to `key`: an explicit `null` is what
/// `double_option` reads as "clear this", and every other key is omitted,
/// which leaves it alone.
///
/// Refused for a key the wire type does not carry, which serde would
/// otherwise ignore, writing nothing in silence; and for a value its field
/// cannot hold, such as a port above 65535.
pub(super) fn request(key: &SettingKey, value: Value) -> Result<UpdateSettingsRequest> {
    let camel = kebab_to_camel(key.name);
    if !known_keys().contains_key(&camel) {
        bail!("`{}` is not a setting the settings API carries", key.name);
    }
    let body = Value::Object(std::iter::once((camel, value)).collect());
    Ok(serde_json::from_value(body)?)
}
