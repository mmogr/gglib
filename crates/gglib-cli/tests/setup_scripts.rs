//! `make setup`'s two shell scripts, run on a machine made of stand-ins.
//!
//! Each script gets a `PATH` that holds only what the test put there, so the
//! tools it finds, and the `gglib` it runs, are scripts that record how they
//! were called. Nothing here installs or builds anything.

#![cfg(unix)]

#[path = "support/stub_tools.rs"]
mod stub_tools;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use clap::Parser;
use stub_tools::{Machine, one_at_a_time};

/// The repository's `scripts/` directory.
fn scripts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts")
}

/// Put the real `name` from this machine on `machine`'s `PATH`, for a
/// utility a script needs and no test is about (`head`, `awk`).
fn with_real(machine: Machine, name: &str) -> Machine {
    let real = std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(name))
                .find(|candidate| candidate.is_file())
        })
        .unwrap_or_else(|| panic!("{name} is on this machine's PATH"));
    std::os::unix::fs::symlink(real, machine.path().join(name)).expect("a link to the real tool");
    machine
}

/// Run `script` on `machine`, in an empty directory, with no terminal.
fn run(script: &str, machine: &Machine) -> Output {
    let cwd = tempfile::tempdir().expect("an empty directory to run in");
    Command::new("/bin/bash")
        .arg(scripts_dir().join(script))
        .current_dir(cwd.path())
        .env_clear()
        .env("PATH", machine.path())
        .env("HOME", cwd.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap_or_else(|e| panic!("running {script}: {e}"))
}

fn text(out: &Output) -> String {
    format!(
        "status: {:?}\nstdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A `gglib` that writes down the first line typed at it, as `typed <line>`.
const GGLIB_THAT_ASKS: &str = r#"IFS= read -r answer; printf 'typed %s\n' "$answer" >> "$CALLS""#;

/// A machine with no GPU has no `nvcc`, no `vulkaninfo` and is not a Mac.
/// The script once chose `--cpu-only` there, which the command does not have.
/// And with no terminal the end of input would cancel the install while the
/// make target still reported success, so the script answers the question.
#[test]
fn with_no_gpu_and_no_terminal_the_install_script_runs_an_install_the_command_accepts() {
    let _guard = one_at_a_time();
    let machine = Machine::bare().with("gglib", GGLIB_THAT_ASKS);

    let out = run("install-llama.sh", &machine);

    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(
        machine.calls(),
        ["gglib config llama install", "typed y"],
        "gglib is run once, and its question answered\n{}",
        text(&out)
    );
    gglib_cli::Cli::try_parse_from(["gglib", "config", "llama", "install"])
        .unwrap_or_else(|e| panic!("the command the script runs is one gglib has: {e}"));
}

/// What the command says is what the script says: its failure is the
/// script's, so `make setup` stops where the install was refused.
#[test]
fn the_install_script_fails_when_the_command_does() {
    let _guard = one_at_a_time();
    let machine = Machine::bare().with("gglib", "exit 3");

    let out = run("install-llama.sh", &machine);

    assert_eq!(out.status.code(), Some(3), "{}", text(&out));
}

#[test]
fn the_install_script_stops_when_there_is_no_gglib_to_run() {
    let _guard = one_at_a_time();
    let machine = Machine::bare();

    let out = run("install-llama.sh", &machine);

    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("gglib binary not found"),
        "{}",
        text(&out)
    );
}

/// The toolchains `check-deps.sh` checks, as tools that answer with the
/// banners the real ones print.
fn toolchains(without: &str) -> Machine {
    let mut machine = ["head", "awk", "sed", "grep"]
        .into_iter()
        .fold(Machine::bare(), with_real);
    for (name, banner) in [
        ("cargo", "cargo 1.97.1 (1d8b05cdd 2026-07-20)"),
        ("rustc", "rustc 1.97.1 (82e1608df 2026-07-21)"),
        ("node", "v22.12.0"),
        ("npm", "10.9.0"),
        ("git", "git version 2.43.0"),
        ("make", "GNU Make 4.4.1"),
        ("gcc", "gcc (GCC) 14.2.1 20240910"),
        ("g++", "g++ (GCC) 14.2.1 20240910"),
        ("pkg-config", "0.29.2"),
        ("cmake", "cmake version 3.28.1"),
    ] {
        if name != without {
            machine = machine.with_tool(name, banner);
        }
    }
    machine
}

/// Before there is a gglib the script checks the toolchains and nothing
/// else: no library is asked of pkg-config and no GPU is looked for. It says
/// which command holds the rest of the list.
#[test]
fn before_gglib_is_built_the_dependency_script_checks_only_the_toolchains() {
    let _guard = one_at_a_time();
    let machine = toolchains("");

    let out = run("check-deps.sh", &machine);

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{}", text(&out));
    for row in [
        "cargo",
        "rustc",
        "node",
        "npm",
        "git",
        "make",
        "gcc",
        "g++",
        "pkg-config",
        "cmake",
    ] {
        assert!(
            stdout
                .lines()
                .any(|line| line.starts_with(row) && line.contains('✓')),
            "a row for {row}\n{}",
            text(&out)
        );
    }
    for gone in [
        "libssl", "python", "libcurl", "patchelf", "webkit", "CUDA", "Vulkan", "GPU", "Metal",
    ] {
        assert!(
            !stdout.lines().any(|line| line.starts_with(gone)),
            "no row for {gone}\n{}",
            text(&out)
        );
    }
    assert!(
        stdout.contains("cargo run -p gglib-cli -- config check-deps"),
        "{}",
        text(&out)
    );

    let calls = machine.calls();
    assert!(
        calls.contains(&"pkg-config --version".to_owned()),
        "{calls:?}"
    );
    assert!(
        calls.iter().all(|call| call.ends_with(" --version")),
        "each tool is asked its version and nothing more: {calls:?}"
    );
}

/// Once a gglib exists, the rest of the list is that binary's, and so is the
/// verdict: the script ends as `gglib config check-deps` does.
#[test]
fn once_gglib_is_built_the_dependency_script_hands_over_to_it() {
    let _guard = one_at_a_time();
    let machine = toolchains("").with("gglib", "echo 'the rest of the list'; exit 7");

    let out = run("check-deps.sh", &machine);

    assert_eq!(out.status.code(), Some(7), "{}", text(&out));
    assert!(
        machine
            .calls()
            .contains(&"gglib config check-deps".to_owned()),
        "{}",
        text(&out)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("the rest of the list"),
        "{}",
        text(&out)
    );
    gglib_cli::Cli::try_parse_from(["gglib", "config", "check-deps"])
        .unwrap_or_else(|e| panic!("the command handed over to is one gglib has: {e}"));
}

/// A missing toolchain stops the script there, with how to install it, and
/// the rest of the list is not asked for.
#[test]
fn a_missing_toolchain_fails_the_dependency_script_before_any_hand_over() {
    let _guard = one_at_a_time();
    let machine = toolchains("cmake").with("gglib", "exit 0");

    let out = run("check-deps.sh", &machine);

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        stdout
            .lines()
            .any(|line| line.starts_with("cmake") && line.contains("MISSING")),
        "{}",
        text(&out)
    );
    assert!(stdout.contains("Install build tools"), "{}", text(&out));
    assert!(
        !machine
            .calls()
            .iter()
            .any(|call| call.starts_with("gglib ")),
        "{}",
        text(&out)
    );
}

/// What `make <target>` would run: `make -n` prints the recipes without
/// running them.
fn dry_run(target: &str) -> String {
    let out = Command::new("make")
        .args(["-n", target])
        .current_dir(scripts_dir().join(".."))
        .output()
        .expect("make is on this machine");
    assert!(out.status.success(), "{}", text(&out));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A build installs what `package-lock.json` says and leaves the file
/// alone. `npm install` rewrites it, and a modified lockfile stops a
/// `git pull` that changes it.
#[test]
fn a_build_installs_with_npm_ci() {
    for target in ["build-gui", "build-tauri"] {
        let listed = dry_run(target);
        let installs: Vec<&str> = listed
            .lines()
            .filter(|line| line.ends_with("npm ci") || line.ends_with("npm install"))
            .collect();
        assert_eq!(
            installs,
            ["UV_USE_IO_URING=0 npm ci"],
            "{target}:\n{listed}"
        );
    }
}

/// `make setup` is the same steps in the same order.
#[test]
fn make_setup_runs_its_steps_in_the_order_it_always_has() {
    let listed = dry_run("setup");

    // A step, and how often it is printed: bundling prints both its arms.
    let steps = [
        ("./scripts/check-deps.sh", 1),
        ("npm ci", 1),
        ("npm run build:tauri", 1),
        ("build --release -p gglib-cli -p gglib-app", 1),
        ("npm run tauri:bundle", 2),
        ("cp target/release/gglib", 1),
        ("./target/release/gglib config models-dir prompt", 1),
        ("./target/release/gglib config fast-downloads prompt", 1),
        ("llama-install-auto", 1),
        ("scripts/install-llama.sh", 1),
    ];
    let mut from = 0;
    for (step, times) in steps {
        assert_eq!(
            listed.matches(step).count(),
            times,
            "{step:?} in:\n{listed}"
        );
        let at = listed[from..]
            .find(step)
            .unwrap_or_else(|| panic!("{step:?} comes next in:\n{listed}"));
        from += at + step.len();
    }
}
