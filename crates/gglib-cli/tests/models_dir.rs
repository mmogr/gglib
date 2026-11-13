//! `gglib config set models-dir` and `unset models-dir`, and what the next
//! run resolves.
//!
//! Two runs of the binary, because the point is the second one. It starts in
//! a directory that is not the data root, as every `gglib` does: nothing
//! loads a `.env` into its environment, and the stored directory is read
//! from the data root's `.env` alone.

use std::path::Path;
use std::process::{Command, Output};

/// `gglib config <args>` over `data` as its data root, started in `cwd`, with
/// `models_dir` as the directory its environment names, if any. `data` is
/// its home too, so the default directory is never the user's own, and its
/// `PATH` is a directory that is not there, so `config paths` finds no gglib
/// of this machine's to ask for a version.
fn config(data: &Path, cwd: &Path, models_dir: Option<&Path>, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_gglib"));
    command
        .arg("config")
        .args(args)
        .current_dir(cwd)
        .env("GGLIB_DATA_DIR", data)
        .env("HOME", data)
        .env("PATH", data.join("no-path"))
        .env_remove("GGLIB_MODELS_DIR")
        .env_remove("GGLIB_RESOURCE_DIR");
    if let Some(dir) = models_dir {
        command.env("GGLIB_MODELS_DIR", dir);
    }
    command.output().expect("running `gglib config`")
}

fn text(out: &Output) -> String {
    format!(
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The value `show` prints on its `models-dir` row.
fn shown(show: &Output) -> String {
    let stdout = String::from_utf8_lossy(&show.stdout);
    let row = stdout
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("models-dir "))
        .unwrap_or_else(|| panic!("no models-dir row\n{}", text(show)));
    row.trim().to_owned()
}

#[test]
fn a_models_directory_set_by_one_run_is_the_one_the_next_run_resolves() {
    let data = tempfile::tempdir().expect("temp data dir");
    let elsewhere = tempfile::tempdir().expect("a working directory apart from the data root");
    let before = data.path().join("before");
    let chosen = data.path().join("my models");

    let set = config(
        data.path(),
        elsewhere.path(),
        Some(&before),
        &["set", "models-dir", &chosen.to_string_lossy()],
    );
    assert!(set.status.success(), "the set must succeed\n{}", text(&set));
    assert!(chosen.is_dir(), "the directory is created");

    let show = config(data.path(), elsewhere.path(), None, &["show"]);
    assert!(show.status.success(), "{}", text(&show));
    assert_eq!(
        shown(&show),
        format!("{} (environment, Writable)", chosen.display()),
        "the next run must resolve the stored directory"
    );
}

/// A `set` under a `GGLIB_MODELS_DIR` says the environment outranks what it
/// stored, so it never names a folder `show` then contradicts.
#[test]
fn a_set_under_the_environment_says_the_environment_outranks_it() {
    let data = tempfile::tempdir().expect("temp data dir");
    let elsewhere = tempfile::tempdir().expect("a working directory apart from the data root");
    let from_env = data.path().join("from env");
    let chosen = data.path().join("chosen");

    let set = config(
        data.path(),
        elsewhere.path(),
        Some(&from_env),
        &["set", "models-dir", &chosen.to_string_lossy()],
    );

    assert!(set.status.success(), "{}", text(&set));
    let said = String::from_utf8_lossy(&set.stdout);
    assert!(
        said.contains("outranks it")
            && said.contains(&format!("downloads go to {}", from_env.display())),
        "{}",
        text(&set)
    );
}

/// `unset` removes the stored directory and leaves the other lines, so the
/// next run resolves the default.
#[test]
fn an_unset_models_directory_leaves_the_next_run_on_the_default() {
    let data = tempfile::tempdir().expect("temp data dir");
    let elsewhere = tempfile::tempdir().expect("a working directory apart from the data root");
    let chosen = data.path().join("chosen");
    let kept = "GGLIB_RESOURCE_DIR=\"/somewhere\"";
    let set = config(
        data.path(),
        elsewhere.path(),
        None,
        &["set", "models-dir", &chosen.to_string_lossy()],
    );
    assert!(set.status.success(), "{}", text(&set));
    let env = data.path().join(".env");
    let stored = std::fs::read_to_string(&env).expect("the .env");
    std::fs::write(&env, format!("{stored}{kept}\n")).expect("a line after it");

    let unset = config(
        data.path(),
        elsewhere.path(),
        None,
        &["unset", "models-dir"],
    );
    assert!(unset.status.success(), "{}", text(&unset));
    assert!(
        String::from_utf8_lossy(&unset.stdout).contains("downloads go to the default"),
        "{}",
        text(&unset)
    );

    let show = config(data.path(), elsewhere.path(), None, &["show"]);
    assert!(show.status.success(), "{}", text(&show));
    assert!(shown(&show).contains(" (default, "), "{}", text(&show));
    let env = std::fs::read_to_string(&env).expect("the .env");
    assert!(!env.contains("GGLIB_MODELS_DIR"), "{env}");
    assert!(
        env.lines().any(|l| l == kept),
        "the other line stays\n{env}"
    );
}

/// The line a `set` writes for a directory with a space in its name is one
/// the data root's `.env` reader reads back, and the lines around it stay.
#[test]
fn a_stored_directory_with_a_space_in_its_name_is_read_back_and_the_other_lines_kept() {
    let data = tempfile::tempdir().expect("temp data dir");
    let elsewhere = tempfile::tempdir().expect("a working directory apart from the data root");
    let before = data.path().join("before");
    let chosen = data.path().join("my models");
    let kept = "GGLIB_RESOURCE_DIR=\"/somewhere\"";
    std::fs::write(
        data.path().join(".env"),
        format!("GGLIB_MODELS_DIR=\"/replaced\"\n{kept}\n"),
    )
    .expect("an .env with a line after the models directory");

    let set = config(
        data.path(),
        elsewhere.path(),
        Some(&before),
        &["set", "models-dir", &chosen.to_string_lossy()],
    );
    assert!(set.status.success(), "the set must succeed\n{}", text(&set));

    let paths = config(data.path(), elsewhere.path(), None, &["paths"]);
    assert!(paths.status.success(), "{}", text(&paths));
    let stdout = String::from_utf8_lossy(&paths.stdout);
    for line in [
        format!("models_dir = {}", chosen.display()),
        "models_source = EnvVar".to_owned(),
    ] {
        assert!(
            stdout.lines().any(|printed| printed == line),
            "the next run must print `{line}`\n{}",
            text(&paths)
        );
    }
    let env = std::fs::read_to_string(data.path().join(".env")).expect("the .env");
    assert!(
        env.lines().any(|l| l == kept),
        "the other line must stay\n{env}"
    );
}

/// A path that cannot be a directory is refused in the words the command
/// has always used, and nothing is stored.
#[test]
fn a_refused_models_directory_is_not_stored() {
    let data = tempfile::tempdir().expect("temp data dir");
    let elsewhere = tempfile::tempdir().expect("a working directory apart from the data root");
    let file = data.path().join("a file");
    std::fs::write(&file, "in the way").expect("a file where the folder would go");

    let set = config(
        data.path(),
        elsewhere.path(),
        None,
        &["set", "models-dir", &file.to_string_lossy()],
    );

    assert!(!set.status.success(), "{}", text(&set));
    let refusal = format!("{} exists but is not a directory", file.display());
    assert!(
        String::from_utf8_lossy(&set.stderr).contains(&refusal),
        "{}",
        text(&set)
    );
    assert!(!data.path().join(".env").exists(), "nothing is stored");
}
