//! Tests of `python_env` that run an interpreter.

use super::*;

/// How long a stand-in may stay busy before the helper stops waiting for it.
#[cfg(unix)]
const BUSY_LIMIT: std::time::Duration = std::time::Duration::from_secs(10);

/// What `run` answers once it no longer fails because the program is busy.
///
/// Linux does not run a file that a process has open for writing. A test
/// binary whose other tests spawn processes can fork while this thread is
/// writing a script, and the child holds a copy of that descriptor until it
/// execs. No copy is left once a run has started: the writer closed its own
/// before the first attempt, so a later fork inherits none.
#[cfg(unix)]
fn once_not_busy<T>(mut run: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let started = std::time::Instant::now();
    loop {
        match run() {
            Err(e)
                if e.kind() == std::io::ErrorKind::ExecutableFileBusy
                    && started.elapsed() < BUSY_LIMIT =>
            {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            outcome => return outcome,
        }
    }
}

/// A script at `python` that answers the version probe with `version`. It
/// has run once by the time this returns, so the next run of it starts.
#[cfg(unix)]
fn write_stand_in(python: &Path, version: &str) {
    fs::write(
        python,
        format!("#!/bin/sh\necho /fake/python3\necho {version}\n"),
    )
    .expect("write the stand-in");
    fs::set_permissions(python, fs::Permissions::from_mode(0o755)).expect("make it executable");
    once_not_busy(|| std::process::Command::new(python).output()).expect("the stand-in runs");
}

/// A stand-in interpreter: a script that answers the version probe with
/// `version`, in an environment directory laid out as a venv is.
#[cfg(unix)]
fn environment_reporting(version: &str) -> (tempfile::TempDir, PythonEnvironment) {
    let root = tempfile::tempdir().expect("tempdir");
    let env_dir = root.path().join(ENV_NAME);
    let python = env_dir.join("bin").join("python3");
    fs::create_dir_all(python.parent().expect("bin directory")).expect("create bin");
    write_stand_in(&python, version);

    let env = PythonEnvironment {
        env_dir,
        script_path: root.path().join("helper.py"),
    };
    (root, env)
}

/// A program the kernel calls busy is run again until a run starts, and what
/// that run answers is handed back.
#[cfg(unix)]
#[test]
fn a_busy_program_is_run_again_until_it_starts() {
    let mut runs = 0;

    let outcome = once_not_busy(|| {
        runs += 1;
        if runs < 3 {
            Err(std::io::ErrorKind::ExecutableFileBusy.into())
        } else {
            Ok(runs)
        }
    });

    assert_eq!(outcome.expect("the third run starts"), 3);
}

/// Only a busy program is waited for: any other failure is the answer.
#[cfg(unix)]
#[test]
fn a_program_that_fails_for_another_reason_is_not_run_again() {
    let mut runs = 0;

    let outcome = once_not_busy(|| -> std::io::Result<()> {
        runs += 1;
        Err(std::io::ErrorKind::NotFound.into())
    });

    assert_eq!(
        outcome.expect_err("it does not start").kind(),
        std::io::ErrorKind::NotFound
    );
    assert_eq!(runs, 1);
}

/// A stand-in another thread still has open for writing is not handed back
/// until that thread lets go of it. That is so wherever the kernel refuses to
/// run such a file; macOS runs it, and there is nothing to wait out there.
#[cfg(unix)]
#[test]
fn a_stand_in_still_open_for_writing_is_waited_out() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let root = tempfile::tempdir().expect("tempdir");
    let python = root.path().join("python3");
    write_stand_in(&python, "3.9");
    let held = fs::OpenOptions::new()
        .append(true)
        .open(&python)
        .expect("open the stand-in for writing");
    let refused = std::process::Command::new(&python).output();
    if !matches!(&refused, Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy) {
        return;
    }
    let let_go = Arc::new(AtomicBool::new(false));
    let holder = std::thread::spawn({
        let let_go = Arc::clone(&let_go);
        move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let_go.store(true, Ordering::SeqCst);
            drop(held);
        }
    });

    write_stand_in(&python, "3.9");

    assert!(
        let_go.load(Ordering::SeqCst),
        "handed back while still open for writing"
    );
    holder.join().expect("the holder ends");
}

/// An environment built before the floor rose to 3.10 holds an interpreter
/// the pinned packages will not install against.
#[cfg(unix)]
#[tokio::test]
async fn an_environment_below_the_floor_is_too_old() {
    let (_root, env) = environment_reporting("3.9");

    assert!(env.interpreter_too_old().await);
}

/// Such an environment is removed before anything is installed into it. The
/// rebuild is pointed at an interpreter that is not there, so it ends there.
#[cfg(unix)]
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

#[cfg(unix)]
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
