//! Pattern constants for capability detection.
//!
//! These patterns identify a reasoning model from its
//! chat template and model name.

/// Known thinking/reasoning tag patterns used by various models.
pub(crate) const THINKING_TAG_PATTERNS: &[&str] = &[
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
pub(crate) const REASONING_NAME_HIGH_CONFIDENCE: &[&str] = &["deepseek-r1", "qwq", "o1", "o3"];

/// Medium-confidence reasoning model name patterns.
pub(crate) const REASONING_NAME_MEDIUM_CONFIDENCE: &[&str] =
    &["deepseek-v3", "qwen3", "thinking", "reasoning", "cot"];
