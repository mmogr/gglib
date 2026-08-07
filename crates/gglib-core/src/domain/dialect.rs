//! Tool-call dialect specification.
//!
//! A [`DialectSpec`] describes, as plain data, how a model wraps tool calls
//! inside its text output: the envelope markers, the body encodings to try,
//! and the whitespace layout it uses when emitting a call. It is the single
//! source of truth consumed by every layer that must agree on a dialect:
//!
//! - the delimited stream parser (`normalize::parsers::delimited`) scans for
//!   `tool_open`/`tool_close` and decodes bodies via `body_codecs`;
//! - the decode-time GBNF grammar (`request_pipeline::constrain`) is
//!   generated from the same markers and [`EmissionProfile`];
//! - detection (`gglib-gguf`) produces specs — either derived from the
//!   model's own chat template or the [`DialectSpec::qwen_xml`] builtin.
//!
//! Because parser, grammar, and detection all read one value, they cannot
//! drift: anything the grammar permits, the parser can parse, provable via
//! [`DialectSpec::render_call`].
//!
//! Specs are persisted per model (JSON in the `dialect_spec` column), so the
//! serde shape is forward-compatible: later-added fields must carry
//! `#[serde(default)]`.

use serde::{Deserialize, Serialize};

/// Spec `id` of the built-in Qwen/Hermes `<tool_call>` dialect.
pub const QWEN_XML_DIALECT_ID: &str = "qwen-xml";

/// Spec `id` for dialects derived from a model's chat template.
pub const DERIVED_DIALECT_ID: &str = "derived";

/// Synthetic tool-call ID prefix used by template-derived specs.
pub const DERIVED_ID_PREFIX: &str = "call_dialect_";

/// Synthetic tool-call ID prefix used by the built-in Qwen dialect.
///
/// Kept Qwen-branded for continuity with pre-spec releases.
pub const QWEN_ID_PREFIX: &str = "call_qwen_";

/// How a tool-call body is encoded between the envelope markers.
///
/// Codec *internals* (key names, inner XML markers) are properties of the
/// codec itself, invariant across models that use it, and live in the parser
/// — a spec only selects which codecs apply and in what order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyCodec {
    /// A `{"name": ..., "arguments": {...}}` JSON object (Qwen 2/2.5,
    /// Hermes, and most template-derived dialects).
    Json,
    /// One or more `<function=NAME><parameter=KEY>VALUE</parameter>...`
    /// blocks (Qwen 3 under `--jinja`, Hermes-style).
    FunctionXml,
}

/// Whitespace layout a dialect uses when emitting a call.
///
/// Consumed by the GBNF grammar generator and by
/// [`DialectSpec::render_call`], so enforcement and tests emit exactly what
/// the model was trained to produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmissionProfile {
    /// Whether a newline separates the open marker from the body.
    pub newline_after_open: bool,
    /// Whether a newline separates the body from the close marker.
    pub newline_before_close: bool,
}

impl Default for EmissionProfile {
    /// Newlines on both sides — the layout shared by every dialect observed
    /// so far (`<tool_call>\n{...}\n</tool_call>`).
    fn default() -> Self {
        Self {
            newline_after_open: true,
            newline_before_close: true,
        }
    }
}

fn default_id_prefix() -> String {
    DERIVED_ID_PREFIX.to_owned()
}

/// A model's tool-call dialect, described entirely as data.
///
/// See the module docs for the consumer contract. `tool_open == tool_close`
/// is a valid spec (fenced dialects); an empty marker is not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialectSpec {
    /// Stable identifier — [`QWEN_XML_DIALECT_ID`] for the builtin,
    /// [`DERIVED_DIALECT_ID`] for template-derived specs.
    pub id: String,
    /// Marker that opens a tool-call envelope, e.g. `<tool_call>`.
    pub tool_open: String,
    /// Marker that closes a tool-call envelope, e.g. `</tool_call>`.
    pub tool_close: String,
    /// Body encodings to try, in order, against the envelope contents.
    pub body_codecs: Vec<BodyCodec>,
    /// Whitespace layout for emitted calls (grammar + [`Self::render_call`]).
    #[serde(default)]
    pub emission: EmissionProfile,
    /// Prefix for synthesized tool-call IDs, e.g. `call_qwen_`.
    #[serde(default = "default_id_prefix")]
    pub id_prefix: String,
}

impl DialectSpec {
    /// The built-in Qwen 2 / 2.5 / 3 (and Hermes-family) dialect:
    /// `<tool_call>` envelope with a JSON body, falling back to the
    /// `<function=...>` inner-XML body Qwen 3 emits under `--jinja`.
    #[must_use]
    pub fn qwen_xml() -> Self {
        Self {
            id: QWEN_XML_DIALECT_ID.to_owned(),
            tool_open: "<tool_call>".to_owned(),
            tool_close: "</tool_call>".to_owned(),
            body_codecs: vec![BodyCodec::Json, BodyCodec::FunctionXml],
            emission: EmissionProfile::default(),
            id_prefix: QWEN_ID_PREFIX.to_owned(),
        }
    }

    /// Whether the JSON body codec applies — the precondition for GBNF
    /// grammar enforcement, which can only originate JSON-shaped bodies.
    #[must_use]
    pub fn supports_json_body(&self) -> bool {
        self.body_codecs.contains(&BodyCodec::Json)
    }

    /// Render one canonical tool call exactly as the grammar would enforce
    /// it: envelope markers, [`EmissionProfile`] newlines, and a JSON body
    /// with `name` before `arguments`.
    ///
    /// This is the bridge that proves grammar and parser share one source:
    /// tests feed `render_call` output through the parser and require the
    /// call to round-trip.
    #[must_use]
    pub fn render_call(&self, name: &str, arguments: &serde_json::Value) -> String {
        let after_open = if self.emission.newline_after_open {
            "\n"
        } else {
            ""
        };
        let before_close = if self.emission.newline_before_close {
            "\n"
        } else {
            ""
        };
        let name_json = serde_json::Value::String(name.to_owned());
        format!(
            "{}{}{{\"name\": {}, \"arguments\": {}}}{}{}",
            self.tool_open, after_open, name_json, arguments, before_close, self.tool_close
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builtin_qwen_spec_shape() {
        let spec = DialectSpec::qwen_xml();
        assert_eq!(spec.id, QWEN_XML_DIALECT_ID);
        assert_eq!(spec.tool_open, "<tool_call>");
        assert_eq!(spec.tool_close, "</tool_call>");
        assert_eq!(
            spec.body_codecs,
            vec![BodyCodec::Json, BodyCodec::FunctionXml]
        );
        assert_eq!(spec.id_prefix, QWEN_ID_PREFIX);
        assert!(spec.supports_json_body());
    }

    #[test]
    fn function_xml_only_spec_has_no_json_body() {
        let spec = DialectSpec {
            body_codecs: vec![BodyCodec::FunctionXml],
            ..DialectSpec::qwen_xml()
        };
        assert!(!spec.supports_json_body());
    }

    #[test]
    fn render_call_matches_the_qwen_emission_shape() {
        let spec = DialectSpec::qwen_xml();
        let emission = spec.render_call("read_file", &json!({"path": "a.rs"}));
        assert_eq!(
            emission,
            "<tool_call>\n{\"name\": \"read_file\", \"arguments\": {\"path\":\"a.rs\"}}\n</tool_call>"
        );
    }

    #[test]
    fn render_call_honors_the_emission_profile() {
        let spec = DialectSpec {
            emission: EmissionProfile {
                newline_after_open: false,
                newline_before_close: false,
            },
            ..DialectSpec::qwen_xml()
        };
        let emission = spec.render_call("f", &json!({}));
        assert_eq!(
            emission,
            "<tool_call>{\"name\": \"f\", \"arguments\": {}}</tool_call>"
        );
    }

    #[test]
    fn render_call_json_escapes_the_name() {
        let spec = DialectSpec::qwen_xml();
        let emission = spec.render_call("we\"ird", &json!({}));
        assert!(emission.contains(r#""we\"ird""#));
    }

    #[test]
    fn serde_round_trip_preserves_every_field() {
        let spec = DialectSpec {
            id: DERIVED_DIALECT_ID.to_owned(),
            tool_open: "«TC»".to_owned(),
            tool_close: "«/TC»".to_owned(),
            body_codecs: vec![BodyCodec::Json],
            emission: EmissionProfile {
                newline_after_open: false,
                newline_before_close: true,
            },
            id_prefix: DERIVED_ID_PREFIX.to_owned(),
        };
        let json = serde_json::to_string(&spec).unwrap();
        let back: DialectSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back, spec);
    }

    #[test]
    fn deserialize_tolerates_missing_defaulted_fields() {
        // A row persisted by an older build that predates `emission` /
        // `id_prefix` must still deserialize.
        let json = r#"{
            "id": "qwen-xml",
            "tool_open": "<tool_call>",
            "tool_close": "</tool_call>",
            "body_codecs": ["json", "function_xml"]
        }"#;
        let spec: DialectSpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.emission, EmissionProfile::default());
        assert_eq!(spec.id_prefix, DERIVED_ID_PREFIX);
    }

    #[test]
    fn deserialize_tolerates_unknown_fields() {
        // A row persisted by a *newer* build with extra fields must not
        // fail on an older reader.
        let json = r#"{
            "id": "derived",
            "tool_open": "A",
            "tool_close": "B",
            "body_codecs": ["json"],
            "reasoning_open": "<think>"
        }"#;
        let spec: DialectSpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.tool_open, "A");
    }
}
