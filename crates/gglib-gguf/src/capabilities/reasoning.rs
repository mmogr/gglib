//! Reasoning/thinking model capability detection.

use std::collections::HashMap;

use super::patterns::{
    REASONING_NAME_HIGH_CONFIDENCE, REASONING_NAME_MEDIUM_CONFIDENCE, THINKING_TAG_PATTERNS,
};

/// Detect if a model supports reasoning/thinking based on its metadata.
///
/// Scores the chat template, the model name and the architecture, and answers
/// whether the model is a reasoning model that outputs `<think>` or similar
/// tags.
#[must_use]
pub(crate) fn detect_reasoning_support(metadata: &HashMap<String, String>) -> bool {
    let mut score = 0.0f32;

    // Check chat template for thinking patterns (highest confidence)
    if let Some(template) = metadata.get("tokenizer.chat_template") {
        let template_lower = template.to_lowercase();

        for pattern in THINKING_TAG_PATTERNS {
            let pattern_lower = pattern.to_lowercase();
            if template_lower.contains(&pattern_lower) {
                // Opening tags are higher confidence
                if pattern.starts_with('<') && !pattern.starts_with("</") {
                    score += 0.4;
                } else {
                    score += 0.2;
                }
            }
        }

        // Check for template variables indicating thinking support
        if template_lower.contains("enable_thinking")
            || template_lower.contains("thinking_forced_open")
        {
            score += 0.3;
        }
    }

    // Check model name for reasoning patterns
    if let Some(name) = metadata.get("general.name") {
        let name_lower = name.to_lowercase();

        // High-confidence patterns
        for pattern in REASONING_NAME_HIGH_CONFIDENCE {
            if name_lower.contains(pattern) {
                score += 0.4;
            }
        }

        // Medium-confidence patterns (skip if already matched as high)
        for pattern in REASONING_NAME_MEDIUM_CONFIDENCE {
            if !REASONING_NAME_HIGH_CONFIDENCE.contains(pattern) && name_lower.contains(pattern) {
                score += 0.25;
            }
        }
    }

    // Check architecture
    if let Some(arch) = metadata.get("general.architecture") {
        if arch.to_lowercase().contains("deepseek") {
            score += 0.15;
        }
    }

    // 0.3 or more is a reasoning model
    score >= 0.3
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATE: &str = "tokenizer.chat_template";
    const NAME: &str = "general.name";
    const ARCH: &str = "general.architecture";

    fn detected(pairs: &[(&str, &str)]) -> bool {
        let metadata = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        detect_reasoning_support(&metadata)
    }

    /// Each signal alone, then the weak ones in pairs: a closing tag, a
    /// medium-confidence name and the architecture each fall short alone, and
    /// any two of them reach the threshold together.
    #[test]
    fn a_model_is_a_reasoning_model_once_its_signals_reach_the_threshold() {
        let plain = "{% for m in messages %}{{ m.content }}{% endfor %}";
        let cases: &[(&[(&str, &str)], bool)] = &[
            (&[], false),
            (&[(TEMPLATE, "<think>{{ m.thinking }}</think>")], true),
            (&[(TEMPLATE, "{{ m.content }}<think>")], true),
            (&[(TEMPLATE, "{{ m.content }}</think>")], false),
            (&[(TEMPLATE, "{% if enable_thinking %}on{% endif %}")], true),
            (&[(NAME, "DeepSeek-R1-Distill-Qwen-32B")], true),
            (&[(NAME, "Qwen3-8B")], false),
            (&[(ARCH, "deepseek2")], false),
            (&[(NAME, "Qwen3-8B"), (ARCH, "deepseek2")], true),
            (
                &[(TEMPLATE, "{{ m.content }}</think>"), (ARCH, "deepseek2")],
                true,
            ),
            (
                &[(TEMPLATE, "{{ m.content }}</think>"), (NAME, "Qwen3-8B")],
                true,
            ),
            (&[(TEMPLATE, plain), (NAME, "Llama-2-7B-Chat")], false),
        ];
        for (pairs, expected) in cases {
            assert_eq!(detected(pairs), *expected, "{pairs:?}");
        }
    }

    /// The fixture chat templates, on the template alone: the ones that carry
    /// thinking tags are reasoning models and the rest are not.
    #[test]
    fn the_fixture_templates_are_told_apart_by_their_thinking_tags() {
        let cases = [
            ("qwen2_5", include_str!("testdata/qwen2_5.jinja"), false),
            ("qwen3", include_str!("testdata/qwen3.jinja"), true),
            (
                "hermes2_pro",
                include_str!("testdata/hermes2_pro.jinja"),
                false,
            ),
            ("llama3_1", include_str!("testdata/llama3_1.jinja"), false),
            (
                "deepseek_r1",
                include_str!("testdata/deepseek_r1.jinja"),
                false,
            ),
            (
                "Qwen3.5-4B",
                include_str!("testdata/loop_guard_note/Qwen3.5-4B.jinja"),
                true,
            ),
            (
                "DeepSeek-V3.1",
                include_str!("testdata/loop_guard_note/deepseek-ai-DeepSeek-V3.1.jinja"),
                true,
            ),
            (
                "Mistral-Nemo",
                include_str!("testdata/loop_guard_note/mistralai-Mistral-Nemo-Instruct-2407.jinja"),
                false,
            ),
            (
                "gpt-oss-120b",
                include_str!("testdata/loop_guard_note/openai-gpt-oss-120b.jinja"),
                false,
            ),
            (
                "Phi-3.5-mini",
                include_str!("testdata/loop_guard_note/microsoft-Phi-3.5-mini-instruct.jinja"),
                false,
            ),
        ];
        for (name, template, expected) in cases {
            assert_eq!(detected(&[(TEMPLATE, template)]), expected, "{name}");
        }
    }
}
