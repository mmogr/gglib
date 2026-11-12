//! A conversation's stored settings, read and changed key by key.
//!
//! The settings are kept as one JSON object. A key this build does not know,
//! or whose value it cannot read, is passed over when they are read, and it
//! stays as it is stored when another key is changed: a write names only
//! the keys whose values it changes ([`SettingsEdit`]).

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::chat::ConversationSettings;

/// A change to some keys of a conversation's stored settings.
///
/// Each key named is set to its value, or removed for `None`. Every other
/// stored key stays as it is, one this build does not know or cannot read
/// among them. Empty changes nothing.
pub type SettingsEdit = BTreeMap<String, Option<Value>>;

impl ConversationSettings {
    /// The settings stored as `text`, each key read on its own: one this
    /// build does not know, or whose value it cannot read, is passed over,
    /// and every other is read. `None` for text that is not a JSON object.
    #[must_use]
    pub fn read(text: &str) -> Option<Self> {
        let Ok(Value::Object(stored)) = serde_json::from_str(text) else {
            return None;
        };
        if let Ok(whole) = serde_json::from_value(Value::Object(stored.clone())) {
            return Some(whole);
        }
        let readable = stored.into_iter().filter(|(key, value)| {
            let alone = Map::from_iter([(key.clone(), value.clone())]);
            serde_json::from_value::<Self>(Value::Object(alone)).is_ok()
        });
        serde_json::from_value(Value::Object(readable.collect())).ok()
    }

    /// The keys these settings change from `held`, the settings they were
    /// made from, as they are stored: a value that differs is set, and a key
    /// these do not store is removed. A key `held` did not read is not
    /// named, so it stays as it is stored.
    #[must_use]
    pub fn edit_from(&self, held: &Self) -> SettingsEdit {
        let (mut held, next) = (stored(held), stored(self));
        let mut edit: SettingsEdit = next
            .into_iter()
            .filter(|(key, value)| held.remove(key).as_ref() != Some(value))
            .map(|(key, value)| (key, Some(value)))
            .collect();
        edit.extend(held.into_iter().map(|(key, _)| (key, None)));
        edit
    }

    /// [`edit_from`](Self::edit_from), naming the model and its name whether
    /// or not they changed: the keys written beside the `model_id` the model
    /// decides, so the three are written together, even by a writer whose
    /// `held` is out of date.
    #[must_use]
    pub fn edit_with_model_from(&self, held: &Self) -> SettingsEdit {
        let mut edit = self.edit_from(held);
        let mut next = stored(self);
        for key in MODEL_KEYS {
            edit.insert(key.to_owned(), next.remove(key));
        }
        edit
    }
}

/// The keys `ConversationSettings::model` and `model_name` are stored by.
const MODEL_KEYS: [&str; 2] = ["model", "model_name"];

/// `settings` as their stored text reads back: parsed from that text, so a
/// value is the one written (`0.7`, not the `f32`'s widened `f64`).
fn stored(settings: &ConversationSettings) -> Map<String, Value> {
    serde_json::to_string(settings)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "settings_edit_tests.rs"]
mod settings_edit_tests;
