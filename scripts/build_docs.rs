use regex::Regex;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const BASE_URL: &str = "https://github.com/mmogr/gglib-rust/blob/main/";

struct DocSpec {
    input: &'static str,
    output: &'static str,
    start_marker: &'static str,
    end_marker: &'static str,
}

const DOC_SPECS: &[DocSpec] = &[
    DocSpec {
        input: "README.md",
        output: "crate_docs.md",
        start_marker: "<!-- crate-docs:start -->",
        end_marker: "<!-- crate-docs:end -->",
    },
    DocSpec {
        input: "src/commands/README.md",
        output: "commands_docs.md",
        start_marker: "<!-- module-docs:start -->",
        end_marker: "<!-- module-docs:end -->",
    },
    DocSpec {
        input: "src/commands/download/README.md",
        output: "commands_download_docs.md",
        start_marker: "<!-- module-docs:start -->",
        end_marker: "<!-- module-docs:end -->",
    },
    DocSpec {
        input: "src/commands/gui_web/README.md",
        output: "commands_gui_web_docs.md",
        start_marker: "<!-- module-docs:start -->",
        end_marker: "<!-- module-docs:end -->",
    },
    DocSpec {
        input: "src/commands/llama/README.md",
        output: "commands_llama_docs.md",
        start_marker: "<!-- module-docs:start -->",
        end_marker: "<!-- module-docs:end -->",
    },
    DocSpec {
        input: "src/services/README.md",
        output: "services_docs.md",
        start_marker: "<!-- module-docs:start -->",
        end_marker: "<!-- module-docs:end -->",
    },
    DocSpec {
        input: "src/models/README.md",
        output: "models_docs.md",
        start_marker: "<!-- module-docs:start -->",
        end_marker: "<!-- module-docs:end -->",
    },
    DocSpec {
        input: "src/proxy/README.md",
        output: "proxy_docs.md",
        start_marker: "<!-- module-docs:start -->",
        end_marker: "<!-- module-docs:end -->",
    },
    DocSpec {
        input: "src/utils/README.md",
        output: "utils_docs.md",
        start_marker: "<!-- module-docs:start -->",
        end_marker: "<!-- module-docs:end -->",
    },
];

/// Process all documentation sources and emit curated markdown files in OUT_DIR.
pub fn process_readme() -> std::io::Result<()> {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));

    let link_regex =
        Regex::new(r"\[((?:[^\[\]]|\[[^\[\]]*\])*)\]\(([^)]*)\)").expect("invalid link regex");

    for spec in DOC_SPECS {
        let input_path = manifest_dir.join(spec.input);
        let output_path = out_dir.join(spec.output);

        let raw = fs::read_to_string(&input_path)?;
        let section = extract_section(&raw, spec.start_marker, spec.end_marker, &input_path);
        let rewritten = rewrite_links(section.trim(), &link_regex);

        fs::write(&output_path, rewritten.trim_end().to_string() + "\n")?;
        println!("cargo:rerun-if-changed={}", input_path.display());
    }

    Ok(())
}

fn extract_section<'a>(
    content: &'a str,
    start_marker: &str,
    end_marker: &str,
    path: &Path,
) -> &'a str {
    let start = content
        .find(start_marker)
        .unwrap_or_else(|| panic!("Missing {} in {}", start_marker, path.display()));
    let after_start = start + start_marker.len();
    let end = content[after_start..]
        .find(end_marker)
        .map(|idx| after_start + idx)
        .unwrap_or_else(|| panic!("Missing {} in {}", end_marker, path.display()));

    if end <= after_start {
        panic!("Markers in {} are out of order", path.display());
    }

    &content[after_start..end]
}

fn rewrite_links(content: &str, regex: &Regex) -> String {
    regex
        .replace_all(content, |caps: &regex::Captures<'_>| {
            let text = &caps[1];
            let url = &caps[2];

            if url.starts_with("http://") || url.starts_with("https://") || url.starts_with('#') {
                format!("[{}]({})", text, url)
            } else {
                let clean = url.trim_start_matches("./");
                let absolute = format!("{}{}", BASE_URL, clean);
                format!("[{}]({})", text, absolute)
            }
        })
        .into_owned()
}
