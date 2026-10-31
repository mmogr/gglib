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

    /// The file names these three packages and bounds each from both sides:
    /// a floor the helper was tested at, and a ceiling under the next release
    /// line, so a bump moves a line rather than opening it. The versions
    /// themselves are Dependabot's to move: CI installs the file and runs
    /// the helper's tests against it on every run, which is what vouches for
    /// a bump.
    #[test]
    fn the_embedded_file_bounds_each_package_from_both_sides() {
        let lines = requirements();
        let names: Vec<&str> = lines
            .iter()
            .map(|line| line.split(['>', '<', '=', '~', '!']).next().unwrap())
            .collect();
        assert_eq!(names, ["huggingface_hub", "hf_xet", "tqdm"], "{lines:?}");
        for line in &lines {
            assert!(line.contains(">="), "no floor: {line}");
            assert!(line.contains(",<"), "no ceiling: {line}");
        }
    }
}
