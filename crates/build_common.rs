// Shared build script utilities for README-to-rustdoc transformation.
// Include this in build.rs files with: include!("../build_common.rs");
//
// Required imports in the including file:
//   use std::env;
//   use std::fs;
//   use std::path::Path;
//
// Its tests run in `crates/gglib-build-info/tests/build_common.rs`, which
// includes this file the same way.

/// Process a crate's README.md for rustdoc, so its front page renders without
/// a network and without links that only work on GitHub.
///
/// Transformations:
/// 1. Drop every badge line: a shields.io image, and the file on the `badges`
///    branch it reads, need a network, and the docs are read offline too.
/// 2. Point every relative link at the repository (see
///    [`absolutize_relative_links`]).
///
/// The repo URL is read from workspace Cargo.toml, keeping READMEs URL-agnostic.
///
/// # Why source links and not module links
///
/// A `](src/…)` link names a *path*, so the source is the honest target. Turned
/// into an intra-doc link to the module, it would break for a `#[cfg(test)]`
/// module, which rustdoc does not compile, and for a name shared by a function
/// and a module, which is ambiguous; rustdoc warns on both. It would resolve
/// for a private module only under `--document-private-items`, which
/// `make doc-check` and the ci, docs and release workflows pass. Pointing at
/// the source on the repo works for every link, needs no visibility analysis,
/// and reads the same on GitHub and in rustdoc.
fn process_readme_for_rustdoc(crate_dir: &str) {
    println!("cargo:rerun-if-changed=README.md");
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    // This file is `include!`d into every crate's build.rs, so editing it
    // changes what they all emit. Without this line cargo does not know that,
    // and a stale build-script binary goes on applying the previous
    // transformation with no sign that it has.
    println!("cargo:rerun-if-changed=../build_common.rs");

    let readme_path = Path::new(crate_dir).join("README.md");
    let Ok(content) = fs::read_to_string(&readme_path) else {
        return; // No README, nothing to process
    };

    let mut rustdoc_content = strip_badge_lines(&content);

    // Without a repo URL the links stay relative, and `make doc-check` reports
    // the ones that then break.
    if let (Some(url), Some(crate_path)) = (
        get_workspace_repo_url(crate_dir),
        crate_relative_path(crate_dir),
    ) {
        let root = Path::new(crate_dir).join("../..");
        let is_dir = |path: &str| root.join(path).is_dir();
        rustdoc_content = absolutize_relative_links(&rustdoc_content, &url, &crate_path, &is_dir);
    }

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("README_GENERATED.md");
    fs::write(dest_path, rustdoc_content).unwrap();
}

/// The text without its badge lines: any line holding a shields.io image or a
/// link to a file on the `badges` branch.
fn strip_badge_lines(content: &str) -> String {
    content
        .split_inclusive('\n')
        .filter(|line| {
            !line.contains("img.shields.io")
                && !line.contains("raw.githubusercontent.com/mmogr/gglib/badges")
        })
        .collect()
}

/// Rewrite every relative link, `](target)`, to an absolute repository URL.
///
/// A target is rewritten when it has a `/` or ends in `.md` or `.rs` (its
/// `#fragment` set aside), and has no `::`, which marks an intra-doc link such
/// as `crate::ports` that rustdoc resolves itself. A URL, a bare `#fragment`
/// and a `mailto:` are left alone, and so is a target with a space, which is
/// not a link. The target is resolved against the crate's directory with `.`
/// and `..` worked out; one that climbs out of the repository is left as
/// written.
///
/// A directory, or a target ending in `/`, gets `tree`, a file `blob`. GitHub
/// redirects `blob` to `tree` for a directory, so the distinction is not
/// load-bearing; it means the emitted URL is the one that answers rather than
/// the one that bounces. The fragment is kept.
fn absolutize_relative_links(
    content: &str,
    repo_url: &str,
    crate_path: &str,
    is_dir: &dyn Fn(&str) -> bool,
) -> String {
    const OPEN: &str = "](";

    let mut out = String::with_capacity(content.len());
    let mut rest = content;

    while let Some(start) = rest.find(OPEN) {
        let (before, from_open) = rest.split_at(start + OPEN.len());
        out.push_str(before);
        let Some(close) = from_open.find(')') else {
            // No closing paren: not a link after all, so copy it through
            // unchanged rather than swallowing the rest of the file.
            out.push_str(from_open);
            return out;
        };
        let target = &from_open[..close];
        if let Some((path, fragment)) = repository_path(target, crate_path) {
            let kind =
                if target.split('#').next().is_some_and(|p| p.ends_with('/')) || is_dir(&path) {
                    "tree"
                } else {
                    "blob"
                };
            // Pushed piecewise rather than through `format!`: clippy's
            // `format_push_string` objects to the extra allocation.
            out.push_str(repo_url);
            out.push('/');
            out.push_str(kind);
            out.push_str("/main/");
            out.push_str(&path);
            out.push_str(fragment);
        } else {
            out.push_str(target);
        }
        rest = &from_open[close..];
    }

    out.push_str(rest);
    out
}

/// The repository path a relative link target names, and its `#fragment`
/// (empty when it has none), or `None` when the target is not one to rewrite.
fn repository_path<'a>(target: &'a str, crate_path: &str) -> Option<(String, &'a str)> {
    const LEFT_ALONE: [&str; 3] = ["http", "#", "mailto:"];
    if target.is_empty()
        || target.contains("::")
        || target.contains(char::is_whitespace)
        || LEFT_ALONE.iter().any(|prefix| target.starts_with(prefix))
    {
        return None;
    }
    let (path, fragment) = target
        .find('#')
        .map_or((target, ""), |at| target.split_at(at));
    let extension = Path::new(path).extension().and_then(|e| e.to_str());
    if !(path.contains('/') || matches!(extension, Some("md" | "rs"))) {
        return None;
    }
    let mut segments: Vec<&str> = crate_path.split('/').filter(|s| !s.is_empty()).collect();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            name => segments.push(name),
        }
    }
    Some((segments.join("/"), fragment))
}

/// The crate's directory relative to the workspace root, slash-separated for
/// use in a URL (e.g. `crates/gglib-core`).
///
/// Uses the same "two levels up is the workspace root" assumption as
/// [`get_workspace_repo_url`], so the two agree by construction.
fn crate_relative_path(crate_dir: &str) -> Option<String> {
    let path = Path::new(crate_dir);
    let workspace_root = path.parent()?.parent()?;
    let relative = path.strip_prefix(workspace_root).ok()?;
    Some(
        relative
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

/// Extract repository URL from workspace Cargo.toml.
/// Returns None if the file can't be read or doesn't contain a repository field.
fn get_workspace_repo_url(crate_dir: &str) -> Option<String> {
    let workspace_toml = Path::new(crate_dir)
        .parent()? // crates/
        .parent()? // workspace root
        .join("Cargo.toml");

    let content = fs::read_to_string(workspace_toml).ok()?;

    // Simple extraction: find repository = "..." line
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("repository")
            && line.contains('=')
            && let Some(start) = line.find('"')
            && let Some(end) = line.rfind('"')
            && start < end
        {
            return Some(line[start + 1..end].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{absolutize_relative_links, strip_badge_lines};

    fn rewrite(content: &str) -> String {
        let is_dir = |path: &str| path == "crates/gglib-x/src/dir";
        absolutize_relative_links(content, "https://g/r", "crates/gglib-x", &is_dir)
    }

    #[test]
    fn relative_links_point_at_the_repository() {
        let cases = [
            (
                "[a](src/lib.rs)",
                "[a](https://g/r/blob/main/crates/gglib-x/src/lib.rs)",
            ),
            (
                "[a](src/sub/)",
                "[a](https://g/r/tree/main/crates/gglib-x/src/sub)",
            ),
            (
                "[a](src/dir)",
                "[a](https://g/r/tree/main/crates/gglib-x/src/dir)",
            ),
            (
                "[a](../README.md#arch)",
                "[a](https://g/r/blob/main/crates/README.md#arch)",
            ),
            (
                "[a](../../docs/x.md)",
                "[a](https://g/r/blob/main/docs/x.md)",
            ),
            (
                "[a](./src/./a.rs)",
                "[a](https://g/r/blob/main/crates/gglib-x/src/a.rs)",
            ),
            (
                "[a](NOTES.md)",
                "[a](https://g/r/blob/main/crates/gglib-x/NOTES.md)",
            ),
            (
                "![i](docs/i.png)",
                "![i](https://g/r/blob/main/crates/gglib-x/docs/i.png)",
            ),
            (
                "[a](x.rs), [b](y/)",
                "[a](https://g/r/blob/main/crates/gglib-x/x.rs), [b](https://g/r/tree/main/crates/gglib-x/y)",
            ),
        ];
        for (input, want) in cases {
            assert_eq!(rewrite(input), want, "for {input}");
        }
    }

    #[test]
    fn other_targets_are_left_as_written() {
        let cases = [
            "[a](crate::ports)",
            "[a](crate::ports::ModelRepository)",
            // `::` alone keeps it: an intra-doc path that also ends in `.rs`.
            "[a](self::fmt::lib.rs)",
            "[a](https://example.com/x/y.md)",
            "[a](http://example.com/)",
            "[a](#heading)",
            "[a](mailto:me@example.com)",
            "[a](Word)",
            "[a](../../../outside.md)",
            "[a](src/a b.rs)",
            "[a]()",
            "f(x)](src/no/closing",
        ];
        for input in cases {
            assert_eq!(rewrite(input), input);
        }
    }

    #[test]
    fn badge_lines_are_dropped_and_nothing_else() {
        let input = "# x\n\n<!-- s -->\n![T](https://img.shields.io/e?url=u)\n\nLayer\n\
                     ![B](https://raw.githubusercontent.com/mmogr/gglib/badges/v.json)\n<!-- e -->\nend";
        assert_eq!(
            strip_badge_lines(input),
            "# x\n\n<!-- s -->\n\nLayer\n<!-- e -->\nend"
        );
    }
}
