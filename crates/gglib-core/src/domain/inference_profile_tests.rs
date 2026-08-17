//! Tests for [`super`] — profile-name validation and the starter templates.

use super::*;

#[test]
fn accepts_lowercase_alphanumeric_and_hyphen() {
    for name in [
        "coding",
        "chat",
        "creative",
        "minimal",
        "xhigh",
        "long-form",
        "gpt4-style",
        "a",
    ] {
        assert!(validate_name(name).is_ok(), "should accept {name}");
    }
}

#[test]
fn rejects_empty_name() {
    assert_eq!(validate_name(""), Err(ProfileNameError::Empty));
}

#[test]
fn rejects_name_over_the_length_cap() {
    let long = "a".repeat(MAX_PROFILE_NAME_LEN + 1);
    assert_eq!(
        validate_name(&long),
        Err(ProfileNameError::TooLong(MAX_PROFILE_NAME_LEN + 1))
    );
    assert!(validate_name(&"a".repeat(MAX_PROFILE_NAME_LEN)).is_ok());
}

/// Uppercase, underscores, dots, spaces and colons are all outside the
/// conservative set — the colon especially, since it is the delimiter.
#[test]
fn rejects_characters_outside_the_conservative_set() {
    for name in ["Coding", "long_form", "v1.2", "long form", "a:b", "café"] {
        assert!(
            matches!(
                validate_name(name),
                Err(ProfileNameError::InvalidCharacters(_))
            ),
            "should reject {name}"
        );
    }
}

#[test]
fn rejects_leading_or_trailing_hyphen() {
    for name in ["-coding", "coding-", "-"] {
        assert!(
            matches!(
                validate_name(name),
                Err(ProfileNameError::HyphenBoundary(_))
            ),
            "should reject {name}"
        );
    }
}

#[test]
fn rejects_reserved_profile_names() {
    for name in RESERVED_PROFILE_NAMES {
        assert_eq!(
            validate_name(name),
            Err(ProfileNameError::Reserved((*name).to_owned()))
        );
    }
}

#[test]
fn builtin_template_names_are_valid_and_unique() {
    let templates = builtin_templates();
    let mut names: Vec<&str> = templates.iter().map(|p| p.name.as_str()).collect();
    names.sort_unstable();
    let unique_count = {
        let mut deduped = names.clone();
        deduped.dedup();
        deduped.len()
    };
    assert_eq!(names.len(), unique_count, "template names must be unique");

    for profile in &templates {
        assert!(profile.validate().is_ok(), "invalid name: {}", profile.name);
    }
}

/// The central invariant: a template must leave the parameters it does not
/// care about as `None` so they still resolve from the model's own
/// defaults. A template that filled every field would silently override
/// per-model tuning such as `reasoning_profile`'s `presence_penalty`.
#[test]
fn builtin_templates_are_sparse() {
    for profile in builtin_templates() {
        let c = &profile.config;
        assert!(c.top_k.is_none(), "{} leaves top_k open", profile.name);
        assert!(
            c.max_tokens.is_none(),
            "{} leaves max_tokens open",
            profile.name
        );
        assert!(
            c.repeat_penalty.is_none(),
            "{} leaves repeat_penalty open",
            profile.name
        );
        assert!(
            c.presence_penalty.is_none(),
            "{} leaves presence_penalty open",
            profile.name
        );
        assert!(c.min_p.is_none(), "{} leaves min_p open", profile.name);
    }
}

/// Each family sets its own two parameters and nothing from the other's, so
/// installing the reasoning rungs cannot quietly restyle a model's sampling
/// and installing `coding` cannot quietly cap its thinking.
#[test]
fn each_family_sets_only_its_own_two_parameters() {
    for profile in sampling_templates() {
        let c = &profile.config;
        assert!(c.temperature.is_some(), "{} sets temperature", profile.name);
        assert!(c.top_p.is_some(), "{} sets top_p", profile.name);
        assert!(
            c.reasoning_effort.is_none() && c.reasoning_budget_tokens.is_none(),
            "{} leaves both reasoning controls open",
            profile.name
        );
    }

    for profile in reasoning_templates() {
        let c = &profile.config;
        assert!(
            c.temperature.is_none() && c.top_p.is_none(),
            "{} leaves the distribution alone",
            profile.name
        );
    }
}

/// The whole point of the reasoning family: a rung that set only the effort
/// would be inert on any template that does not read the variable, which is a
/// case ADR 0007 measured rather than imagined. Both halves, every rung.
#[test]
fn every_reasoning_rung_sets_both_halves() {
    let rungs = reasoning_templates();
    assert_eq!(rungs.len(), ReasoningEffort::ALL.len());

    for (profile, expected) in rungs.iter().zip(ReasoningEffort::ALL) {
        assert_eq!(
            profile.name,
            expected.as_str(),
            "one rung per level, named for it and in ladder order"
        );
        assert_eq!(profile.config.reasoning_effort, Some(expected));
        let budget = profile
            .config
            .reasoning_budget_tokens
            .expect("every rung carries a budget so it still bites without template support");
        assert!(
            budget >= -1,
            "{} budget {budget} is inside upstream's range",
            profile.name
        );
    }
}

/// The budget ladder must be monotonic in the direction its names promise:
/// `high` may not think less than `low`. `max`'s `-1` is the deliberate
/// exception — it is "no cap", the top of the ladder rather than below its
/// bottom — so it is checked as the sentinel it is.
#[test]
fn the_budget_ladder_rises_with_the_effort_level() {
    let budgets: Vec<i32> = reasoning_templates()
        .iter()
        .map(|p| p.config.reasoning_budget_tokens.expect("set above"))
        .collect();

    let (top, capped) = budgets.split_last().expect("six rungs");
    assert_eq!(*top, -1, "the top rung defers rather than naming a ceiling");
    assert!(
        capped.windows(2).all(|w| w[0] < w[1]),
        "capped rungs must increase: {capped:?}"
    );
    assert!(capped.iter().all(|&b| b > 0), "no capped rung is a no-op");
}

/// Six listed variants per model would swamp the model picker
/// `list_in_models` exists to protect, so only the ends and a usable middle
/// are advertised. The other three stay addressable by name.
#[test]
fn only_three_reasoning_rungs_are_listed() {
    let listed: Vec<String> = reasoning_templates()
        .into_iter()
        .filter(|p| p.list_in_models)
        .map(|p| p.name)
        .collect();

    assert_eq!(listed, ["low", "high", "max"]);
}

/// A rung's description has to state both halves, because the effort half is
/// only a request — a user reading `gglib config profile list` should not
/// need ADR 0007 to learn that a template may ignore it.
#[test]
fn a_rung_description_names_the_level_and_the_budget() {
    let rungs = reasoning_templates();

    let minimal = rungs.first().expect("minimal is the first rung");
    let text = minimal.description.as_deref().expect("described");
    assert!(text.contains("minimal"), "names the level: {text}");
    assert!(text.contains("256"), "names the budget: {text}");
    assert!(
        text.contains("where the template reads it"),
        "says the effort half is conditional: {text}"
    );

    let max = rungs.last().expect("max is the last rung");
    let text = max.description.as_deref().expect("described");
    assert!(
        !text.contains("-1"),
        "the sentinel is explained, not printed: {text}"
    );
    assert!(text.contains("launch default"), "got: {text}");
}

#[test]
fn serializes_with_camel_case_keys() {
    let profile = &builtin_templates()[0];
    let json = serde_json::to_value(profile).expect("serializes");
    assert!(json.get("listInModels").is_some());
    assert!(json.get("list_in_models").is_none());
}
