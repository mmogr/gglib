//! What a command says before it offers to install llama.cpp, and what it
//! asks.

use super::*;

/// A checkout builds, and says how long that takes before it asks.
#[test]
fn a_checkout_is_told_a_build_is_coming_and_what_it_costs() {
    let (lines, question) = offer(&InstallMethod::Source { no_prebuilt: None });

    assert_eq!(
        lines,
        [
            "Running from source repository - will build llama.cpp from source.",
            "",
            "   This compiles llama.cpp for your hardware and typically takes",
            "   15-30 minutes. It happens once; later runs reuse the binaries.",
            "",
        ]
    );
    assert_eq!(question, "Would you like to install llama.cpp now?");
}

#[test]
fn a_platform_with_a_release_is_offered_the_download() {
    let (lines, question) = offer(&InstallMethod::Prebuilt {
        description: "macOS ARM64 (Metal)".to_owned(),
    });

    assert_eq!(
        lines,
        [
            "Pre-built llama.cpp binaries are available for macOS ARM64 (Metal).",
            "",
        ]
    );
    assert_eq!(question, "Would you like to download them now?");
}

/// The tools a build needs are listed, and how to install them is the text
/// `config llama install` gives when one is missing.
#[test]
fn a_platform_without_a_release_is_told_why_and_what_a_build_needs() {
    let (lines, question) = offer(&InstallMethod::Source {
        no_prebuilt: Some("Linux requires building from source for CUDA support".to_owned()),
    });

    let mut expected: Vec<String> = [
        "Linux requires building from source for CUDA support",
        "",
        "llama.cpp will be built from source to enable GPU acceleration.",
        "",
        "   This compiles llama.cpp for your hardware and typically takes",
        "   15-30 minutes. It happens once; later runs reuse the binaries.",
        "",
        "Required build tools:",
        "  • git - for cloning the repository",
        "  • cmake - for build configuration",
        "  • g++ or clang++ - for compilation",
        "",
    ]
    .map(str::to_owned)
    .to_vec();
    expected.extend(build_tool_install_lines());
    expected.push(String::new());

    assert_eq!(lines, expected);
    assert_eq!(question, "Would you like to build llama.cpp now?");
}
