//! The sampling parameters a config states, as text.

use gglib_core::domain::InferenceConfig;

/// Every parameter `config` sets: its field name and its value as it was
/// typed, in name order so two runs print the same thing.
///
/// Reads [`InferenceConfig::to_openai_json_patch`] rather than naming fields,
/// because that patch *is* every field the config holds. A hand-written list
/// covered seven of the eighteen in the serve banner and eleven in `gglib
/// model inspect`, so a model storing only `frequency_penalty` showed no
/// defaults at all while they applied to every request.
pub(crate) fn stated_parameters(config: &InferenceConfig) -> Vec<(String, String)> {
    let patch = config.to_openai_json_patch();
    let mut fields: Vec<_> = patch.iter().collect();
    fields.sort_by_key(|(field, _)| *field);
    let stated = |(field, value): (&String, _)| (field.clone(), render(value));
    fields.into_iter().map(stated).collect()
}

/// Render one patch value the way the user typed it.
///
/// Every sampling parameter gglib models as a float is an `f32`, and the patch
/// carries them as JSON numbers — i.e. `f64`. Printing that directly shows
/// `0.1` as `0.10000000149011612`: the f64 nearest to the f32 nearest to 0.1,
/// which is accurate, useless, and not what anyone typed. Narrowing back to
/// `f32` before formatting restores the shortest representation that
/// round-trips, so `--temperature 0.1` prints `0.1`.
#[allow(
    clippy::cast_possible_truncation,
    reason = "grandfathered at lint inheritance, #1157"
)]
fn render(value: &serde_json::Value) -> String {
    match value.as_f64() {
        // Integral values print without a synthetic ".0" — `max-tokens: 512`,
        // not `512.0`.
        Some(n) if n.fract() == 0.0 => format!("{n}"),
        Some(n) => format!("{}", n as f32),
        None => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The regression: an `f32` widened through JSON printed its f64 shadow.
    #[test]
    fn a_float_prints_as_the_user_typed_it() {
        let patch = InferenceConfig {
            temperature: Some(0.1),
            top_p: Some(0.95),
            ..Default::default()
        }
        .to_openai_json_patch();

        assert_eq!(render(&patch["temperature"]), "0.1");
        assert_eq!(render(&patch["top_p"]), "0.95");
    }

    /// Counts stay counts: no synthetic decimal point.
    #[test]
    fn an_integral_value_prints_without_a_fraction() {
        let patch = InferenceConfig {
            max_tokens: Some(512),
            top_k: Some(40),
            ..Default::default()
        }
        .to_openai_json_patch();

        assert_eq!(render(&patch["max_tokens"]), "512");
        assert_eq!(render(&patch["top_k"]), "40");
    }

    /// Non-numeric fields (the reasoning effort level) pass through unharmed.
    #[test]
    fn a_non_numeric_value_falls_back_to_its_own_rendering() {
        assert_eq!(render(&serde_json::json!("high")), "\"high\"");
    }

    /// Every field that is set is stated, under its own name and in name
    /// order, and none that is not.
    #[test]
    fn every_field_that_is_set_is_stated_in_name_order() {
        let config = InferenceConfig {
            top_k: Some(40),
            temperature: Some(0.1),
            frequency_penalty: Some(0.25),
            seed: Some(7),
            reasoning_budget_tokens: Some(-1),
            ..Default::default()
        };

        let stated = stated_parameters(&config);

        let pairs: Vec<_> = stated
            .iter()
            .map(|(f, v)| (f.as_str(), v.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("frequency_penalty", "0.25"),
                ("reasoning_budget_tokens", "-1"),
                ("seed", "7"),
                ("temperature", "0.1"),
                ("top_k", "40"),
            ]
        );
        assert!(stated_parameters(&InferenceConfig::default()).is_empty());
    }
}
