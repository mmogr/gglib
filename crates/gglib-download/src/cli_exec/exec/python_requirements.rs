//! The accelerator's Python packages.
//!
//! They live in `scripts/hf_xet_requirements.txt`, beside the helper they
//! serve, so the helper's own tests install exactly what gglib installs.

const REQUIREMENTS_SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/scripts/hf_xet_requirements.txt"
));

/// One requirement specifier per package, comments and blank lines left out.
pub(super) fn requirements() -> Vec<String> {
    parse(REQUIREMENTS_SOURCE)
}

fn parse(source: &str) -> Vec<String> {
    source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_and_blank_lines_are_not_requirements() {
        assert_eq!(parse("# why\n\n  a>=1,<2  \nb==3\n"), ["a>=1,<2", "b==3"]);
    }

    /// The pins are bumped on purpose: this fails when the file changes, so
    /// the change is made twice and read once.
    #[test]
    fn the_embedded_file_pins_the_tested_release_lines() {
        assert_eq!(
            requirements(),
            [
                "huggingface_hub>=2.1.1,<2.2",
                "hf_xet>=1.6.0,<1.7",
                "tqdm>=4.70.1,<5"
            ]
        );
    }
}
