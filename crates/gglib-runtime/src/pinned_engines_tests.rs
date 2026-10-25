//! Every flag gglib starts an engine with, and every fixed value it gives
//! one, is one the pinned engine's own `--help` lists, and not as removed or
//! deprecated.
//!
//! The flags are gathered from gglib's own builders, not kept in a list:
//! `llama-server`'s from `build_command` with every option on, `sd-server`'s
//! from `sd_server_args` for every image family with every component, and
//! `llama-bench`'s from the benchmark's `.arg("…")` literals. Two checks here
//! hold the builders' own flag literals to what was gathered, so a flag added
//! to a builder as a literal is caught. `--version`, which gglib runs both
//! servers with to read what they are, is not here: sd-server answers it
//! before its option parser and its `--help` does not list it, so CI runs both
//! `--version`s itself instead.
//!
//! The pinned binaries are not here. CI installs them, saves each one's
//! `--help` in `GGLIB_ENGINE_HELP_DIR`, lists each pinned release's assets in
//! `GGLIB_ENGINE_ASSETS_DIR`, and runs this crate's ignored tests:
//! `cargo test -p gglib-runtime --lib -- --ignored`. The asset checks sit
//! beside each engine's platform table and read their list through
//! [`pinned_release_assets`].

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use gglib_core::cache_config::KvCacheType;
use gglib_core::domain::{ComponentRole, ImageFamily, ModelComponent};
use gglib_core::ports::{JinjaMode, ServerConfig};

use crate::command::build_command;
use crate::llama::args::REASONING_FORMATS;
use crate::sd::{SdServerConfig, sd_server_args};

/// What a binary is started with: each flag, and the fixed values given it.
type Uses = BTreeMap<String, BTreeSet<String>>;

/// Whether `token` is a flag: a dash, or two, then a letter.
fn is_flag(token: &str) -> bool {
    let name = token.strip_prefix("--").or_else(|| token.strip_prefix('-'));
    name.is_some_and(|name| name.starts_with(|c: char| c.is_ascii_alphabetic()))
}

/// Whether `value` is a fixed word, such as `q8_0` or `mmap+mlock`, rather
/// than a path, an address or a number, which the help cannot list.
fn is_word(value: &str) -> bool {
    value.starts_with(|c: char| c.is_ascii_lowercase())
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_+-.".contains(c))
}

/// Add an argument list's flags, and each flag's fixed value, to `uses`.
fn gather<'a>(uses: &mut Uses, argv: impl IntoIterator<Item = &'a str>) {
    let argv: Vec<&str> = argv.into_iter().collect();
    for (i, token) in argv.iter().enumerate() {
        if !is_flag(token) {
            continue;
        }
        let values = uses.entry((*token).to_owned()).or_default();
        if let Some(value) = argv.get(i + 1).filter(|v| !is_flag(v) && is_word(v)) {
            values.insert((*value).to_owned());
        }
    }
}

/// `llama-server`'s, from launches with every option on: jinja on and off,
/// each reasoning format, and each KV cache type. The config is written out
/// field by field, so a field added to it does not compile here until this
/// says what it is set to.
fn llama_server_uses() -> Uses {
    let mut uses = Uses::new();
    let mut launch = |edit: &dyn Fn(&mut ServerConfig)| {
        let mut config = ServerConfig {
            model_id: 1,
            model_name: "m".to_owned(),
            model_path: PathBuf::from("/m/model.gguf"),
            mmproj: Some(PathBuf::from("/m/projector.gguf")),
            port: None,
            base_port: 9000,
            context_size: Some(4096),
            jinja: JinjaMode::Defer,
            reasoning_format: None,
            spec_draft_n_max: Some(3),
            spec_draft_p_min: Some(0.75),
            slot_save_path: Some(PathBuf::from("/m/slots")),
            cache_ram_mb: Some(1024),
            cache_reuse: Some(256),
            cache_type_k: None,
            cache_type_v: None,
            mlock: true,
            embeddings: true,
        };
        edit(&mut config);
        let command = build_command(Path::new("/m/llama-server"), &config, 5500);
        let argv: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        gather(&mut uses, argv.iter().map(String::as_str));
    };
    for jinja in [JinjaMode::On, JinjaMode::Off] {
        launch(&|config| config.jinja = jinja);
    }
    for format in REASONING_FORMATS {
        launch(&|config| config.reasoning_format = Some(format.to_owned()));
    }
    for &kind in KvCacheType::ALL {
        launch(&|config| {
            config.cache_type_k = Some(kind);
            config.cache_type_v = Some(kind);
        });
    }
    uses
}

/// `sd-server`'s, from every image family with a file in every role.
fn sd_server_uses() -> Uses {
    let mut uses = Uses::new();
    for family in ImageFamily::ALL {
        let config = SdServerConfig {
            model_id: 1,
            model_name: "m".to_owned(),
            model_path: PathBuf::from("/m/model.gguf"),
            family,
            components: ComponentRole::ALL
                .into_iter()
                .map(|role| ModelComponent {
                    role,
                    path: PathBuf::from("/m/component.gguf"),
                })
                .collect(),
            port: None,
        };
        let argv = sd_server_args(&config, 9123);
        gather(&mut uses, argv.iter().filter_map(|arg| OsStr::to_str(arg)));
    }
    uses
}

/// The string literals of `.arg("…")` calls in `source`, in order.
fn arg_literals(source: &str) -> Vec<&str> {
    source
        .split(".arg(\"")
        .skip(1)
        .filter_map(|rest| rest.split_once("\")").map(|(literal, _)| literal))
        .collect()
}

/// The part of a source file before its tests.
fn before_tests(source: &str) -> &str {
    source.split("#[cfg(test)]").next().unwrap_or(source)
}

/// The benchmark that starts `llama-bench`, before its tests.
const BENCHMARK: &str = include_str!("../../gglib-app-services/src/benchmark/perf.rs");

/// `llama-bench`'s, from the benchmark's `.arg("…")` literals.
fn llama_bench_uses() -> Uses {
    let mut uses = Uses::new();
    gather(&mut uses, arg_literals(before_tests(BENCHMARK)));
    uses
}

/// Every flag literal `build_command` names is among those gathered.
fn assert_the_llama_server_builder_is_gathered(gathered: &Uses) {
    let source = before_tests(include_str!("command.rs"));
    let literals: Vec<&str> = arg_literals(source)
        .into_iter()
        .filter(|l| is_flag(l))
        .collect();
    assert!(
        literals.contains(&"-m") && literals.contains(&"--load-mode"),
        "{literals:?}"
    );
    for literal in literals {
        assert!(gathered.contains_key(literal), "{literal} is not gathered");
    }
}

/// Every flag literal `sd_server_args` names, and each component role's
/// flag, is among those gathered.
fn assert_the_sd_server_builder_is_gathered(gathered: &Uses) {
    let source = before_tests(include_str!("sd/server/args.rs"));
    let literals: Vec<&str> = source
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|literal| is_flag(literal))
        .collect();
    assert!(literals.contains(&"--diffusion-model") && literals.contains(&"--listen-port"));
    for literal in literals {
        assert!(gathered.contains_key(literal), "{literal} is not gathered");
    }
    for role in ComponentRole::ALL {
        let flag = format!("--{}", role.as_str());
        assert!(gathered.contains_key(&flag), "{flag} is not gathered");
    }
}

/// One option a `--help` lists: its flags, and everything said of it.
#[derive(Debug)]
struct HelpOption {
    flags: Vec<String>,
    text: String,
}

/// The options a `--help` lists. An option's line starts, after an indent of
/// at most eight spaces, with a flag. Its flags are the line's leading flag
/// tokens (`-ctk,  --cache-type-k TYPE` gives `-ctk` and `--cache-type-k`).
/// The line and every deeper line after it are what it says of itself.
fn help_options(help: &str) -> Vec<HelpOption> {
    let mut options: Vec<HelpOption> = Vec::new();
    for line in help.lines() {
        let body = line.trim_start();
        let indent = line.len() - body.len();
        let first = body.split_whitespace().next().unwrap_or("");
        if indent <= 8 && is_flag(first.trim_end_matches(',')) {
            let mut flags = Vec::new();
            for token in body.split_whitespace() {
                let token = token.trim_end_matches(',');
                let name = token.split('=').next().unwrap_or(token);
                if !is_flag(name) {
                    break;
                }
                flags.push(name.to_owned());
            }
            options.push(HelpOption {
                flags,
                text: body.to_owned(),
            });
        } else if let Some(option) = options.last_mut() {
            option.text.push('\n');
            option.text.push_str(body);
        }
    }
    options
}

/// What `help` does not bear out of `uses`: a flag it does not list, one it
/// says has been removed (llama.cpp keeps a retired flag in its `--help` with
/// that text, and refuses it at launch), one it calls deprecated, or a fixed
/// value its option never names. Removed and deprecated are read from the
/// option's own line, where both binaries say them, so a deprecated value in
/// the list below it does not count against the flag.
fn problems(binary: &str, help: &str, uses: &Uses) -> Vec<String> {
    let options = help_options(help);
    let mut problems = Vec::new();
    for (flag, values) in uses {
        let Some(option) = options.iter().find(|o| o.flags.iter().any(|f| f == flag)) else {
            problems.push(format!("{binary} --help lists no {flag}"));
            continue;
        };
        let said = option.text.lines().next().unwrap_or("").to_lowercase();
        if said.contains("has been removed") {
            problems.push(format!("{binary} --help says {flag} has been removed"));
        }
        if said.contains("deprecated") {
            problems.push(format!("{binary} --help calls {flag} deprecated"));
        }
        let words: BTreeSet<&str> = option
            .text
            .split(|c: char| !(c.is_ascii_alphanumeric() || "_+-.".contains(c)))
            .map(|word| word.trim_end_matches('.'))
            .collect();
        for value in values {
            if !words.contains(value.as_str()) {
                problems.push(format!("{binary} --help names no {value} for {flag}"));
            }
        }
    }
    problems
}

/// A `--help` in llama.cpp's layout: flags at the margin, descriptions in a
/// column, continuation lines indented to it.
const LLAMA_STYLE: &str = "\
----- common params -----

-h,    --help, --usage                  print usage and exit
-c,    --ctx-size N                     size of the prompt context (default: 4096)
                                        --ctx-size 0 takes the model's own
-ctk,  --cache-type-k TYPE              KV cache data type for K
                                        allowed values: f32, f16, bf16, q8_0, q4_0.
--reasoning-format FORMAT               one of:
                                        - none: leaves thoughts unparsed
                                        - deepseek: puts thoughts in reasoning_content
                                        - deepseek-legacy: deprecated, keeps the tags too
--old-flag                              deprecated, use --new-flag
--draft-max N                           the argument has been removed. use --spec-draft-n-max
-fa,   --flash-attn [on|off|auto]       set Flash Attention use
";

/// A `--help` in a getopt-like layout, indented, values in angle brackets.
const SD_STYLE: &str = "\
Options:
  -m, --model <string>                     path to full model
  --sampling-method <string>               sampling method, one of [euler, euler_a, heun]
  --fa                                     use flash attention
";

fn uses(entries: &[(&str, &[&str])]) -> Uses {
    entries
        .iter()
        .map(|(flag, values)| {
            let values = values.iter().map(|v| (*v).to_owned()).collect();
            ((*flag).to_owned(), values)
        })
        .collect()
}

/// Either name of a flag is found, and a value is found in its own option's
/// text, the last of a sentence included.
#[test]
fn a_help_bears_out_flags_by_either_name_and_values_in_their_own_option() {
    let llama = uses(&[
        ("-c", &[]),
        ("--ctx-size", &[]),
        ("-ctk", &["q8_0"]),
        ("--cache-type-k", &["f16", "q4_0"]),
        ("--reasoning-format", &["none", "deepseek"]),
        ("--flash-attn", &["auto"]),
    ]);
    assert_eq!(
        problems("llama-server", LLAMA_STYLE, &llama),
        Vec::<String>::new()
    );
    let sd = uses(&[
        ("-m", &[]),
        ("--sampling-method", &["euler_a"]),
        ("--fa", &[]),
    ]);
    assert_eq!(problems("sd-server", SD_STYLE, &sd), Vec::<String>::new());
}

#[test]
fn a_help_without_a_flag_or_its_value_or_retiring_it_says_so() {
    let wanted = uses(&[
        ("--gone", &[]),
        ("--cache-type-k", &["q9_9"]),
        ("--ctx-size", &["deepseek"]),
        ("--old-flag", &[]),
        ("--draft-max", &[]),
    ]);
    assert_eq!(
        problems("llama-server", LLAMA_STYLE, &wanted),
        [
            "llama-server --help names no q9_9 for --cache-type-k",
            "llama-server --help names no deepseek for --ctx-size",
            "llama-server --help says --draft-max has been removed",
            "llama-server --help lists no --gone",
            "llama-server --help calls --old-flag deprecated",
        ]
    );
}

/// A description line is the option above's, not an option of its own, even
/// when it starts with a dash, as `- none: …` does, or with a flag, as the
/// line under `--ctx-size` does.
#[test]
fn a_description_line_belongs_to_its_option_whatever_it_starts_with() {
    let options = help_options(LLAMA_STYLE);
    let reasoning = options
        .iter()
        .find(|o| o.flags == ["--reasoning-format"])
        .expect("listed");
    assert!(reasoning.text.contains("- deepseek:"), "{reasoning:?}");
    let context: Vec<&HelpOption> = options
        .iter()
        .filter(|o| o.flags.iter().any(|f| f == "--ctx-size"))
        .collect();
    assert_eq!(context.len(), 1, "{options:?}");
    assert!(context[0].text.contains("takes the model's own"));
}

#[test]
fn every_flag_the_llama_server_builder_names_is_gathered() {
    let gathered = llama_server_uses();
    assert_the_llama_server_builder_is_gathered(&gathered);
    assert!(gathered["--cache-type-k"].contains("q4_1"), "{gathered:?}");
    assert!(gathered["--reasoning-format"].contains("deepseek-legacy"));
    assert!(gathered["--load-mode"].contains("mmap+mlock"));
}

#[test]
fn every_flag_the_sd_server_builder_names_is_gathered() {
    let gathered = sd_server_uses();
    assert_the_sd_server_builder_is_gathered(&gathered);
    assert!(
        gathered["--sampling-method"].contains("euler_a"),
        "{gathered:?}"
    );
}

/// The benchmark names each flag in its own `.arg("…")`, which is all the
/// gathering reads; an `.args(…)` there would hide flags from it.
#[test]
fn the_benchmark_starts_llama_bench_with_its_five_flags() {
    assert!(!before_tests(BENCHMARK).contains(".args("));
    let gathered = llama_bench_uses();
    assert_eq!(
        gathered.keys().map(String::as_str).collect::<Vec<_>>(),
        ["-m", "-n", "-o", "-p", "-r"]
    );
    assert!(gathered["-o"].contains("json"));
}

/// The pinned binaries' own word on every flag and value gglib uses. CI
/// installs them and saves each `--help` as `<binary>.txt` in
/// `GGLIB_ENGINE_HELP_DIR`. The builders are held to what is gathered here
/// too, so a kill switch set in the environment cannot shrink it unseen.
#[test]
#[ignore = "reads the pinned binaries' --help; CI sets GGLIB_ENGINE_HELP_DIR"]
fn the_pinned_engines_list_every_flag_and_value_gglib_starts_them_with() {
    let dir = std::env::var_os("GGLIB_ENGINE_HELP_DIR")
        .map(PathBuf::from)
        .expect("GGLIB_ENGINE_HELP_DIR names the folder of saved --help texts");
    let llama_server = llama_server_uses();
    assert_the_llama_server_builder_is_gathered(&llama_server);
    let sd_server = sd_server_uses();
    assert_the_sd_server_builder_is_gathered(&sd_server);
    let mut found = Vec::new();
    for (binary, uses) in [
        ("llama-server", llama_server),
        ("llama-bench", llama_bench_uses()),
        ("sd-server", sd_server),
    ] {
        let path = dir.join(format!("{binary}.txt"));
        let help =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(!help_options(&help).is_empty(), "{binary}: no options read");
        found.extend(problems(binary, &help, &uses));
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}

/// The asset names of a pinned release, one per line in `file` in
/// `GGLIB_ENGINE_ASSETS_DIR`, as `gh release view` lists them for CI.
pub(crate) fn pinned_release_assets(file: &str) -> Vec<String> {
    let dir = std::env::var_os("GGLIB_ENGINE_ASSETS_DIR")
        .map(PathBuf::from)
        .expect("GGLIB_ENGINE_ASSETS_DIR names the folder of asset lists");
    let path = dir.join(file);
    let names: Vec<String> = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    assert!(!names.is_empty(), "{}: no assets listed", path.display());
    names
}
