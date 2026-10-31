#!/usr/bin/env python3
"""Decide which of CI's macOS and Windows jobs a change can reach.

    scripts/ci_scope.py scope --base <ref> --head <ref> [--event pull_request|push] [--pr-count N]
    scripts/ci_scope.py --self-test

`scope` runs the self-test, then compares two commits and prints, on stdout
and nothing else, one `name=true` or `name=false` line per output, for
$GITHUB_OUTPUT. Its reasons go to stderr.

* `version_only`: the two Cargo.toml versions differ, no file was added,
  deleted or renamed, and every changed line of every changed file but
  CHANGELOG.md is its old line with the old version replaced by the new one.
  A release pull request, a promotion and a bump commit are; a change that
  moves anything else is not.
* `rust`: not `version_only`, and some changed path is outside INERT below.
  Everything not listed there counts as reaching a Rust build.
* `ffi`: not `version_only`, and scripts/apple/ffi-inputs-changed.sh says the
  change reaches what `ffi-apple` builds. That script needs cargo.
* `ios`: `rust`. The Makefile's PHONE_CRATES holds gglib-agent, which is
  outside gglib-ffi's closure, so `ffi` is not enough.
* `lock`: not `version_only`, and Cargo.lock or rust-toolchain.toml changed.
* `cross_os`: on a pull request, `rust`. On a push, `lock`, so main saves the
  cross-OS caches for the lockfile the next pull requests restore; or `rust`
  when no pull request is behind the commit (`--pr-count 0`), since an admin
  can push to main without one and no pull request CI has seen it.
* `apple`: `ffi`.

What it does NOT decide, so that nobody reads more into a `false` than is there:

* Whether a Linux job runs. Every Linux job runs on every change.
* Anything about a path in INERT beyond what its comment says. The
  frontend reaches a Rust build only as `web_ui/`, which the Linux `clippy`
  job builds and embeds; a frontend change that breaks only the macOS or
  Windows build is left to the release builds.

The self-test builds a git repository per case in a temporary directory,
with ffi-inputs-changed.sh replaced by a stand-in so it needs no cargo, and
passes when every case gives exactly its planted outputs.

Exit codes:
    0  printed the outputs, or the self-test passed
    1  the self-test failed, git failed, or the arguments were refused
"""

from __future__ import annotations

import argparse
import fnmatch
import os
import re
import subprocess
import sys
import tempfile
import tomllib
from collections.abc import Callable

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FFI_SCRIPT = "scripts/apple/ffi-inputs-changed.sh"
OUTPUTS = ("version_only", "rust", "ffi", "ios", "lock", "cross_os", "apple")

# Paths inside INERT below that do reach a gated job, checked first.
REACHES = [
    # The two Apple jobs run these through the Makefile.
    "scripts/apple/**",
    # The pinned-engines and llama-xcframework actions read the pin with it.
    "scripts/llama_pin.sh",
    # Defines the gated jobs themselves.
    ".github/workflows/ci.yml",
    # The pinned engines, llama.xcframework and the test models the gated
    # jobs install.
    ".github/actions/**",
]

# Paths that cannot reach a Rust build on macOS, Windows or iOS. A path ending
# `/**` is a directory; any other is matched at its own depth, so `*.md` is the
# root's Markdown only and a crate's README, its rustdoc front page, reaches.
INERT = [
    # Nothing compiles a file here in. Two integration tests read one when
    # they run, and only Linux's `test` runs them.
    "docs/**",
    # The repository's own README, CHANGELOG and the rest. Each crate's front
    # page is its own README.
    "*.md",
    # The TypeScript frontend. It reaches Rust only as `web_ui/`, which the
    # Linux `clippy` job builds and embeds through gglib-app.
    "src/**",
    "tests/ts/**",
    "index.html",
    "tray.html",
    "vite.config.ts",
    # npm's manifests. tauri.conf.json names no version, so gglib-app takes
    # its version from Cargo.toml, not package.json.
    "package*.json",
    # TypeScript tooling.
    "tsconfig*.json",
    "eslint.config.js",
    "knip.jsonc",
    # The Homebrew and AUR templates, which release.yml renders after the
    # builds.
    "packaging/**",
    # Repository checks and release helpers, run on Linux. gglib-cli's
    # setup_scripts test reads three of them when it runs, and only Linux's
    # `test` runs it.
    "scripts/**",
    # The other workflows, Dependabot and the issue forms.
    ".github/**",
    # Read by release-please alone.
    "release-please-config.json",
    ".release-please-manifest.json",
    # Editor settings.
    ".vscode/**",
    # Nothing includes it; each crate names its licence in its manifest.
    "LICENSE",
]


def matches(path: str, pattern: str) -> bool:
    if pattern.endswith("/**"):
        return path.startswith(pattern[:-2])
    return path.count("/") == pattern.count("/") and fnmatch.fnmatchcase(path, pattern)


def reaches_rust(path: str) -> bool:
    if any(matches(path, pattern) for pattern in REACHES):
        return True
    return not any(matches(path, pattern) for pattern in INERT)


def git(repo: str, *args: str) -> str:
    # The caller's own git configuration stays out: a global diff.renames,
    # diff.noprefix or signing setting would change what is read or written.
    env = {**os.environ, "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1"}
    done = subprocess.run(
        ["git", *args], cwd=repo, env=env, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    return done.stdout.decode("utf-8", "surrogateescape")


def changes(repo: str, base: str, head: str) -> list[tuple[str, str]]:
    """(status, path) per changed path. A rename is its deletion and its addition."""
    fields = git(repo, "diff", "--name-status", "--no-renames", "-z", base, head).split("\0")
    fields = [field for field in fields if field]
    return list(zip(fields[0::2], fields[1::2]))


def version(repo: str, ref: str) -> str | None:
    try:
        manifest = tomllib.loads(git(repo, "show", f"{ref}:Cargo.toml"))
        return manifest["workspace"]["package"]["version"]
    except (subprocess.CalledProcessError, tomllib.TOMLDecodeError, KeyError, TypeError):
        return None


def line_pairs(diff: str) -> list[tuple[list[str], list[str]]] | None:
    """Each hunk's removed and added lines, or None when the diff holds
    something else: a mode change, no hunk at all (a binary file), or a
    change to a final newline."""
    hunks: list[tuple[list[str], list[str]]] = []
    for line in diff.split("\n"):
        if line.startswith("@@"):
            hunks.append(([], []))
        elif not hunks:
            if line.startswith(("old mode ", "new mode ")):
                return None
        elif line.startswith("-"):
            hunks[-1][0].append(line[1:])
        elif line.startswith("+"):
            hunks[-1][1].append(line[1:])
        elif line.startswith("\\"):
            return None
    return hunks or None


def is_version_only(repo: str, base: str, head: str, changed: list[tuple[str, str]], say) -> bool:
    old, new = version(repo, base), version(repo, head)
    if old is None or new is None or old == new:
        say(f"version_only: Cargo.toml's version is {old} at the base and {new} at the head")
        return False
    for status, path in changed:
        if status != "M":
            what = {"A": "added", "D": "deleted"}.get(status, f"changed with status {status}")
            say(f"version_only: {path} was {what}")
            return False
    bump = re.compile(re.escape(old))
    for _, path in changed:
        if path == "CHANGELOG.md":
            continue
        diff = git(repo, "diff", "-U0", "--no-color", "--no-ext-diff", "--no-renames", base, head, "--", path)
        hunks = line_pairs(diff)
        if hunks is None:
            say(f"version_only: {path} changed in a way that is not a line of text")
            return False
        for removed, added in hunks:
            if len(removed) != len(added):
                say(f"version_only: a hunk in {path} removes {len(removed)} lines and adds {len(added)}")
                return False
            for before, after in zip(removed, added):
                if bump.sub(lambda _: new, before) != after:
                    say(f"version_only: {path} changes more than the version: {after.strip()!r}")
                    return False
    say(f"version_only: every changed line moves {old} to {new}")
    return True


def ffi_script(repo: str, base: str, head: str) -> bool:
    done = subprocess.run(
        [os.path.join(repo, FFI_SCRIPT), base, head], cwd=repo, check=True, stdout=subprocess.PIPE, text=True
    )
    last = done.stdout.strip().splitlines()[-1:] or [""]
    if last[0] not in ("changed=true", "changed=false"):
        raise RuntimeError(f"{FFI_SCRIPT} printed {done.stdout!r}, not changed=true or changed=false")
    return last[0] == "changed=true"


def decide(
    repo: str,
    base: str,
    head: str,
    event: str,
    pr_count: int | None,
    ffi_changed: Callable[[str, str, str], bool],
    say=lambda message: None,
) -> dict[str, bool]:
    if event not in ("pull_request", "push"):
        raise ValueError(f"--event is pull_request or push, not {event!r}")
    if event == "push" and (pr_count is None or pr_count < 0):
        raise ValueError("a push needs --pr-count, the number of pull requests behind the commit")
    changed = changes(repo, base, head)
    say(f"{len(changed)} paths changed between {base} and {head}")
    version_only = is_version_only(repo, base, head, changed, say)
    paths = [path for _, path in changed]
    reaching = [path for path in paths if reaches_rust(path)]
    rust = not version_only and bool(reaching)
    if not version_only:
        say(f"rust: {len(reaching)} of them reach a Rust build" + "".join(f"\n  {path}" for path in reaching[:20]))
    ffi = not version_only and ffi_changed(repo, base, head)
    lock = not version_only and any(path in ("Cargo.lock", "rust-toolchain.toml") for path in paths)
    if event == "pull_request":
        cross_os = rust
    else:
        cross_os = lock or (pr_count == 0 and rust)
        say(f"push: {pr_count} pull request(s) behind the commit")
    return {
        "version_only": version_only,
        "rust": rust,
        "ffi": ffi,
        "ios": rust,
        "lock": lock,
        "cross_os": cross_os,
        "apple": ffi,
    }


# ---------------------------------------------------------------------------
# Self-test


def tree(ver: str, foo: str = "1.2.3", foo_sum: str = "aaaa") -> dict[str, str]:
    """A small workspace at version `ver`. `bar` is a third-party crate that
    sits at 0.29.0, the version two cases release from."""
    return {
        "Cargo.toml": f'[workspace]\nmembers = ["crates/gglib-core"]\n\n[workspace.package]\nversion = "{ver}"\n\n'
        f'[workspace.dependencies]\ngglib-core = {{ path = "crates/gglib-core", version = "{ver}" }}\nbar = "0.29.0"\n',
        "Cargo.lock": "version = 4\n\n"
        '[[package]]\nname = "bar"\nversion = "0.29.0"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\nchecksum = "bbbb"\n\n'
        f'[[package]]\nname = "foo"\nversion = "{foo}"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\nchecksum = "{foo_sum}"\n\n'
        f'[[package]]\nname = "gglib-core"\nversion = "{ver}"\ndependencies = [\n "bar",\n "foo",\n]\n',
        "package.json": f'{{\n  "name": "gglib",\n  "version": "{ver}",\n  "private": true\n}}\n',
        "package-lock.json": f'{{\n  "name": "gglib",\n  "version": "{ver}",\n  "packages": {{\n    "": {{\n      "version": "{ver}"\n    }}\n  }}\n}}\n',
        ".release-please-manifest.json": f'{{\n  ".": "{ver}"\n}}\n',
        "CHANGELOG.md": "# Changelog\n",
        "rust-toolchain.toml": '[toolchain]\nchannel = "1.97.1"\n',
        "clippy.toml": "too-many-arguments-threshold = 8\n",
        "crates/gglib-core/src/lib.rs": "pub fn one() -> u8 {\n    1\n}\n",
        "crates/gglib-core/README.md": "# gglib-core\n",
        "src/main.tsx": "export const one = 1;\n",
        "docs/guide.md": "# Guide\n",
        "README.md": "# gglib\n",
        "scripts/llama_pin.sh": "#!/bin/sh\necho b1\n",
        "scripts/check.py": "print('ok')\n",
        ".github/workflows/ci.yml": "name: CI\n",
        ".github/workflows/coverage.yml": "name: Coverage\n",
    }


def edit(files: dict[str, str], changes: dict[str, str | None]) -> dict[str, str]:
    """`files` with each path in `changes` replaced, or removed when None."""
    out = {**files, **{path: body for path, body in changes.items() if body is not None}}
    return {path: body for path, body in out.items() if changes.get(path, "") is not None}


RC2, RC3 = tree("0.29.0-rc.2"), tree("0.29.0-rc.3")
CODE_CHANGE = "pub fn one() -> u8 {\n    2\n}\n"
RELEASE_RC3 = edit(RC3, {"CHANGELOG.md": "# Changelog\n\n## 0.29.0-rc.3\n\n* a fix\n"})

# (name, base, head, event, pr_count, what ffi-inputs-changed.sh answers or
#  None when it must not be asked, the outputs that are true)
CASES = [
    ("a release pull request", RC2, RELEASE_RC3, "pull_request", None, None, "version_only"),
    ("a promotion", RC3, tree("0.29.0"), "pull_request", None, None, "version_only"),
    # bar stays at 0.29.0, the version released from: an unchanged line is
    # not read.
    ("a first candidate", tree("0.29.0"), tree("0.30.0-rc.1"), "pull_request", None, None, "version_only"),
    # foo moves from 0.29.0 to 0.30.0-rc.1 in the same change. Its version
    # line passes the replacement, and its checksum line fails it: a
    # registry crate cannot move without one.
    (
        "a first candidate that moves a third-party crate across the same versions",
        tree("0.29.0", foo="0.29.0"),
        tree("0.30.0-rc.1", foo="0.30.0-rc.1", foo_sum="cccc"),
        "pull_request", None, True, "rust ffi ios lock cross_os apple",
    ),
    (
        "a release pull request with a third-party bump",
        RC2, tree("0.29.0-rc.3", foo="1.2.4", foo_sum="dddd"),
        "pull_request", None, True, "rust ffi ios lock cross_os apple",
    ),
    (
        "a release pull request that adds a file",
        RC2, edit(RC3, {"docs/notes.md": "# Notes\n"}),
        "pull_request", None, True, "rust ffi ios lock cross_os apple",
    ),
    (
        "a release pull request that inserts a line beside the version",
        RC2,
        edit(RC3, {"package.json": RC3["package.json"].replace('rc.3",\n', 'rc.3",\n  "type": "module",\n')}),
        "pull_request", None, True, "rust ffi ios lock cross_os apple",
    ),
    # CHANGELOG.md is the one file whose lines are not read, so only the
    # rule against an added file stops this.
    (
        "a release pull request that adds CHANGELOG.md",
        edit(RC2, {"CHANGELOG.md": None}),
        RELEASE_RC3,
        "pull_request", None, True, "rust ffi ios lock cross_os apple",
    ),
    (
        "a release pull request that changes another field of package.json",
        RC2, edit(RC3, {"package.json": RC3["package.json"].replace("true", "false")}),
        "pull_request", None, True, "rust ffi ios lock cross_os apple",
    ),
    ("the changelog alone, the version unmoved", RC2, edit(RC2, {"CHANGELOG.md": RELEASE_RC3["CHANGELOG.md"]}), "pull_request", None, False, ""),
    ("the frontend alone", RC2, edit(RC2, {"src/main.tsx": "export const one = 2;\n"}), "pull_request", None, False, ""),
    (
        "docs, scripts, another workflow and the root README",
        RC2,
        edit(
            RC2,
            {
                "docs/guide.md": "# Guide\n\nMore.\n",
                "scripts/check.py": "print('fine')\n",
                ".github/workflows/coverage.yml": "name: Coverage (Rust)\n",
                "README.md": "# gglib, the hub\n",
            },
        ),
        "pull_request", None, False, "",
    ),
    ("clippy.toml", RC2, edit(RC2, {"clippy.toml": "too-many-arguments-threshold = 9\n"}), "pull_request", None, False, "rust ios cross_os"),
    ("scripts/llama_pin.sh", RC2, edit(RC2, {"scripts/llama_pin.sh": "#!/bin/sh\necho b2\n"}), "pull_request", None, True, "rust ffi ios cross_os apple"),
    ("ci.yml", RC2, edit(RC2, {".github/workflows/ci.yml": "name: CI\non: push\n"}), "pull_request", None, True, "rust ffi ios cross_os apple"),
    ("a crate's README", RC2, edit(RC2, {"crates/gglib-core/README.md": "# gglib-core, the core\n"}), "pull_request", None, False, "rust ios cross_os"),
    (
        "a file moved out of a crate into docs",
        RC2,
        edit(RC2, {"crates/gglib-core/src/lib.rs": None, "docs/lib.rs": RC2["crates/gglib-core/src/lib.rs"]}),
        "pull_request", None, False, "rust ios cross_os",
    ),
    (
        "a file moved from docs into a crate",
        RC2,
        edit(RC2, {"docs/guide.md": None, "crates/gglib-core/guide.md": RC2["docs/guide.md"]}),
        "pull_request", None, False, "rust ios cross_os",
    ),
    ("rust-toolchain.toml", RC2, edit(RC2, {"rust-toolchain.toml": '[toolchain]\nchannel = "1.98.0"\n'}), "pull_request", None, True, "rust ffi ios lock cross_os apple"),
    ("a push of code a pull request tested", RC2, edit(RC2, {"crates/gglib-core/src/lib.rs": CODE_CHANGE}), "push", 1, False, "rust ios"),
    ("a push that moves the lockfile", RC2, tree("0.29.0-rc.2", foo="1.2.4", foo_sum="dddd"), "push", 1, True, "rust ffi ios lock cross_os apple"),
    ("a push of code with no pull request", RC2, edit(RC2, {"crates/gglib-core/src/lib.rs": CODE_CHANGE}), "push", 0, False, "rust ios cross_os"),
    ("a push of the frontend with no pull request", RC2, edit(RC2, {"src/main.tsx": "export const one = 2;\n"}), "push", 0, False, ""),
    ("a push of a release with no pull request", RC2, RELEASE_RC3, "push", 0, None, "version_only"),
]

# A release pull request whose package.json also became executable: a change
# with no line in it.
MODE_CASE = ("a release pull request that changes a file's mode", "rust ffi ios lock cross_os apple")

# (path, reaches a Rust build)
PATHS = [
    ("crates/gglib-core/src/lib.rs", True),
    ("crates/gglib-core/README.md", True),
    ("src-tauri/tauri.conf.json", True),
    ("contracts/upstream/llama/shape.txt", True),
    ("Cargo.toml", True),
    ("Makefile", True),
    ("clippy.toml", True),
    ("scripts/apple/build-xcframework.sh", True),
    ("scripts/llama_pin.sh", True),
    (".github/workflows/ci.yml", True),
    (".github/actions/pinned-engines/models.txt", True),
    ("docs/error-codes.json", False),
    ("CHANGELOG.md", False),
    ("src/App.tsx", False),
    ("src-tauri.md", False),
    ("tests/ts/app.test.ts", False),
    ("tests/rust.rs", True),
    ("package-lock.json", False),
    ("tsconfig.app.json", False),
    ("crates/x/tsconfig.json", True),
    ("packaging/homebrew/gglib.rb", False),
    ("scripts/crate_list.py", False),
    ("scripts/status_card/requirements.txt", False),
    (".github/workflows/coverage.yml", False),
    (".github/dependabot.yml", False),
    (".vscode/tasks.json", False),
    ("LICENSE", False),
]


def make_repo(root: str, name: str, base: dict[str, str], head: dict[str, str], chmod: str | None = None) -> str:
    repo = os.path.join(root, re.sub(r"\W+", "-", name))
    os.makedirs(repo)
    git(repo, "init", "-q", "-b", "main")
    for files in (base, head):
        for path in git(repo, "ls-files").split("\n"):
            if path and path not in files:
                os.remove(os.path.join(repo, path))
        for path, body in files.items():
            full = os.path.join(repo, path)
            os.makedirs(os.path.dirname(full), exist_ok=True)
            with open(full, "w", encoding="utf-8") as handle:
                handle.write(body)
        if chmod and files is head:
            os.chmod(os.path.join(repo, chmod), 0o755)
        git(repo, "add", "-A")
        git(repo, "-c", "user.name=fixture", "-c", "user.email=fixture@example.invalid", "commit", "-q", "-m", "x")
    return repo


def self_test() -> bool:
    failures: list[str] = []
    for path, expected in PATHS:
        if reaches_rust(path) != expected:
            failures.append(f"{path}: reaches_rust is {not expected}, planted {expected}")
    cases = [(n, b, h, e, p, f, x, None) for n, b, h, e, p, f, x in CASES]
    cases.append((MODE_CASE[0], RC2, RC3, "pull_request", None, True, MODE_CASE[1], "package.json"))
    with tempfile.TemporaryDirectory() as root:
        for name, base, head, event, pr_count, ffi_says, expected, chmod in cases:
            repo = make_repo(root, name, base, head, chmod)

            def stand_in(_repo: str, _base: str, _head: str, answer=ffi_says, case=name) -> bool:
                if answer is None:
                    raise AssertionError(f"{case}: asked {FFI_SCRIPT}, which a version-only change must not run")
                return answer

            try:
                got = decide(repo, "HEAD^1", "HEAD", event, pr_count, stand_in)
            except AssertionError as error:
                failures.append(str(error))
                continue
            want = {name_: name_ in expected.split() for name_ in OUTPUTS}
            if got != want:
                wrong = ", ".join(f"{k}={str(got[k]).lower()}" for k in OUTPUTS if got[k] != want[k])
                failures.append(f"{name}: {wrong}")
        repo = make_repo(root, "refused", RC2, RC3)
        for event, pr_count in (("push", None), ("merge_group", None)):
            try:
                decide(repo, "HEAD^1", "HEAD", event, pr_count, lambda *_: False)
                failures.append(f"--event {event} with --pr-count {pr_count} was accepted")
            except ValueError:
                pass
    total = len(PATHS) + len(cases) + 2
    if not failures:
        print(f"✅ self-test: {total} planted cases, each as planted", file=sys.stderr)
        return True
    print("❌ self-test failed: the rules no longer give what the fixtures plant", file=sys.stderr)
    for failure in failures:
        print(f"     {failure}", file=sys.stderr)
    return False


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--self-test", action="store_true", help="run the fixtures only")
    commands = parser.add_subparsers(dest="command")
    scope = commands.add_parser("scope", help="self-test, then print the outputs for two commits")
    scope.add_argument("--base", required=True)
    scope.add_argument("--head", required=True)
    scope.add_argument("--event", default="pull_request", choices=("pull_request", "push"))
    scope.add_argument("--pr-count", type=int)
    args = parser.parse_args()
    if not args.self_test and args.command != "scope":
        parser.error("give --self-test or scope")
    if not self_test():
        return 1
    if args.self_test:
        return 0
    say = lambda message: print(message, file=sys.stderr)  # noqa: E731
    try:
        outputs = decide(REPO, args.base, args.head, args.event, args.pr_count, ffi_script, say)
    except (ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        detail = getattr(error, "stderr", None)
        print(f"❌ {error}" + (f"\n{detail.decode(errors='replace')}" if detail else ""), file=sys.stderr)
        return 1
    for name in OUTPUTS:
        line = f"{name}={str(outputs[name]).lower()}"
        print(line)
        print(f"  {line}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
