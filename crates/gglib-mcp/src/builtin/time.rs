//! `builtin:get_current_time` tool implementation.

use std::collections::HashMap;

use chrono::Utc;
use chrono_tz::Tz;
use serde_json::{Value, json};

/// Returns the current time as `{ time, timezone, format }`.
///
/// The JSON shape matches `TimeResult` in `TimeRenderer.tsx` so the frontend
/// renderer can display it without any extra parsing conventions.
pub fn get_current_time(args: &HashMap<String, Value>) -> Value {
    let tz_name = args
        .get("timezone")
        .and_then(Value::as_str)
        .unwrap_or("UTC");
    let format = args
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("human");

    let tz: Tz = tz_name.parse().unwrap_or(Tz::UTC);

    let now_local = Utc::now().with_timezone(&tz);

    let time_value: Value = match format {
        "iso" => Value::String(now_local.to_rfc3339()),
        "unix" => Value::Number(Utc::now().timestamp().into()),
        _ => Value::String(now_local.format("%A, %B %e, %Y %H:%M:%S %Z").to_string()),
    };

    json!({
        "time": time_value,
        "timezone": tz.name(),
        "format": format,
    })
}
