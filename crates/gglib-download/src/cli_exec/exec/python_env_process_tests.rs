//! Tests of `python_env` that run a program: an interpreter, or the stand-in
//! for one.

use super::*;

/// A stand-in interpreter that answers the version probe with `version`,
/// where an environment directory laid out as a venv keeps its interpreter
/// on this OS: `Scripts\python.exe` on Windows, `bin/python3` elsewhere.
fn environment_reporting(version: &str) -> (tempfile::TempDir, PythonEnvironment) {
    let root = tempfile::tempdir().expect("tempdir");
    let env = PythonEnvironment {
        env_dir: root.path().join(ENV_NAME),
        script_path: root.path().join("helper.py"),
    };
    let python = env.python_path();
    let bin = python.parent().expect("the interpreter's directory");
    fs::create_dir_all(bin).expect("create the interpreter's directory");
    let name = if cfg!(windows) { "python" } else { "python3" };
    let installed =
        gglib_test_proc::install(bin, name, &format!("print /fake/python3\nprint {version}"));

    assert_eq!(
        env.python_path(),
        installed,
        "the stand-in is where the environment looks for its interpreter"
    );
    (root, env)
}

/// The status of a program that exited with `code`.
fn exited_with(code: u8) -> std::process::ExitStatus {
    let dir = tempfile::tempdir().expect("tempdir");
    let builder = gglib_test_proc::install(dir.path(), "builder", &format!("exit {code}"));
    gglib_core::utils::process::cmd(&builder)
        .status()
        .expect("the stand-in runs")
}

/// `create_env` runs the builder through `.output()`, not `.status()`, so
/// its stderr is captured into the error and not inherited by the terminal,
/// where it could corrupt a live `MultiProgress` redraw. This is the
/// formatting half of that: a real exit status and the captured bytes.
#[test]
fn venv_failure_reason_includes_captured_stderr() {
    let reason =
        create_env_failure_reason("venv", exited_with(1), b"NotADirectoryError: [Errno 20]\n");

    assert!(reason.contains("exited with"));
    assert!(reason.contains("NotADirectoryError"));
}

/// Which builder failed decides what the user has to go fix, so the
/// message names it and does not always say `venv`.
#[test]
fn create_env_failure_reason_names_the_builder() {
    let reason = create_env_failure_reason("uv", exited_with(2), b"no interpreter found\n");

    assert!(reason.starts_with("uv exited with"), "{reason}");
    assert!(reason.contains("no interpreter found"), "{reason}");
}

/// Empty stderr must not leave a dangling ": " with nothing after it: the
/// reason is the bare "exited with <status>", whatever the status's own
/// words are on this OS.
#[test]
fn venv_failure_reason_omits_colon_when_stderr_is_empty() {
    let status = exited_with(1);

    let reason = create_env_failure_reason("venv", status, b"");

    assert_eq!(reason, format!("venv exited with {status}"));
}

/// An environment built before the floor rose to 3.10 holds an interpreter
/// the pinned packages will not install against.
#[tokio::test]
async fn an_environment_below_the_floor_is_too_old() {
    let (_root, env) = environment_reporting("3.9");

    assert!(env.interpreter_too_old().await);
}

/// Such an environment is removed before anything is installed into it. The
/// rebuild is pointed at an interpreter that is not there, so it ends there.
#[tokio::test]
async fn an_environment_below_the_floor_is_removed_to_be_built_again() {
    let (root, env) = environment_reporting("3.9");
    let missing = root.path().join("no-such-python");

    let outcome = env.ensure_env_ready(None, Some(&missing)).await;

    assert!(
        matches!(outcome, Err(EnvSetupError::PythonInvalid { .. })),
        "{outcome:?}"
    );
    assert!(!env.env_dir.exists());
}

#[tokio::test]
async fn an_environment_at_the_floor_is_kept() {
    let (_root, env) = environment_reporting("3.10");

    assert!(!env.interpreter_too_old().await);
}

/// A probe that fails for any other reason is not a verdict on the version.
#[tokio::test]
async fn an_environment_with_no_interpreter_is_not_called_too_old() {
    let root = tempfile::tempdir().expect("tempdir");
    let env = PythonEnvironment {
        env_dir: root.path().join(ENV_NAME),
        script_path: root.path().join("helper.py"),
    };

    assert!(!env.interpreter_too_old().await);
}

/// Test that environment isolation properly removes polluted environment variables
/// and sets PYTHONNOUSERSITE=1 to prevent stdlib resolution issues.
///
/// This test simulates a dirty environment by setting polluted variables directly
/// on the Command object, then verifies that `apply_python_subprocess_isolation`
/// removes them and sets PYTHONNOUSERSITE=1.
#[tokio::test]
async fn test_environment_isolation_removes_polluted_vars() {
    // Find a working Python interpreter
    let Ok(python) = which::which("python3").or_else(|_| which::which("python")) else {
        eprintln!("Python not available for test, skipping environment isolation test");
        return;
    };

    // Create a command with a "dirty" environment simulating a conda/virtualenv shell
    let mut cmd = async_cmd(python);

    // Simulate polluted environment by setting variables on the Command
    cmd.env("PYTHONHOME", "/fake/python/home")
        .env("PYTHONPATH", "/fake/python/path")
        .env("PYTHONUSERBASE", "/fake/user/base")
        .env("VIRTUAL_ENV", "/fake/venv")
        .env("CONDA_PREFIX", "/fake/conda")
        .env("CONDA_DEFAULT_ENV", "fake_env")
        .env("CONDA_PROMPT_MODIFIER", "(fake_env)")
        .env("CONDA_SHLVL", "1");

    // Apply our isolation function - this should remove the polluted vars
    apply_python_subprocess_isolation(&mut cmd);

    // Use Python to print its environment variables that we care about
    cmd.arg("-c").arg(
        "import os, sys; \
         print('PYTHONHOME=' + os.getenv('PYTHONHOME', 'UNSET')); \
         print('PYTHONPATH=' + os.getenv('PYTHONPATH', 'UNSET')); \
         print('PYTHONUSERBASE=' + os.getenv('PYTHONUSERBASE', 'UNSET')); \
         print('VIRTUAL_ENV=' + os.getenv('VIRTUAL_ENV', 'UNSET')); \
         print('CONDA_PREFIX=' + os.getenv('CONDA_PREFIX', 'UNSET')); \
         print('CONDA_DEFAULT_ENV=' + os.getenv('CONDA_DEFAULT_ENV', 'UNSET')); \
         print('PYTHONNOUSERSITE=' + os.getenv('PYTHONNOUSERSITE', 'UNSET')); \
         print('SUCCESS')",
    );

    let output = cmd.output().await.expect("Failed to run Python subprocess");

    // Verify the Python subprocess ran successfully
    assert!(
        output.status.success(),
        "Python subprocess failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Assert that all polluted variables were removed (should be UNSET)
    assert!(
        stdout.contains("PYTHONHOME=UNSET"),
        "PYTHONHOME should be removed, got: {stdout}"
    );
    assert!(
        stdout.contains("PYTHONPATH=UNSET"),
        "PYTHONPATH should be removed, got: {stdout}"
    );
    assert!(
        stdout.contains("PYTHONUSERBASE=UNSET"),
        "PYTHONUSERBASE should be removed, got: {stdout}"
    );
    assert!(
        stdout.contains("VIRTUAL_ENV=UNSET"),
        "VIRTUAL_ENV should be removed, got: {stdout}"
    );
    assert!(
        stdout.contains("CONDA_PREFIX=UNSET"),
        "CONDA_PREFIX should be removed, got: {stdout}"
    );
    assert!(
        stdout.contains("CONDA_DEFAULT_ENV=UNSET"),
        "CONDA_DEFAULT_ENV should be removed, got: {stdout}"
    );

    // Assert that PYTHONNOUSERSITE was explicitly set to '1'
    assert!(
        stdout.contains("PYTHONNOUSERSITE=1"),
        "PYTHONNOUSERSITE should be set to '1', got: {stdout}"
    );

    // Verify Python ran successfully (can import encodings)
    assert!(
        stdout.contains("SUCCESS"),
        "Python should successfully import stdlib and print SUCCESS"
    );
}
