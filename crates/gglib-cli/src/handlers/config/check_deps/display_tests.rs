use super::*;
use gglib_core::utils::system::{
    BuildOrigin, DependencyStatus, EngineInstall, Found, GpuInfo, HostOs, LinuxDistro,
    dependency_report,
};

/// The same text with its colour taken out.
fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("\x1b[") {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        rest = after.find('m').map_or("", |end| &after[end + 1..]);
    }
    out.push_str(rest);
    out
}

fn rendered(os: HostOs, origin: BuildOrigin, found: &Found) -> String {
    let text = plain(&render_report(&dependency_report(
        os,
        origin,
        LinuxDistro::Debian,
        found,
    )));
    // Shown with `--nocapture`: the report as a user on `os` would read it.
    println!("--- {os:?}, {origin:?} ---\n{text}");
    text
}

/// C11 on the installed copy that reported it: nothing is required and
/// missing, and the report says what the build tools are for.
#[test]
fn an_installed_gglib_is_told_it_needs_none_of_the_build_tools() {
    for os in [HostOs::Windows, HostOs::Linux] {
        let found = Found {
            llama_install: EngineInstall::Download,
            llama_installed: true,
            ..Found::default()
        };
        let text = rendered(os, BuildOrigin::Installed, &found);

        assert!(!text.contains("✗ missing"), "{text}");
        assert!(!text.lines().any(|line| line.starts_with('*')), "{text}");
        assert!(
            text.contains("✓ Nothing this gglib needs is missing."),
            "{text}"
        );
        for words in [
            "Building gglib from source",
            "Building an engine from source",
            "An installed gglib needs none of them",
        ] {
            assert!(text.contains(words), "{words} in {text}");
        }
        assert!(
            text.lines()
                .any(|line| line.starts_with(" npm ") && line.contains("○ not installed")),
            "{text}"
        );
    }
}

/// A checkout build still marks what builds it, and fails without it.
#[test]
fn a_checkout_is_told_what_building_it_needs() {
    let mut found = Found {
        gpu: GpuInfo {
            has_metal: true,
            ..GpuInfo::default()
        },
        llama_install: EngineInstall::Download,
        ..Found::default()
    };
    for (name, version) in [
        ("cargo", "1.97.1"),
        ("rustc", "1.97.1"),
        ("node", "24.22.3"),
        ("npm", "11.19.1"),
        ("git", "2.54.0"),
        ("cmake", "4.3.3"),
        ("make", "3.81"),
        ("pkg-config", "3.0.7"),
        ("libssl-dev", "3.6.3"),
        ("python3", "3.14.6"),
    ] {
        found.versions.insert(name, version.to_owned());
    }
    let text = rendered(HostOs::MacOs, BuildOrigin::Checkout, &found);

    assert!(text.contains("*cargo               ✓ v1.97.1"), "{text}");
    assert!(
        text.lines()
            .any(|line| line.starts_with("*C++ compiler") && line.contains("✗ missing")),
        "{text}"
    );
    assert!(
        text.contains("✗ 1 dependency this gglib needs is missing."),
        "{text}"
    );
    assert!(text.contains(" Metal               ✓ available"), "{text}");
}

/// C11: a probe that found something without a version, such as Metal,
/// printed as "✓ vavailable".
#[test]
fn a_word_where_a_version_would_be_takes_no_v() {
    let available =
        Dependency::optional("Metal", "Apple GPU").with_status(DependencyStatus::Present {
            version: "available".to_string(),
        });
    let row = dependency_row(&available);
    assert!(row.contains("✓ available"), "{row}");

    let versioned = Dependency::required("cmake", "Build").with_status(DependencyStatus::Present {
        version: "3.28.1".to_string(),
    });
    assert!(dependency_row(&versioned).contains("✓ v3.28.1"));
}
