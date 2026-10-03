//! Which of a repository's projectors is fetched with a download.
//!
//! [`choose_projector`] holds the rule once. A download, a repair and every
//! listing that says "this one comes with it" ask it.

use super::types::Quantization;

/// The projector a download of `quantization` fetches, among `projectors`,
/// the paths of the repository's projector files.
///
/// The projector whose own name carries `quantization`; else the `F16` one;
/// else the first by name. A name that carries no quantization satisfies
/// only the last arm. `None` exactly when the repository has no projector.
/// Where several satisfy an arm, the first of them by name is taken, so the
/// answer does not depend on the order given.
#[must_use]
pub fn choose_projector<'a>(
    quantization: Quantization,
    projectors: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    let mut by_name: Vec<&str> = projectors.into_iter().collect();
    by_name.sort_unstable();
    let carrying = |wanted: Quantization| {
        by_name
            .iter()
            .copied()
            .find(|name| !wanted.is_unknown() && Quantization::from_filename(name) == wanted)
    };
    carrying(quantization)
        .or_else(|| carrying(Quantization::F16))
        .or_else(|| by_name.first().copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_projector_of_the_same_quantization_is_chosen() {
        let projectors = [
            "mmproj-F16.gguf",
            "X.mmproj-Q8_0.gguf",
            "a-mmproj-BF16.gguf",
        ];

        assert_eq!(
            choose_projector(Quantization::Q8_0, projectors),
            Some("X.mmproj-Q8_0.gguf")
        );
    }

    #[test]
    fn without_one_of_the_same_quantization_the_f16_one_is_chosen() {
        let projectors = ["a-mmproj-BF16.gguf", "mmproj-F16.gguf", "mmproj-Q4_0.gguf"];

        assert_eq!(
            choose_projector(Quantization::Q8_0, projectors),
            Some("mmproj-F16.gguf")
        );
    }

    #[test]
    fn without_either_the_first_by_name_is_chosen() {
        let projectors = ["mmproj-Q4_0.gguf", "mmproj-BF16.gguf"];

        assert_eq!(
            choose_projector(Quantization::Q8_0, projectors),
            Some("mmproj-BF16.gguf")
        );
    }

    #[test]
    fn a_repository_without_projectors_gives_none() {
        assert_eq!(choose_projector(Quantization::Q8_0, []), None);
    }

    /// Two files of one arm: the first by name, whatever order they came in.
    #[test]
    fn the_order_given_does_not_change_the_choice() {
        let one_way = ["b.mmproj-Q8_0.gguf", "a.mmproj-Q8_0.gguf"];
        let other_way = ["a.mmproj-Q8_0.gguf", "b.mmproj-Q8_0.gguf"];

        assert_eq!(
            choose_projector(Quantization::Q8_0, one_way),
            Some("a.mmproj-Q8_0.gguf")
        );
        assert_eq!(
            choose_projector(Quantization::Q8_0, other_way),
            Some("a.mmproj-Q8_0.gguf")
        );
    }

    /// A name with no quantization in it is not "the same quantization" as an
    /// unknown one: the F16 projector is still preferred.
    #[test]
    fn an_unknown_quantization_matches_no_projector_by_name() {
        let projectors = ["a.mmproj.gguf", "mmproj-F16.gguf"];

        assert_eq!(
            choose_projector(Quantization::Unknown, projectors),
            Some("mmproj-F16.gguf")
        );
    }

    /// An F16 download takes the F16 projector by the first arm, and a
    /// projector with no quantization in its name is still a choice.
    #[test]
    fn an_unquantized_projector_name_is_chosen_when_it_is_the_only_one() {
        assert_eq!(
            choose_projector(Quantization::F16, ["model.mmproj.gguf"]),
            Some("model.mmproj.gguf")
        );
    }
}
