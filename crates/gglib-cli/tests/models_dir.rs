//! `gglib config models-dir set` and `prompt`, and what the next run resolves.
//!
//! Two runs of the binary, because the point is the second one. It starts
//! either in a directory that is not the data root, as an installed `gglib`
//! does, so nothing loads the stored directory into its environment for it,
//! or in the data root, as a run from the checkout does, so the whole `.env`
//! there is loaded and has to survive the line the first run wrote.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// `gglib config <args>` over `data` as its data root, started in `cwd`, with
/// `models_dir` as the directory its environment names, if any, and `typed`
/// on its standard input, if anything.
fn config(
    data: &Path,
    cwd: &Path,
    models_dir: Option<&Path>,
    args: &[&str],
    typed: Option<&str>,
) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_gglib"));
    command
        .arg("config")
        .args(args)
        .current_dir(cwd)
        .env("GGLIB_DATA_DIR", data)
        .env_remove("GGLIB_MODELS_DIR")
        .env_remove("GGLIB_RESOURCE_DIR")
        .stdin(typed.map_or_else(Stdio::null, |_| Stdio::piped()))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = models_dir {
        command.env("GGLIB_MODELS_DIR", dir);
    }
    let mut child = command.spawn().expect("starting `gglib config`");
    if let Some(typed) = typed {
        let mut stdin = child.stdin.take().expect("its standard input");
        stdin.write_all(typed.as_bytes()).expect("typing at it");
    }
    child.wait_with_output().expect("running `gglib config`")
}

fn text(out: &Output) -> String {
    format!(
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// What `show` prints for a directory the store, not a default, supplied.
fn shown(dir: &Path) -> String {
    format!(
        "Current models directory: {} (source: EnvVar)",
        dir.display()
    )
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
        &["models-dir", "set", &chosen.to_string_lossy()],
        None,
    );
    assert!(set.status.success(), "the set must succeed\n{}", text(&set));
    assert!(chosen.is_dir(), "the directory is created");

    let show = config(
        data.path(),
        elsewhere.path(),
        None,
        &["models-dir", "show"],
        None,
    );
    assert!(show.status.success(), "{}", text(&show));
    assert!(
        String::from_utf8_lossy(&show.stdout).contains(&shown(&chosen)),
        "the next run must resolve the stored directory\n{}",
        text(&show)
    );
}

/// `prompt` stores the directory typed at it, as `set` stores its argument.
#[test]
fn a_models_directory_typed_at_the_prompt_is_the_one_the_next_run_resolves() {
    let data = tempfile::tempdir().expect("temp data dir");
    let elsewhere = tempfile::tempdir().expect("a working directory apart from the data root");
    let before = data.path().join("before");
    let chosen = data.path().join("typed models");

    let prompt = config(
        data.path(),
        elsewhere.path(),
        Some(&before),
        &["models-dir", "prompt"],
        Some(&format!("{}\n", chosen.display())),
    );
    assert!(prompt.status.success(), "{}", text(&prompt));
    assert!(chosen.is_dir(), "the directory is created");

    let show = config(
        data.path(),
        elsewhere.path(),
        None,
        &["models-dir", "show"],
        None,
    );
    assert!(show.status.success(), "{}", text(&show));
    assert!(
        String::from_utf8_lossy(&show.stdout).contains(&shown(&chosen)),
        "the next run must resolve the directory typed at the prompt\n{}",
        text(&show)
    );
}

/// A run started in the data root loads the `.env` there into its
/// environment, and loading stops at a line it cannot read. The line a `set`
/// writes for a directory with a space in its name is one it can: the next
/// run has that directory from it, and the line after it as well.
#[test]
fn a_stored_directory_with_a_space_in_its_name_leaves_the_rest_of_the_env_file_loading() {
    let data = tempfile::tempdir().expect("temp data dir");
    let elsewhere = tempfile::tempdir().expect("a working directory apart from the data root");
    let before = data.path().join("before");
    let chosen = data.path().join("my models");
    // Forward slashes on every platform, so the line needs no escaping.
    let resources = data.path().join("res").to_string_lossy().replace('\\', "/");
    std::fs::write(
        data.path().join(".env"),
        format!("GGLIB_MODELS_DIR=\"/replaced\"\nGGLIB_RESOURCE_DIR=\"{resources}\"\n"),
    )
    .expect("an .env with a line after the models directory");

    let set = config(
        data.path(),
        elsewhere.path(),
        Some(&before),
        &["models-dir", "set", &chosen.to_string_lossy()],
        None,
    );
    assert!(set.status.success(), "the set must succeed\n{}", text(&set));

    let paths = config(data.path(), data.path(), None, &["paths"], None);
    assert!(paths.status.success(), "{}", text(&paths));
    let stdout = String::from_utf8_lossy(&paths.stdout);
    for line in [
        format!("models_dir = {}", chosen.display()),
        "models_source = EnvVar".to_owned(),
        format!("resource_root = {resources}"),
    ] {
        assert!(
            stdout.lines().any(|printed| printed == line),
            "the next run must print `{line}`\n{}",
            text(&paths)
        );
    }
}

/// A directory that is absent is refused under `--no-create`, in the words
/// the command has always used, and the next run resolves what it did before.
#[test]
fn a_refused_models_directory_is_not_stored() {
    let data = tempfile::tempdir().expect("temp data dir");
    let elsewhere = tempfile::tempdir().expect("a working directory apart from the data root");
    let before = data.path().join("before");
    let absent = data.path().join("absent");

    let set = config(
        data.path(),
        elsewhere.path(),
        Some(&before),
        &[
            "models-dir",
            "set",
            &absent.to_string_lossy(),
            "--no-create",
        ],
        None,
    );

    assert!(!set.status.success(), "{}", text(&set));
    let refusal = format!("Directory {} does not exist", absent.display());
    assert!(
        String::from_utf8_lossy(&set.stderr).contains(&refusal),
        "{}",
        text(&set)
    );
    assert!(!absent.exists());
    assert!(!data.path().join(".env").exists(), "nothing is stored");
}
