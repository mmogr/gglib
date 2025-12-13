//! Pattern constants for capability detection.
//!
//! These patterns are used to identify model capabilities from
//! chat templates and model names.

/// Known thinking/reasoning tag patterns used by various models.
pub const THINKING_TAG_PATTERNS: &[&str] = &[
    // Standard patterns (DeepSeek R1, Qwen3, most reasoning models)
    "<think>",
    "<think ",
    "</think>",
    // Alternative tag names
    "<reasoning>",
    "</reasoning>",
    // Seed-OSS models
    "<seed:think>",
    "</seed:think>",
    // Command-R7B style
    "<|START_THINKING|>",
    "<|END_THINKING|>",
    // Apertus style
    "<|inner_prefix|>",
    "<|inner_suffix|>",
    // Nemotron V2 style
    "enable_thinking",
    // Bailing/Ring models
    "thinking_forced_open",
];

/// High-confidence reasoning model name patterns.
pub const REASONING_NAME_HIGH_CONFIDENCE: &[&str] = &["deepseek-r1", "qwq", "o1", "o3"];

/// Medium-confidence reasoning model name patterns.
pub const REASONING_NAME_MEDIUM_CONFIDENCE: &[&str] =
    &["deepseek-v3", "qwen3", "thinking", "reasoning", "cot"];

/// Tool calling model name patterns.
pub const TOOL_CALLING_NAME_PATTERNS: &[&str] = &[
    "hermes",
    "functionary",
    "firefunction",
    "toolcall",
    "function",
    "agent",
];

/// High-confidence tool calling patterns with format hints.
/// Format: (pattern, `format_name`, score)
pub const TOOL_PATTERNS_HIGH_CONFIDENCE: &[(&str, &str, f32)] = &[
    ("<tool_call>", "hermes", 0.5),
    ("</tool_call>", "hermes", 0.3),
    ("<tool_response>", "hermes", 0.3),
    ("[tool_calls]", "mistral", 0.5),
    ("[tool_results]", "mistral", 0.3),
    ("<｜tool▁calls▁begin｜>", "deepseek", 0.5),
    ("<｜tool▁call▁begin｜>", "deepseek", 0.4),
    ("<|python_tag|>", "llama3", 0.5),
    ("functools[", "firefunction", 0.5),
    (">>>", "functionary", 0.3),
    ("from functions import", "functionary", 0.4),
];

/// Medium-confidence tool calling patterns (Jinja conditionals).
pub const TOOL_PATTERNS_MEDIUM_CONFIDENCE: &[&str] = &[
    "if tools",
    "tools is defined",
    "tools | length",
    "available_tools",
];
