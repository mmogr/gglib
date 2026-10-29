//! What the recorder keeps of llama-server's answers.
//!
//! The rule: nothing the model generated and nothing rendered from a prompt
//! enters a projection. `/props` values, `--help` defaults, prompt-side
//! counts on the dateless templates, same-run deltas and booleans may. Raw
//! bodies are redacted the same way and go to a folder of their own, never
//! into the per-PR comparison. The one body kept verbatim is what
//! `/apply-template` renders for a fixed chat on a dateless template
//! ([`Recording::prompts`]): the rendering is the reading.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Map, Value, json};

use crate::pinned_engines_tests::help_options;

/// Why a reading is kept: the ADR whose criterion it reads, quoted from the
/// ADR file, how it is taken, and what a change means.
pub(super) struct Criterion {
    pub(super) adr: &'static str,
    pub(super) quote: &'static str,
    pub(super) how_read: &'static str,
    pub(super) if_moved: &'static str,
}

/// Everything one recording pass kept.
#[derive(Default)]
pub(super) struct Recording {
    /// `endpoint<TAB>key path<TAB>JSON type`, one line each.
    pub(super) shape: BTreeSet<String>,
    /// One flat scalar per reading, with its criterion if an ADR names it.
    pub(super) observed: BTreeMap<String, (Value, Option<&'static Criterion>)>,
    /// The projections other than the two above, by path under
    /// `contracts/upstream/`.
    pub(super) texts: BTreeMap<&'static str, String>,
    /// Redacted raw bodies, by file name.
    pub(super) bodies: BTreeMap<String, String>,
    /// Rendered prompts, verbatim, by file name: bodies that are not
    /// redacted, and that both passes must render alike.
    pub(super) prompts: BTreeMap<String, String>,
}

impl Recording {
    /// A reading no ADR names.
    pub(super) fn plain(&mut self, name: impl Into<String>, value: impl Into<Value>) {
        self.observed.insert(name.into(), (value.into(), None));
    }

    /// A reading an ADR's criterion names.
    pub(super) fn named(
        &mut self,
        name: impl Into<String>,
        value: impl Into<Value>,
        criterion: &'static Criterion,
    ) {
        self.observed
            .insert(name.into(), (value.into(), Some(criterion)));
    }

    /// Add `value`'s key paths and types under `endpoint`.
    pub(super) fn shape_of(&mut self, endpoint: &str, value: &Value) {
        shape(endpoint, "", value, &mut self.shape);
    }

    /// Keep `body` (JSON) redacted, as `name`.
    pub(super) fn body(&mut self, name: &str, body: &Value) {
        let mut body = body.clone();
        redact(&mut body);
        let text = serde_json::to_string_pretty(&body).expect("a body serialises");
        self.bodies.insert(name.to_owned(), text + "\n");
    }

    /// Every projection file, by path under `contracts/upstream/`.
    pub(super) fn files(&self) -> BTreeMap<&'static str, String> {
        let mut files = self.texts.clone();
        let shape = self.shape.iter().fold(String::new(), |mut text, line| {
            text.push_str(line);
            text.push('\n');
            text
        });
        files.insert("llama/shape.txt", shape);
        let observed: Map<String, Value> = self
            .observed
            .iter()
            .map(|(name, (value, criterion))| {
                let mut entry = json!({ "value": value });
                if let Some(c) = criterion {
                    entry["adr"] = json!(c.adr);
                    entry["criterion"] = json!(c.quote);
                    entry["how_read"] = json!(c.how_read);
                    entry["if_moved"] = json!(c.if_moved);
                }
                (name.clone(), entry)
            })
            .collect();
        let observed = serde_json::to_string_pretty(&Value::Object(observed)).expect("JSON");
        files.insert("llama/observed.json", observed + "\n");
        files
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn shape(endpoint: &str, path: &str, value: &Value, out: &mut BTreeSet<String>) {
    if !path.is_empty() {
        out.insert(format!("{endpoint}\t{path}\t{}", type_name(value)));
    }
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                shape(endpoint, &path, child, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                shape(endpoint, &format!("{path}[]"), item, out);
            }
        }
        _ => {}
    }
}

/// Keys whose values change per run, name a path on this machine, or were
/// generated or rendered: their value is replaced, their key kept.
const REDACTED: &[&str] = &[
    "chat_template",
    "content",
    "created",
    "generated",
    "id",
    "model",
    "model_alias",
    "model_path",
    "prompt",
    "reasoning_content",
    "system_fingerprint",
    "time_ms",
    "timings",
];

fn redact(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if REDACTED.contains(&key.as_str()) && !child.is_null() {
                    *child = match child {
                        Value::Number(_) => json!(0),
                        Value::Object(_) => json!({}),
                        _ => json!("<redacted>"),
                    };
                } else {
                    redact(child);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact),
        _ => {}
    }
}

/// A streamed body, each `data:` frame redacted as [`Recording::body`] does.
pub(super) fn redact_stream(body: &str) -> String {
    let mut out = String::new();
    for line in body.lines() {
        match line.strip_prefix("data: ") {
            Some(data) if data.starts_with('{') => {
                let mut frame: Value = serde_json::from_str(data).expect("a JSON frame");
                redact(&mut frame);
                out.push_str("data: ");
                out.push_str(&frame.to_string());
            }
            _ => out.push_str(line),
        }
        out.push('\n');
    }
    out
}

/// The `default:` a `--help` gives `flag`, as written there.
pub(super) fn help_default(help: &str, flag: &str) -> Option<String> {
    let options = help_options(help);
    let option = options.iter().find(|o| o.flags.iter().any(|f| f == flag))?;
    let text = option.text.replace('\n', " ");
    let rest = text.split("default: ").nth(1)?;
    let value = rest.split([',', ')']).next()?.trim().trim_matches('\'');
    Some(value.to_owned())
}

/// `value` as the `f32` llama.cpp holds it, so `0.800000011920929` reads `0.8`.
pub(super) fn as_f32(value: &Value) -> Value {
    let narrow: f32 = value.to_string().parse().expect("a number");
    let shortest: f64 = narrow.to_string().parse().expect("an f32 reads as an f64");
    json!(shortest)
}

/// What must never reach a committed file: a path on the recording machine,
/// or the name of the tool that recorded it there.
/// Compared without case.
const LOCAL: [&str; 4] = ["/private/", "/users/", "/home/", "claude"];

/// `text`, kept as `name`, names nothing on the machine that recorded it.
pub(super) fn assert_nothing_local(name: &str, text: &str) {
    for marker in LOCAL {
        let line = text
            .lines()
            .find(|line| line.to_lowercase().contains(marker));
        assert!(line.is_none(), "{name} would commit {marker:?}: {line:?}");
    }
}

/// Write `content` to `root/rel`, which must stay inside `root`.
pub(super) fn write_under(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    assert!(
        path.starts_with(root) && !rel.contains(".."),
        "{} is outside {}",
        path.display(),
        root.display()
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("make the folder");
    }
    std::fs::write(&path, content).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}

/// The lines of `have` and `want` that the other lacks, at most 40.
pub(super) fn line_diff(have: &str, want: &str) -> String {
    let had: BTreeSet<&str> = have.lines().collect();
    let wanted: BTreeSet<&str> = want.lines().collect();
    let gone = had.difference(&wanted).map(|l| format!("- {l}"));
    let new = wanted.difference(&had).map(|l| format!("+ {l}"));
    gone.chain(new).take(40).collect::<Vec<_>>().join("\n")
}
