//! The id a model is listed under in `/v1/models`, and reading it back.

use super::*;

fn listed<'a>(id: i64, name: &'a str, quantization: Option<&'a str>) -> ListedModel<'a> {
    ListedModel {
        id,
        name,
        quantization,
    }
}

#[test]
fn a_name_no_other_model_has_is_its_own_id() {
    let models = [
        listed(1, "qwen3", Some("Q4_K_M")),
        listed(2, "llama", Some("Q4_K_M")),
        listed(3, "gemma", None),
    ];
    assert_eq!(openai_model_ids(&models), ["qwen3", "llama", "gemma"]);
}

#[test]
fn two_quantizations_of_one_name_are_listed_by_quantization() {
    let models = [
        listed(4, "qwen3", Some("Q4_K_M")),
        listed(9, "qwen3", Some("Q6_K")),
    ];
    assert_eq!(openai_model_ids(&models), ["qwen3@Q4_K_M", "qwen3@Q6_K"]);
}

/// The same quantization from two repositories cannot be told apart by it,
/// so both take their catalogue id; a third twin with its own quantization
/// still takes that.
#[test]
fn one_quantization_twice_is_listed_by_catalogue_id() {
    let models = [
        listed(4, "qwen3", Some("Q4_K_M")),
        listed(9, "qwen3", Some("Q4_K_M")),
        listed(11, "qwen3", Some("Q8_0")),
    ];
    assert_eq!(
        openai_model_ids(&models),
        ["qwen3@4", "qwen3@9", "qwen3@Q8_0"]
    );
}

#[test]
fn a_twin_with_no_quantization_is_listed_by_catalogue_id() {
    let models = [listed(4, "qwen3", Some("Q4_K_M")), listed(9, "qwen3", None)];
    assert_eq!(openai_model_ids(&models), ["qwen3@Q4_K_M", "qwen3@9"]);
}

#[test]
fn a_unique_name_beside_twins_stays_bare() {
    let models = [
        listed(1, "qwen3", Some("Q4_K_M")),
        listed(2, "mistral", Some("Q4_K_M")),
        listed(3, "qwen3", Some("Q6_K")),
    ];
    assert_eq!(
        openai_model_ids(&models),
        ["qwen3@Q4_K_M", "mistral", "qwen3@Q6_K"]
    );
}

/// A quantization that would read back as an id, or split at the wrong `@`,
/// is not used as a suffix.
#[test]
fn a_quantization_that_would_misread_takes_the_id_form() {
    let models = [
        listed(4, "qwen3", Some("8")),
        listed(9, "qwen3", Some("Q@4")),
        listed(11, "qwen3", Some("")),
    ];
    assert_eq!(
        openai_model_ids(&models),
        ["qwen3@4", "qwen3@9", "qwen3@11"]
    );
}

/// Every id the list writes reads back as the model it was written for.
#[test]
fn every_listed_id_resolves_to_its_own_model() {
    let models = [
        listed(1, "qwen3", Some("Q4_K_M")),
        listed(2, "qwen3", Some("Q4_K_M")),
        listed(3, "qwen3", Some("Q6_K")),
        listed(4, "qwen3", None),
        listed(5, "mistral", Some("Q4_K_M")),
    ];
    for (model, id) in models.iter().zip(openai_model_ids(&models)) {
        if id.contains(LISTED_ID_SEPARATOR) {
            assert_eq!(resolve_listed_id(&id, &models), Some(model.id), "{id}");
        } else {
            assert_eq!(id, model.name);
        }
    }
}

#[test]
fn split_reads_a_number_as_an_id_and_anything_else_as_a_quantization() {
    assert_eq!(
        split_listed_id("qwen3@7"),
        Some(("qwen3", ListedSuffix::Id(7)))
    );
    assert_eq!(
        split_listed_id("qwen3@Q6_K"),
        Some(("qwen3", ListedSuffix::Quantization("Q6_K")))
    );
    // On the last `@`, so a name that has one still carries a suffix.
    assert_eq!(
        split_listed_id("org@qwen3@Q6_K"),
        Some(("org@qwen3", ListedSuffix::Quantization("Q6_K")))
    );
    for bare in ["qwen3", "@Q6_K", "qwen3@"] {
        assert_eq!(split_listed_id(bare), None, "{bare}");
    }
}

#[test]
fn an_id_suffix_must_name_a_model_of_that_name() {
    let models = [listed(1, "qwen3", None), listed(2, "mistral", None)];
    assert_eq!(resolve_listed_id("qwen3@1", &models), Some(1));
    assert_eq!(resolve_listed_id("qwen3@2", &models), None);
    assert_eq!(resolve_listed_id("qwen3@3", &models), None);
}

/// Two models of one name and one quantization were listed by id, so the
/// quantization alone names neither.
#[test]
fn a_quantization_two_twins_share_resolves_to_nothing() {
    let models = [
        listed(1, "qwen3", Some("Q4_K_M")),
        listed(2, "qwen3", Some("Q4_K_M")),
        listed(3, "qwen3", Some("Q6_K")),
    ];
    assert_eq!(resolve_listed_id("qwen3@Q4_K_M", &models), None);
    assert_eq!(resolve_listed_id("qwen3@Q6_K", &models), Some(3));
    assert_eq!(resolve_listed_id("qwen3@NOPE", &models), None);
    assert_eq!(resolve_listed_id("qwen3", &models), None);
}

/// What `get_by_identifier` finds for `identifier`, in its order: a number
/// is an id, then the exact name (its lowest row), then an `@` form.
fn looked_up(identifier: &str, models: &[ListedModel<'_>]) -> Option<i64> {
    if let Ok(id) = identifier.parse::<i64>() {
        return models.iter().find(|m| m.id == id).map(|m| m.id);
    }
    models
        .iter()
        .filter(|m| m.name == identifier)
        .map(|m| m.id)
        .min()
        .or_else(|| resolve_listed_id(identifier, models))
}

/// A model whose name is literally another's suffixed id would be found by
/// the exact name first, so that id is not used: the next form is, and the
/// bare catalogue id when every form is someone's name. Every id is
/// distinct and finds its own model through the real lookup order.
#[test]
fn an_id_that_is_another_models_name_is_not_used() {
    let models = [
        listed(1, "foo@bar", None),
        listed(2, "foo", Some("bar")),
        listed(3, "foo", Some("Q8")),
        listed(4, "foo@5", None),
        listed(5, "qwen", None),
        listed(6, "qwen", None),
        listed(7, "qwen@5", None),
        listed(8, "qwen@Q4", None),
    ];
    let ids = openai_model_ids(&models);

    assert_eq!(ids[1], "foo@2");
    assert_eq!(ids[4], "5");
    let mut distinct = ids.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(distinct.len(), ids.len(), "{ids:?}");
    for (model, id) in models.iter().zip(&ids) {
        assert_eq!(looked_up(id, &models), Some(model.id), "{id} in {ids:?}");
    }
}
