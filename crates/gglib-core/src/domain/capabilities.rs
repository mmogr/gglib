//! Model capability detection, inference, and request transformation.
//!
//! This module owns two orthogonal pipelines that operate on different phases
//! of a model request:
//!
//! ## 1. Request-side capability pipeline
//!
//! Before a chat-completion request is forwarded to llama-server the proxy
//! consults the stored [`ModelCapabilities`] flags to decide whether to rewrite
//! the message list.  Flags are inferred at import time and stored in the
//! database; they can also be overridden at any time via the API or CLI.
//!
//! | Layer | Function | When it fires |
//! |---|---|---|
//! | Template analysis | [`infer_from_chat_template`] | At model import — reads `tokenizer.chat_template` from the GGUF |
//! | Architecture registry | [`capabilities_from_architecture`] | At model import — reads `general.architecture` as a backstop when the GGUF ships without a chat template |
//! | Request rewriting | [`transform_messages_for_capabilities`] | At proxy time — merges consecutive same-role messages for models that require strict turn alternation |
//!
//! The result of Layer 1 and Layer 2 is **OR-combined** and stored in
//! `Model.capabilities`.  The proxy reads this value once per request via a
//! single catalog lookup.
//!
//! ## 2. Response-side normalization pipeline
//!
//! Separate from request rewriting, some models (e.g., Qwen) embed tool-call
//! JSON inside XML tags in the response text.  This is handled by the
//! `format:*` tag pipeline in `gglib-proxy::normalize`, which is entirely
//! independent from `ModelCapabilities`.
//!
//! ## Template analysis — positive vs. negative signals
//!
//! [`infer_from_chat_template`] uses two kinds of signals for system-role
//! detection, evaluated in priority order:
//!
//! | Priority | Signal | Example pattern | Conclusion |
//! |---|---|---|---|
//! | **1 (positive)** | `[SYSTEM_PROMPT]` in template | Mistral v7 | `SUPPORTS_SYSTEM_ROLE` set |
//! | **1 (positive)** | `[AVAILABLE_TOOLS]` in template | Mistral v3/v3-tekken | `SUPPORTS_SYSTEM_ROLE` set |
//! | **2 (negative)** | `"Only user, assistant and tool roles…"` | Old Mistral v1/v2 | `SUPPORTS_SYSTEM_ROLE` not set |
//! | **2 (negative)** | `"got system"` / `"Raise exception"` | Other strict models | `SUPPORTS_SYSTEM_ROLE` not set |
//! | **default** | No signal found | Generic template | `SUPPORTS_SYSTEM_ROLE` set |
//!
//! Positive evidence takes precedence: if `[SYSTEM_PROMPT]` or `[AVAILABLE_TOOLS]`
//! appears, the negative patterns are ignored for system-role purposes.  This
//! matters because some Jinja templates contain both an error-raise branch for
//! unknown roles AND a valid system branch guarded by `[SYSTEM_PROMPT]`.
//!
//! ## Architecture registry
//!
//! [`capabilities_from_architecture`] maps GGUF `general.architecture` strings
//! to [`ModelCapabilities`] flags.  This is the **backstop** for models whose
//! quantized builds strip the `tokenizer.chat_template` section, making
//! `infer_from_chat_template` return `empty()`.
//!
//! | Architecture string | Models | Flags |
//! |---|---|---|
//! | `"mistral"` | Mistral v1/v2 (old) | `REQUIRES_STRICT_TURNS` |
//! | `"mistral3"` | Devstral, Ministral, Mistral Small 3 | `REQUIRES_STRICT_TURNS \| SUPPORTS_SYSTEM_ROLE` |
//!
//! **To add a new architecture:**
//!
//! 1. Add a match arm in [`capabilities_from_architecture`] mapping the
//!    architecture string to the appropriate flags.
//! 2. Add a unit test in the `#[cfg(test)]` block at the bottom of this file.
//! 3. If the architecture also needs **response-side** normalization (XML tool
//!    calls, custom reasoning tags, etc.), follow the steps in `CONTRIBUTING.md`
//!    under "Adding a new model architecture" to add a `format:*` parser as well.
//! 4. No other files need touching — all call sites already use these functions.
//!
//! **Note on Qwen:** Qwen is intentionally absent from the registry.  Qwen's
//! quantized builds always ship a full chat template, so
//! [`infer_from_chat_template`] handles the request side.  Its response-side
//! `<tool_call>` XML is handled by the `format:qwen-xml` tag pipeline.

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

bitflags! {
    /// Model capabilities inferred from chat template analysis.
    ///
    /// These flags describe what the model's chat template can handle.
    /// Absence means "we don't know" or "not needed", not "forbidden".
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    #[repr(transparent)]
    pub struct ModelCapabilities: u32 {
        /// Model supports system role natively in its chat template.
        ///
        /// When set: system messages can be passed through unchanged.
        /// When unset: system messages must be converted to user messages.
        const SUPPORTS_SYSTEM_ROLE    = 0b0000_0001;

        /// Model requires strict user/assistant alternation.
        ///
        /// When set: consecutive messages of same role must be merged.
        /// When unset: message order can be arbitrary (OpenAI-style).
        const REQUIRES_STRICT_TURNS   = 0b0000_0010;

        /// Model supports tool/function calling.
        ///
        /// When set: tool_calls and tool role messages are supported.
        /// When unset: tool functionality should not be used.
        const SUPPORTS_TOOL_CALLS     = 0b0000_0100;

        /// Model has reasoning/thinking capability.
        ///
        /// When set: model may produce <think> tags or reasoning_content.
        /// When unset: model produces only standard responses.
        const SUPPORTS_REASONING      = 0b0000_1000;
    }
}

impl Default for ModelCapabilities {
    /// Default capabilities represent "unknown" state.
    ///
    /// Models start with empty capabilities and must be explicitly inferred.
    /// This prevents incorrect assumptions about model constraints.
    fn default() -> Self {
        Self::empty()
    }
}

impl Serialize for ModelCapabilities {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.bits().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ModelCapabilities {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bits = u32::deserialize(deserializer)?;
        Ok(Self::from_bits_truncate(bits))
    }
}

impl ModelCapabilities {
    /// Check if model supports system role.
    pub const fn supports_system_role(self) -> bool {
        self.contains(Self::SUPPORTS_SYSTEM_ROLE)
    }

    /// Check if model requires strict user/assistant alternation.
    pub const fn requires_strict_turns(self) -> bool {
        self.contains(Self::REQUIRES_STRICT_TURNS)
    }

    /// Check if model supports tool/function calls.
    pub const fn supports_tool_calls(self) -> bool {
        self.contains(Self::SUPPORTS_TOOL_CALLS)
    }

    /// Check if model supports reasoning phases.
    pub const fn supports_reasoning(self) -> bool {
        self.contains(Self::SUPPORTS_REASONING)
    }
}

/// Infer model capabilities from chat template Jinja source and model name.
///
/// Uses string heuristics to detect template constraints. Returns safe
/// defaults if template is missing or unparseable.
///
/// # Detection Strategy
///
/// Two-layer approach:
/// - **Layer 1 (Metadata)**: Check chat template for reliable signals (preferred)
/// - **Layer 2 (Name Heuristics)**: Use model name patterns as fallback when metadata is missing
///
/// # Capabilities Detected
///
/// - **System role**: Positive signals (`[SYSTEM_PROMPT]`, `[AVAILABLE_TOOLS]`) take precedence over
///   negative signals (explicit rejection messages).  Generic templates with neither signal default
///   to `SUPPORTS_SYSTEM_ROLE` set.
/// - **Strict turns**: Looks for alternation enforcement logic (`ns.index % 2`,
///   `conversation roles must alternate`, etc.)
/// - **Tool calling**: Checks for `<tool_call>`, `if tools`, `function_call` patterns (metadata);
///   falls back to model name patterns like "hermes", "functionary" (heuristic)
/// - **Reasoning**: Checks for `<think>`, `<reasoning>`, `enable_thinking` (metadata);
///   falls back to model name patterns like "deepseek-r1", "qwq", "o1" (heuristic)
///
/// # Fallback Behavior
///
/// Missing or unparseable templates default to empty capabilities (unknown state).
pub fn infer_from_chat_template(
    template: Option<&str>,
    model_name: Option<&str>,
) -> ModelCapabilities {
    let mut caps = ModelCapabilities::empty();

    // ─────────────────────────────────────────────────────────────────────────────
    // Layer 1: Metadata-based detection (chat template analysis)
    // ─────────────────────────────────────────────────────────────────────────────

    let mut tool_detected_from_metadata = false;
    let mut reasoning_detected_from_metadata = false;

    if let Some(template) = template {
        // ── System role detection ───────────────────────────────────────────
        //
        // Positive evidence (Mistral v7 / v3-tekken) takes precedence over any
        // negative error-raise patterns.  Some templates contain both a
        // `[SYSTEM_PROMPT]` branch AND a generic "unsupported role" catch-all,
        // so we must check positive signals first.
        //
        // Sources:
        //   llama.cpp/src/llama-chat.cpp — `tmpl_contains("[SYSTEM_PROMPT]")` →
        //     LLM_CHAT_TEMPLATE_MISTRAL_V7; system role handled natively.
        //   `[AVAILABLE_TOOLS]` → LLM_CHAT_TEMPLATE_MISTRAL_V3; system prepended inline.
        let supports_system_positive =
            template.contains("[SYSTEM_PROMPT]") || template.contains("[AVAILABLE_TOOLS]");

        let forbids_system = !supports_system_positive
            && (template.contains("Only user, assistant and tool roles are supported")
                || template.contains("got system")
                || template.contains("Raise exception for unsupported roles"));

        if !forbids_system {
            caps |= ModelCapabilities::SUPPORTS_SYSTEM_ROLE;
        }

        // Check for strict alternation requirements
        // Mistral-style templates enforce user/assistant alternation with modulo checks
        let requires_alternation = template.contains("must alternate user and assistant")
            || template.contains("conversation roles must alternate")
            || template.contains("ns.index % 2");

        if requires_alternation {
            caps |= ModelCapabilities::REQUIRES_STRICT_TURNS;
        }

        // Detect tool calling support from template
        let has_tool_patterns = template.contains("<tool_call>")
            || template.contains("<|python_tag|>")
            || template.contains("if tools")
            || template.contains("tools is defined")
            || template.contains("tool_calls")
            || template.contains("function_call");

        if has_tool_patterns {
            caps |= ModelCapabilities::SUPPORTS_TOOL_CALLS;
            tool_detected_from_metadata = true;
        }

        // Detect reasoning/thinking support from template
        let has_reasoning_patterns = template.contains("<think>")
            || template.contains("</think>")
            || template.contains("<reasoning>")
            || template.contains("</reasoning>")
            || template.contains("enable_thinking")
            || template.contains("thinking_forced_open")
            || template.contains("reasoning_content");

        if has_reasoning_patterns {
            caps |= ModelCapabilities::SUPPORTS_REASONING;
            reasoning_detected_from_metadata = true;
        }
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Layer 2: Name-based heuristic fallback (when metadata is inconclusive)
    // ─────────────────────────────────────────────────────────────────────────────
    //
    // Only use name patterns when chat template didn't provide clear evidence.
    // This is less reliable but helps with models that have incomplete metadata.

    if let Some(name) = model_name {
        let name_lower = name.to_lowercase();

        // Heuristic: Tool calling support based on model name
        if !tool_detected_from_metadata {
            let has_tool_name = name_lower.contains("hermes")
                || name_lower.contains("functionary")
                || name_lower.contains("firefunction")
                || name_lower.contains("gorilla");

            if has_tool_name {
                caps |= ModelCapabilities::SUPPORTS_TOOL_CALLS;
            }
        }

        // Heuristic: Reasoning support based on model name
        if !reasoning_detected_from_metadata {
            let has_reasoning_name = name_lower.contains("deepseek-r1")
                || name_lower.contains("qwq")
                || name_lower.contains("-r1-")
                || name_lower.contains("o1");

            if has_reasoning_name {
                caps |= ModelCapabilities::SUPPORTS_REASONING;
            }
        }
    }

    caps
}

/// Map a GGUF `general.architecture` value to its inherent [`ModelCapabilities`].
///
/// This is the **single source of truth** for architecture-level behavioural
/// constraints that apply to the **request** side (message preprocessing).
/// It is consulted during model registration alongside
/// [`infer_from_chat_template`] — the two results are `OR`-ed together so that
/// either signal is sufficient.
///
/// # Scope: request preprocessing only
///
/// This registry governs `ModelCapabilities` flags (strict-turn coalescing,
/// system-role conversion, etc.).  It does **not** handle response-stream
/// dialect normalization — that is a separate concern handled by the
/// `GgufCapabilities.extensions` → `format:*` tag → `get_parser()` pipeline
/// in `gglib-core::normalize::registry`.
///
/// For example:
/// - **Qwen** tool-call XML normalization already flows through
///   `detect_tool_support()` → `extensions.insert("format:qwen-xml")` →
///   `to_tags()` → `get_parser()` → `QwenXmlParser`.  Qwen's chat template
///   always contains `<tool_call>` patterns, so `infer_from_chat_template`
///   (Layer 1) sets `SUPPORTS_TOOL_CALLS` reliably.  No architecture entry
///   is needed here for Qwen.
/// - **Mistral** does need an entry: its templates enforce strict alternation,
///   but many quantised builds ship with the tokenizer section stripped, so
///   the template layer produces no signal.  `general.architecture = "mistral"`
///   is always present and provides the necessary backstop.
///
/// # Rationale
///
/// Some models ship without a parseable `tokenizer.chat_template` in the GGUF
/// (stripped quantisation builds, partial uploads).  The chat-template layer
/// then returns `ModelCapabilities::empty()`, silently leaving constraints
/// unapplied.  Reading `general.architecture` from the GGUF gives us a
/// ground-truth signal that is always present and never varies by quantisation.
///
/// # Adding a new architecture
///
/// 1. Add a new `"arch_name" => { … }` arm below.
/// 2. Add a corresponding unit test in the `#[cfg(test)]` block.
/// 3. No other file needs touching — all call sites use this function.
///
/// # Arguments
///
/// * `arch` — value of the `general.architecture` GGUF key
///   (e.g. `"mistral"`, `"llama"`, `"qwen2"`).  `None` means the key was
///   absent; returns `empty()` so the model gets pass-through treatment.
#[must_use]
pub fn capabilities_from_architecture(arch: Option<&str>) -> ModelCapabilities {
    let Some(arch) = arch else {
        return ModelCapabilities::empty();
    };

    match arch {
        // Old Mistral v1/v2 — strict alternation, no system role.
        // Many quantised builds strip the tokenizer section, so the template
        // layer is blind; this entry is the request-side backstop.
        "mistral" => ModelCapabilities::REQUIRES_STRICT_TURNS,

        // Newer Mistral-family models (Devstral, Ministral, Mistral Small 3).
        // Architecture string changed from `"mistral"` to `"mistral3"` when
        // Mistral adopted mistral-common / Tekken tokeniser.  These models
        // support system role via `[SYSTEM_PROMPT]…[/SYSTEM_PROMPT]` tokens
        // (Mistral v7 chat template) but still require strict alternation.
        "mistral3" => {
            ModelCapabilities::REQUIRES_STRICT_TURNS | ModelCapabilities::SUPPORTS_SYSTEM_ROLE
        }

        // All other architectures: no request-side constraints inferred from
        // architecture alone.  Chat-template analysis may still set flags,
        // and response-stream normalization is handled by the format:* tag
        // pipeline independently.
        _ => ModelCapabilities::empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_capabilities() {
        let caps = ModelCapabilities::default();
        // Default is "unknown" - no capabilities set
        assert!(caps.is_empty());
        assert!(!caps.supports_system_role());
        assert!(!caps.requires_strict_turns());
        assert!(!caps.supports_tool_calls());
        assert!(!caps.supports_reasoning());
    }

    #[test]
    fn test_infer_openai_style() {
        let template = r"
            {% for message in messages %}
                {{ message.role }}: {{ message.content }}
            {% endfor %}
        ";
        let caps = infer_from_chat_template(Some(template), None);
        assert!(caps.supports_system_role());
        assert!(!caps.requires_strict_turns());
    }

    #[test]
    fn test_infer_mistral_style() {
        let template = r"
            {% if message.role == 'system' %}
                {{ raise_exception('Only user, assistant and tool roles are supported, got system.') }}
            {% endif %}
            {% if (message['role'] == 'user') != (ns.index % 2 == 0) %}
                {{ raise_exception('conversation roles must alternate user and assistant') }}
            {% endif %}
        ";
        let caps = infer_from_chat_template(Some(template), None);
        assert!(!caps.supports_system_role());
        assert!(caps.requires_strict_turns());
    }

    #[test]
    fn test_infer_missing_template() {
        let caps = infer_from_chat_template(None, None);
        // Missing template means unknown capabilities - no assumptions made
        assert!(caps.is_empty());
        assert!(!caps.supports_system_role());
    }

    #[test]
    fn test_tool_calling_from_template() {
        let template = r"
            {% if tools %}
                <tool_call>{{ message.tool_calls }}</tool_call>
            {% endif %}
        ";
        let caps = infer_from_chat_template(Some(template), None);
        assert!(caps.supports_tool_calls());
    }

    #[test]
    fn test_reasoning_from_template() {
        let template = r"
            {% if enable_thinking %}
                <think>{{ message.thinking }}</think>
            {% endif %}
        ";
        let caps = infer_from_chat_template(Some(template), None);
        assert!(caps.supports_reasoning());
    }

    #[test]
    fn test_tool_calling_name_fallback() {
        // No template, but model name suggests tool support
        let caps = infer_from_chat_template(None, Some("hermes-2-pro-7b"));
        assert!(caps.supports_tool_calls());
    }

    #[test]
    fn test_reasoning_name_fallback() {
        // No template, but model name suggests reasoning support
        let caps = infer_from_chat_template(None, Some("deepseek-r1-lite"));
        assert!(caps.supports_reasoning());
    }

    #[test]
    fn test_metadata_plus_name_fallback() {
        // Template present but has no tool markers - should still use name fallback
        let template = "simple template with no tool markers";
        let caps = infer_from_chat_template(Some(template), Some("hermes-model"));
        // Name fallback should kick in because metadata didn't detect tools
        assert!(caps.supports_tool_calls());
    }

    #[test]
    fn test_metadata_detected_skips_name_fallback() {
        // When metadata detects capability, name pattern is ignored
        let template = "<tool_call>detected</tool_call>";
        let caps = infer_from_chat_template(Some(template), Some("not-a-tool-model"));
        // Metadata detected it, so tool support is enabled regardless of name
        assert!(caps.supports_tool_calls());
    }

    #[test]
    fn test_combined_detections() {
        let template = r"
            {% if tools %}<tool_call>{{ tool }}</tool_call>{% endif %}
            <think>{{ reasoning }}</think>
        ";
        let caps = infer_from_chat_template(Some(template), None);
        assert!(caps.supports_tool_calls());
        assert!(caps.supports_reasoning());
    }

    // ─── capabilities_from_architecture ─────────────────────────────────────

    #[test]
    fn test_arch_none_returns_empty() {
        assert!(capabilities_from_architecture(None).is_empty());
    }

    #[test]
    fn test_arch_mistral_requires_strict_turns() {
        let caps = capabilities_from_architecture(Some("mistral"));
        assert!(caps.requires_strict_turns());
    }

    #[test]
    fn test_arch_llama_returns_empty() {
        assert!(capabilities_from_architecture(Some("llama")).is_empty());
    }

    #[test]
    fn test_arch_unknown_returns_empty() {
        assert!(capabilities_from_architecture(Some("future-arch-xyz")).is_empty());
    }

    #[test]
    fn test_arch_mistral3_strict_turns_and_system_role() {
        let caps = capabilities_from_architecture(Some("mistral3"));
        assert!(
            caps.requires_strict_turns(),
            "mistral3 must enforce strict turns"
        );
        assert!(
            caps.supports_system_role(),
            "mistral3 supports system via [SYSTEM_PROMPT]"
        );
    }

    #[test]
    fn test_infer_mistral_v7_supports_system() {
        // Mistral v7 Jinja template: contains [SYSTEM_PROMPT] token.
        // This is positive evidence — system role IS supported natively.
        let template = r"
            {% if messages[0].role == 'system' %}
                [SYSTEM_PROMPT]{{ messages[0].content }}[/SYSTEM_PROMPT]
            {% endif %}
            {% for message in messages %}
                {% if (message['role'] == 'user') != (loop.index0 % 2 == 0) %}
                    {{ raise_exception('conversation roles must alternate') }}
                {% endif %}
            {% endfor %}
        ";
        let caps = infer_from_chat_template(Some(template), None);
        assert!(
            caps.supports_system_role(),
            "[SYSTEM_PROMPT] is positive evidence"
        );
        assert!(caps.requires_strict_turns(), "still enforces alternation");
    }

    #[test]
    fn test_infer_mistral_v3_supports_system() {
        // Mistral v3 / v3-tekken template: contains [AVAILABLE_TOOLS] token.
        // llama.cpp prepends system content to the first user turn for these.
        let template = r"
            {% if tools is defined %}[AVAILABLE_TOOLS]{{ tools | tojson }}[/AVAILABLE_TOOLS]{% endif %}
            {% for message in messages %}
                {% if message.role == 'user' %}[INST]{{ message.content }}[/INST]
                {% elif message.role == 'assistant' %}{{ message.content }}</s>
                {% endif %}
            {% endfor %}
        ";
        let caps = infer_from_chat_template(Some(template), None);
        assert!(
            caps.supports_system_role(),
            "[AVAILABLE_TOOLS] is positive evidence"
        );
    }

    #[test]
    fn test_infer_mistral_v1_forbids_system() {
        // Old Mistral v1/v2 template: no positive tokens, explicit rejection.
        // Must NOT set SUPPORTS_SYSTEM_ROLE.
        let template = r"
            {% if message.role == 'system' %}
                {{ raise_exception('Only user, assistant and tool roles are supported, got system.') }}
            {% endif %}
        ";
        let caps = infer_from_chat_template(Some(template), None);
        assert!(
            !caps.supports_system_role(),
            "v1/v2 genuinely rejects system role"
        );
    }

    #[test]
    fn test_arch_or_template_additive() {
        // Template detects tool calls; architecture adds strict turns.
        // The two are ORed so both flags appear in the result.
        let template = "<tool_call>{{ tool }}</tool_call>";
        let from_template = infer_from_chat_template(Some(template), None);
        let from_arch = capabilities_from_architecture(Some("mistral"));
        let combined = from_template | from_arch;
        assert!(combined.supports_tool_calls(), "tool calls from template");
        assert!(combined.requires_strict_turns(), "strict turns from arch");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Message Transformation
// ─────────────────────────────────────────────────────────────────────────────

/// The content of a chat message.
///
/// The `OpenAI` API allows `content` to be either a plain string or a structured
/// array of typed content parts (text blocks, image URLs, tool results, etc.).
/// Both forms are preserved faithfully through serialize/deserialize
/// round-trips so the proxy never re-shapes data it did not need to touch.
///
/// # Serde behaviour
///
/// Uses `#[serde(untagged)]`, so the wire representation is unchanged:
/// - `Text("hello")` → `"hello"` (JSON string)
/// - `Parts([…])` → `[{"type":"text","text":"…"},…]` (JSON array)
///
/// A JSON `null` or missing `content` field is handled by the surrounding
/// `Option<MessageContent>` with `#[serde(default)]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Plain UTF-8 text.
    Text(String),
    /// Structured content parts (text, `image_url`, `tool_result`, …).
    ///
    /// Individual part shapes are defined by the `OpenAI` API spec and
    /// validated by the model, not here.
    Parts(Vec<serde_json::Value>),
}

impl MessageContent {
    /// Borrow the inner string slice when this is plain-text content.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s),
            Self::Parts(_) => None,
        }
    }

    /// Consume into a single flat `String`.
    ///
    /// For [`Text`] the string is returned as-is.  For [`Parts`] all
    /// `{"type":"text","text":"…"}` entries are concatenated; other part
    /// types (images, etc.) are omitted — callers should only use this when
    /// a plain-text representation is required (e.g. the `[System]: ` prefix
    /// during system-message conversion).
    ///
    /// [`Text`]: Self::Text
    /// [`Parts`]: Self::Parts
    pub fn into_string(self) -> String {
        match self {
            Self::Text(s) => s,
            Self::Parts(parts) => parts
                .iter()
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join(""),
        }
    }

    /// Merge `other` into `self`, producing a single combined [`MessageContent`].
    ///
    /// | `self`  | `other` | result |
    /// |---------|---------|--------|
    /// | Text    | Text    | Text joined with `"\n\n"` |
    /// | Parts   | Parts   | Parts arrays concatenated |
    /// | Text    | Parts   | Parts with a leading text block |
    /// | Parts   | Text    | Parts with a trailing text block |
    ///
    /// Empty strings are handled gracefully (no `"\n\n"` separator when
    /// either side is empty).
    fn merge_with(self, other: Self) -> Self {
        match (self, other) {
            (Self::Text(mut a), Self::Text(b)) => {
                if a.is_empty() {
                    return Self::Text(b);
                }
                if b.is_empty() {
                    return Self::Text(a);
                }
                a.push_str("\n\n");
                a.push_str(&b);
                Self::Text(a)
            }
            (Self::Parts(mut a), Self::Parts(b)) => {
                a.extend(b);
                Self::Parts(a)
            }
            (Self::Text(a), Self::Parts(b)) => {
                let mut parts = vec![serde_json::json!({"type": "text", "text": a})];
                parts.extend(b);
                Self::Parts(parts)
            }
            (Self::Parts(mut a), Self::Text(b)) => {
                a.push(serde_json::json!({"type": "text", "text": b}));
                Self::Parts(a)
            }
        }
    }
}

impl From<String> for MessageContent {
    fn from(s: String) -> Self {
        Self::Text(s)
    }
}

impl From<&str> for MessageContent {
    fn from(s: &str) -> Self {
        Self::Text(s.to_string())
    }
}

/// A chat message for transformation.
///
/// `content` uses [`MessageContent`] which accepts both a plain JSON string
/// and a JSON array of content-part objects during deserialization, preserving
/// the original form during serialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<MessageContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<serde_json::Value>,
}

impl ChatMessage {
    /// Merge `other` into `self` in-place.
    ///
    /// Used during strict-turn coalescing to combine consecutive same-role
    /// messages.  Content is merged via [`MessageContent::merge_with`]; tool
    /// calls are concatenated as JSON arrays.
    fn merge_into(&mut self, other: Self) {
        self.content = match (self.content.take(), other.content) {
            (None, b) => b,
            (a, None) => a,
            (Some(a), Some(b)) => Some(a.merge_with(b)),
        };
        match (self.tool_calls.as_mut(), other.tool_calls) {
            (_, None) => {}
            (None, tc) => self.tool_calls = tc,
            (Some(last_tc), Some(msg_tc)) => {
                if let (Some(la), Some(ma)) = (last_tc.as_array_mut(), msg_tc.as_array()) {
                    la.extend_from_slice(ma);
                }
            }
        }
    }
}

/// Merge consecutive system messages into a single message.
///
/// This is universally safe because:
/// - No model template requires multiple system messages
/// - Merging preserves all content with clear separation
/// - It prevents errors in strict-turn templates (e.g., gemma3/medgemma)
///
/// # Arguments
///
/// * `messages` - The input chat messages
///
/// # Returns
///
/// Messages with consecutive system messages merged
fn merge_consecutive_system_messages(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    if messages.is_empty() {
        return messages;
    }

    let mut result: Vec<ChatMessage> = Vec::with_capacity(messages.len());

    for msg in messages {
        let is_system_merge = result
            .last()
            .is_some_and(|last| last.role == "system" && msg.role == "system");
        if is_system_merge {
            let last = result.last_mut().unwrap();
            last.content = match (last.content.take(), msg.content) {
                (None, b) => b,
                (a, None) => a,
                (Some(a), Some(b)) => Some(a.merge_with(b)),
            };
        } else {
            result.push(msg);
        }
    }

    result
}

/// Transform chat messages based on model capabilities.
///
/// This is a pure function that applies capability-aware transformations:
/// - Merges consecutive system messages (always, for all models)
/// - Converts system messages to user messages when model doesn't support system role
/// - Merges consecutive same-role messages when model requires strict alternation
///
/// # Invariant
///
/// Consecutive system messages are ALWAYS merged, regardless of capabilities.
/// This prevents Jinja template errors in models with strict role alternation.
///
/// When capabilities are unknown (empty), only system message merging is applied.
/// This prevents degrading standard models while ensuring universal compatibility.
///
/// # Arguments
///
/// * `messages` - The input chat messages to transform
/// * `capabilities` - The model's capability flags
///
/// # Returns
///
/// Transformed messages suitable for the model's constraints
pub fn transform_messages_for_capabilities(
    mut messages: Vec<ChatMessage>,
    capabilities: ModelCapabilities,
) -> Vec<ChatMessage> {
    // STEP 0 (ALWAYS): Merge consecutive system messages.
    // This is safe for ALL models and prevents Jinja template errors
    // in models with strict role alternation (e.g., gemma3/medgemma).
    // Must run BEFORE the capabilities check to protect unknown models.
    messages = merge_consecutive_system_messages(messages);

    // Pass through if capabilities are unknown
    if capabilities.is_empty() {
        return messages;
    }

    // STEP 1: Transform system messages if the model doesn't support them
    if !capabilities.contains(ModelCapabilities::SUPPORTS_SYSTEM_ROLE) {
        for msg in &mut messages {
            if msg.role == "system" {
                msg.role = "user".to_string();
                if let Some(content) = msg.content.take() {
                    msg.content = Some(MessageContent::Text(format!(
                        "[System]: {}",
                        content.into_string()
                    )));
                }
            }
        }
    }

    // STEP 2: Merge consecutive same-role messages if strict turns are required
    if capabilities.contains(ModelCapabilities::REQUIRES_STRICT_TURNS) {
        let mut merged: Vec<ChatMessage> = Vec::new();
        for msg in messages {
            let is_mergeable = msg.role == "user" || msg.role == "assistant";
            let same_role_as_last = merged.last().is_some_and(|last| last.role == msg.role);
            if is_mergeable && same_role_as_last {
                merged.last_mut().unwrap().merge_into(msg);
            } else {
                merged.push(msg);
            }
        }
        return merged;
    }

    messages
}

#[cfg(test)]
mod transform_tests {
    use super::*;

    #[test]
    fn test_transform_unknown_passes_through_non_system() {
        // Non-system messages pass through unchanged with empty capabilities
        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: Some(MessageContent::Text("Hello".to_string())),
                tool_calls: None,
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(MessageContent::Text("Hi there".to_string())),
                tool_calls: None,
            },
        ];
        let original = messages.clone();
        let result = transform_messages_for_capabilities(messages, ModelCapabilities::empty());
        assert_eq!(result, original);
    }

    #[test]
    fn test_merges_consecutive_system_messages_always() {
        // Even with empty capabilities, consecutive system messages should merge
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: Some(MessageContent::Text(
                    "You are a helpful assistant.".to_string(),
                )),
                tool_calls: None,
            },
            ChatMessage {
                role: "system".to_string(),
                content: Some(MessageContent::Text(
                    "WORKING_MEMORY:\n- task1 (ok): done".to_string(),
                )),
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some(MessageContent::Text("Hello".to_string())),
                tool_calls: None,
            },
        ];
        let result = transform_messages_for_capabilities(messages, ModelCapabilities::empty());

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, "system");
        assert_eq!(
            result[0].content.as_ref().and_then(|c| c.as_str()),
            Some("You are a helpful assistant.\n\nWORKING_MEMORY:\n- task1 (ok): done")
        );
        assert_eq!(result[1].role, "user");
    }

    #[test]
    fn test_merges_three_consecutive_system_messages() {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: Some(MessageContent::Text("First.".to_string())),
                tool_calls: None,
            },
            ChatMessage {
                role: "system".to_string(),
                content: Some(MessageContent::Text("Second.".to_string())),
                tool_calls: None,
            },
            ChatMessage {
                role: "system".to_string(),
                content: Some(MessageContent::Text("Third.".to_string())),
                tool_calls: None,
            },
        ];
        let result = transform_messages_for_capabilities(messages, ModelCapabilities::empty());

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].content.as_ref().and_then(|c| c.as_str()),
            Some("First.\n\nSecond.\n\nThird.")
        );
    }

    #[test]
    fn test_handles_empty_system_content() {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: Some(MessageContent::Text(String::new())),
                tool_calls: None,
            },
            ChatMessage {
                role: "system".to_string(),
                content: Some(MessageContent::Text("Actual content".to_string())),
                tool_calls: None,
            },
        ];
        let result = transform_messages_for_capabilities(messages, ModelCapabilities::empty());

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].content.as_ref().and_then(|c| c.as_str()),
            Some("Actual content")
        );
    }

    #[test]
    fn test_transform_system_to_user() {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: Some(MessageContent::Text("You are helpful".to_string())),
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some(MessageContent::Text("Hello".to_string())),
                tool_calls: None,
            },
        ];
        // Use REQUIRES_STRICT_TURNS which doesn't support system but doesn't merge different roles
        let caps = ModelCapabilities::REQUIRES_STRICT_TURNS;
        let result = transform_messages_for_capabilities(messages, caps);
        // System becomes user, both messages remain separate (user + user but different content)
        assert_eq!(result.len(), 1); // They get merged because both are now "user"
        assert_eq!(result[0].role, "user");
        let content_str = result[0].content.as_ref().and_then(|c| c.as_str()).unwrap();
        assert!(content_str.contains("[System]: You are helpful"));
        assert!(content_str.contains("Hello"));
    }

    #[test]
    fn test_transform_preserves_system_when_supported() {
        let messages = vec![ChatMessage {
            role: "system".to_string(),
            content: Some(MessageContent::Text("You are helpful".to_string())),
            tool_calls: None,
        }];
        let caps = ModelCapabilities::SUPPORTS_SYSTEM_ROLE;
        let result = transform_messages_for_capabilities(messages, caps);
        assert_eq!(result[0].role, "system");
        assert_eq!(
            result[0].content,
            Some(MessageContent::Text("You are helpful".to_string()))
        );
    }

    #[test]
    fn test_transform_merges_consecutive_user_messages() {
        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: Some(MessageContent::Text("First".to_string())),
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some(MessageContent::Text("Second".to_string())),
                tool_calls: None,
            },
        ];
        let caps = ModelCapabilities::REQUIRES_STRICT_TURNS;
        let result = transform_messages_for_capabilities(messages, caps);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].content,
            Some(MessageContent::Text("First\n\nSecond".to_string()))
        );
    }

    #[test]
    fn test_transform_does_not_merge_tool_messages() {
        let messages = vec![
            ChatMessage {
                role: "tool".to_string(),
                content: Some(MessageContent::Text("Result 1".to_string())),
                tool_calls: None,
            },
            ChatMessage {
                role: "tool".to_string(),
                content: Some(MessageContent::Text("Result 2".to_string())),
                tool_calls: None,
            },
        ];
        let caps = ModelCapabilities::REQUIRES_STRICT_TURNS;
        let result = transform_messages_for_capabilities(messages, caps);
        assert_eq!(result.len(), 2); // Should not merge tool messages
    }

    #[test]
    fn test_transform_combined_system_and_merge() {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: Some(MessageContent::Text("Be helpful".to_string())),
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some(MessageContent::Text("First".to_string())),
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some(MessageContent::Text("Second".to_string())),
                tool_calls: None,
            },
        ];
        let caps = ModelCapabilities::REQUIRES_STRICT_TURNS; // No system support + strict turns
        let result = transform_messages_for_capabilities(messages, caps);
        assert_eq!(result.len(), 1); // System→user + merge
        assert_eq!(result[0].role, "user");
        let content_str = result[0].content.as_ref().and_then(|c| c.as_str()).unwrap();
        assert!(content_str.contains("[System]: Be helpful"));
        assert!(content_str.contains("First"));
        assert!(content_str.contains("Second"));
    }

    #[test]
    fn test_merge_consecutive_assistant_with_tool_calls() {
        // This is the main bug fix: consecutive assistant messages with tool_calls
        // should be merged for models requiring strict turns
        let tool_call_1 = serde_json::json!([
            {
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "arguments": "{\"location\":\"Paris\"}"
                }
            }
        ]);
        let tool_call_2 = serde_json::json!([
            {
                "id": "call_2",
                "type": "function",
                "function": {
                    "name": "get_time",
                    "arguments": "{\"timezone\":\"UTC\"}"
                }
            }
        ]);

        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: Some(MessageContent::Text("What's the weather?".to_string())),
                tool_calls: None,
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(MessageContent::Text("Let me check...".to_string())),
                tool_calls: Some(tool_call_1),
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(MessageContent::Text("And the time...".to_string())),
                tool_calls: Some(tool_call_2),
            },
        ];

        let caps = ModelCapabilities::REQUIRES_STRICT_TURNS;
        let result = transform_messages_for_capabilities(messages, caps);

        // Should merge into 2 messages: user + merged assistant
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, "user");
        assert_eq!(result[1].role, "assistant");

        // Content should be merged
        assert_eq!(
            result[1].content,
            Some(MessageContent::Text(
                "Let me check...\n\nAnd the time...".to_string()
            ))
        );

        // Tool calls should be concatenated
        let merged_tool_calls = result[1].tool_calls.as_ref().unwrap();
        let tool_calls_array = merged_tool_calls.as_array().unwrap();
        assert_eq!(tool_calls_array.len(), 2);
        assert_eq!(tool_calls_array[0]["id"], "call_1");
        assert_eq!(tool_calls_array[1]["id"], "call_2");
    }

    #[test]
    fn test_merge_assistant_messages_only_first_has_content() {
        // First message has content, second has only tool_calls
        let tool_call = serde_json::json!([
            {
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "arguments": "{}"
                }
            }
        ]);

        let messages = vec![
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(MessageContent::Text("Let me check...".to_string())),
                tool_calls: None,
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: None,
                tool_calls: Some(tool_call),
            },
        ];

        let caps = ModelCapabilities::REQUIRES_STRICT_TURNS;
        let result = transform_messages_for_capabilities(messages, caps);

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].content,
            Some(MessageContent::Text("Let me check...".to_string()))
        );
        assert!(result[0].tool_calls.is_some());
    }

    #[test]
    fn test_merge_assistant_messages_only_second_has_content() {
        // First message has only tool_calls, second has content
        let tool_call = serde_json::json!([
            {
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "arguments": "{}"
                }
            }
        ]);

        let messages = vec![
            ChatMessage {
                role: "assistant".to_string(),
                content: None,
                tool_calls: Some(tool_call),
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(MessageContent::Text("Result received".to_string())),
                tool_calls: None,
            },
        ];

        let caps = ModelCapabilities::REQUIRES_STRICT_TURNS;
        let result = transform_messages_for_capabilities(messages, caps);

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].content,
            Some(MessageContent::Text("Result received".to_string()))
        );
        assert!(result[0].tool_calls.is_some());
    }

    #[test]
    fn test_merge_assistant_messages_neither_has_content() {
        // Both messages have only tool_calls, no content
        let tool_call_1 = serde_json::json!([
            {
                "id": "call_1",
                "type": "function",
                "function": {"name": "tool1", "arguments": "{}"}
            }
        ]);
        let tool_call_2 = serde_json::json!([
            {
                "id": "call_2",
                "type": "function",
                "function": {"name": "tool2", "arguments": "{}"}
            }
        ]);

        let messages = vec![
            ChatMessage {
                role: "assistant".to_string(),
                content: None,
                tool_calls: Some(tool_call_1),
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: None,
                tool_calls: Some(tool_call_2),
            },
        ];

        let caps = ModelCapabilities::REQUIRES_STRICT_TURNS;
        let result = transform_messages_for_capabilities(messages, caps);

        assert_eq!(result.len(), 1);
        assert!(result[0].content.is_none());

        let merged_tool_calls = result[0].tool_calls.as_ref().unwrap();
        let tool_calls_array = merged_tool_calls.as_array().unwrap();
        assert_eq!(tool_calls_array.len(), 2);
    }

    #[test]
    fn test_no_merge_without_strict_turns_capability() {
        // Even with consecutive assistant messages, don't merge if capability not set
        let messages = vec![
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(MessageContent::Text("First".to_string())),
                tool_calls: None,
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(MessageContent::Text("Second".to_string())),
                tool_calls: None,
            },
        ];

        let caps = ModelCapabilities::empty();
        let result = transform_messages_for_capabilities(messages, caps);

        // Should NOT merge without REQUIRES_STRICT_TURNS capability
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_merge_preserves_different_role_boundaries() {
        // Don't merge across different roles
        let tool_call = serde_json::json!([
            {
                "id": "call_1",
                "type": "function",
                "function": {"name": "tool1", "arguments": "{}"}
            }
        ]);

        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: Some(MessageContent::Text("Question".to_string())),
                tool_calls: None,
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(MessageContent::Text("Answer".to_string())),
                tool_calls: Some(tool_call),
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some(MessageContent::Text("Follow-up".to_string())),
                tool_calls: None,
            },
        ];

        let caps = ModelCapabilities::REQUIRES_STRICT_TURNS;
        let result = transform_messages_for_capabilities(messages, caps);

        // Should remain 3 separate messages
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].role, "user");
        assert_eq!(result[1].role, "assistant");
        assert_eq!(result[2].role, "user");
    }

    /// Verify that array-form `content` (`OpenAI` multipart spec) deserializes
    /// correctly into `MessageContent::Parts` and is preserved on serialization.
    #[test]
    fn test_array_content_deserializes_to_parts() {
        let json = serde_json::json!({
            "role": "user",
            "content": [
                {"type": "text", "text": "What is in this image?"},
                {"type": "image_url", "image_url": {"url": "https://example.com/img.png"}}
            ]
        });
        let msg: ChatMessage = serde_json::from_value(json).unwrap();
        assert!(matches!(msg.content, Some(MessageContent::Parts(_))));
        // Round-trip: serialises back to array form
        let re_serialised = serde_json::to_value(&msg).unwrap();
        assert!(re_serialised["content"].is_array());
    }

    /// Verify that two user messages where one has array content and one has
    /// text content are merged correctly into a Parts result.
    #[test]
    fn test_merge_array_content_with_text_content() {
        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: Some(MessageContent::Parts(vec![
                    serde_json::json!({"type": "text", "text": "Look at this:"}),
                    serde_json::json!({"type": "image_url", "image_url": {"url": "https://example.com/img.png"}}),
                ])),
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some(MessageContent::Text("What do you see?".to_string())),
                tool_calls: None,
            },
        ];
        let caps = ModelCapabilities::REQUIRES_STRICT_TURNS;
        let result = transform_messages_for_capabilities(messages, caps);

        assert_eq!(result.len(), 1);
        // Parts + Text → Parts with a trailing text block
        assert!(matches!(&result[0].content, Some(MessageContent::Parts(p)) if p.len() == 3));
    }

    /// Verify that tool-result messages with array content survive the
    /// coalescing path unchanged (they are not mergeable roles).
    #[test]
    fn test_tool_message_with_array_content_passes_through() {
        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: Some(MessageContent::Text("Run the tool".to_string())),
                tool_calls: None,
            },
            ChatMessage {
                role: "tool".to_string(),
                content: Some(MessageContent::Parts(vec![
                    serde_json::json!({"type": "text", "text": "tool result here"}),
                ])),
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some(MessageContent::Text("Thanks".to_string())),
                tool_calls: None,
            },
        ];
        let caps = ModelCapabilities::REQUIRES_STRICT_TURNS;
        let result = transform_messages_for_capabilities(messages, caps);

        // tool message is not merged; user messages are consecutive after tool
        // so the two user messages are separated by the tool message
        assert_eq!(result.len(), 3);
        assert_eq!(result[1].role, "tool");
        assert!(matches!(&result[1].content, Some(MessageContent::Parts(_))));
    }
}
