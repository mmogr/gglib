#!/usr/bin/env python3
"""Turn the checked-out release candidate X.Y.Z-rc.N into the stable X.Y.Z.

    scripts/promote.py --previous vA.B.C   # promote the working tree
    scripts/promote.py --self-test         # run the fixtures only

Requires the version in Cargo.toml's `[workspace.package]` to be a release
candidate, X.Y.Z-rc.N. Then:

* In Cargo.toml, package.json and .release-please-manifest.json, the quoted
  "X.Y.Z-rc.N" becomes "X.Y.Z". It must appear exactly once in each, so one
  line changes per file. This is the substitution check_rc_promotion.sh
  undoes when it compares the tree with the candidate's tag. The files are
  edited as text, so nothing else in them moves: a JSON round trip would
  re-escape any non-ASCII character in package.json.
* In CHANGELOG.md, a heading for X.Y.Z goes above the first `## ` heading,
  the candidate's, with a line saying the release is that candidate
  unchanged. The heading has the form release-please writes, so the next
  cycle's entry lands above it. A missing CHANGELOG.md is created.

`--previous` is the last stable tag, which the heading's compare link starts
from. Nothing is written unless every file can be changed. On success it
prints `rc=X.Y.Z-rc.N` and `stable=X.Y.Z` and nothing else on stdout, for
$GITHUB_OUTPUT; messages go to stderr.

The lockfiles are not touched here: promote.yml re-locks them afterwards.

Exit codes:
    0  promoted, or the self-test passed
    1  refused (the reason is printed), or the self-test failed
"""

from __future__ import annotations

import argparse
import datetime
import os
import re
import sys
import tempfile
from typing import Dict, List, Optional, Tuple

REPO = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
COMPARE = "https://github.com/mmogr/gglib/compare"
VERSIONED = ["Cargo.toml", "package.json", ".release-please-manifest.json"]
CHANGELOG = "CHANGELOG.md"
CANDIDATE = re.compile(r"^(\d+\.\d+\.\d+)-rc\.\d+$")
STABLE_TAG = re.compile(r"^v\d+\.\d+\.\d+$")


class Refusal(Exception):
    pass


def read(root: str, path: str) -> Optional[str]:
    full = os.path.join(root, path)
    if not os.path.exists(full):
        return None
    with open(full, encoding="utf-8", newline="") as handle:
        return handle.read()


def write(root: str, path: str, text: str) -> None:
    with open(os.path.join(root, path), "w", encoding="utf-8", newline="") as handle:
        handle.write(text)


def workspace_version(cargo: str) -> str:
    section = None
    for line in cargo.splitlines():
        header = re.match(r"^\s*\[([^\]]+)\]\s*$", line)
        if header:
            section = header.group(1).strip()
            continue
        if section == "workspace.package":
            found = re.match(r'^version\s*=\s*"([^"]*)"\s*$', line)
            if found:
                return found.group(1)
    raise Refusal("Cargo.toml has no version in [workspace.package]")


def changelog_entry(rc: str, stable: str, previous: str, today: str) -> str:
    return (
        f"## [{stable}]({COMPARE}/{previous}...v{stable}) ({today})\n"
        "\n"
        f"{rc}, promoted unchanged. The candidates below make up this release.\n"
        "\n"
    )


def with_entry(changelog: Optional[str], entry: str) -> str:
    if changelog is None:
        return f"# Changelog\n\n{entry}"
    first = re.search(r"^## ", changelog, re.MULTILINE)
    if first:
        return changelog[: first.start()] + entry + changelog[first.start() :]
    return changelog.rstrip("\n") + "\n\n" + entry


def promote(root: str, previous: str, today: str) -> Tuple[str, str]:
    if not STABLE_TAG.match(previous):
        raise Refusal(f"--previous must be the last stable tag, vX.Y.Z; got '{previous}'")
    texts: Dict[str, str] = {}
    for path in VERSIONED:
        text = read(root, path)
        if text is None:
            raise Refusal(f"{path} does not exist")
        texts[path] = text
    rc = workspace_version(texts["Cargo.toml"])
    candidate = CANDIDATE.match(rc)
    if not candidate:
        raise Refusal(f"the version is {rc}, not a release candidate X.Y.Z-rc.N; there is nothing to promote")
    stable = candidate.group(1)

    changed: Dict[str, str] = {}
    for path, text in texts.items():
        count = text.count(f'"{rc}"')
        if count != 1:
            raise Refusal(f'"{rc}" appears {count} times in {path}; a promotion moves exactly one line per file')
        changed[path] = text.replace(f'"{rc}"', f'"{stable}"')
    changed[CHANGELOG] = with_entry(read(root, CHANGELOG), changelog_entry(rc, stable, previous, today))

    for path, text in changed.items():
        write(root, path, text)
    return rc, stable


# --- self-test -------------------------------------------------------------

CARGO = """\
[workspace]
members = ["crates/a"]

[workspace.package]
version = "0.29.0-rc.3"
edition = "2024"

[workspace.dependencies]
serde = { version = "1.0" }
"""

PACKAGE = """\
{
  "name": "gglib-gui",
  "version": "0.29.0-rc.3",
  "description": "Dashboard — the GGLib control surface",
  "dependencies": {
    "react": "^19.0.0"
  }
}
"""

MANIFEST = '{\n  ".": "0.29.0-rc.3"\n}\n'

LOG = """\
# Changelog

## [0.29.0-rc.3](https://github.com/mmogr/gglib/compare/v0.29.0-rc.2...v0.29.0-rc.3) (2026-11-02)


### What gglib now does

* **chat:** something new
"""

ENTRY = (
    "## [0.29.0](https://github.com/mmogr/gglib/compare/v0.28.0...v0.29.0) (2026-11-03)\n"
    "\n"
    "0.29.0-rc.3, promoted unchanged. The candidates below make up this release.\n"
    "\n"
)


def plant(root: str, files: Dict[str, str]) -> None:
    for path, text in files.items():
        write(root, path, text)


def snapshot(root: str) -> Dict[str, Optional[str]]:
    return {path: read(root, path) for path in VERSIONED + [CHANGELOG]}


def changed_lines(before: str, after: str) -> List[Tuple[str, str]]:
    old, new = before.splitlines(), after.splitlines()
    if len(old) != len(new):
        return [("line count", f"{len(old)} -> {len(new)}")]
    return [(a, b) for a, b in zip(old, new) if a != b]


def self_test() -> bool:
    failures: List[str] = []
    base = {"Cargo.toml": CARGO, "package.json": PACKAGE, ".release-please-manifest.json": MANIFEST}

    def check(name: str, ok: bool, detail: str = "") -> None:
        if not ok:
            failures.append(f"{name}{': ' + detail if detail else ''}")

    # 1. A candidate is promoted: one line per versioned file, the heading
    #    above the candidate's.
    with tempfile.TemporaryDirectory() as root:
        plant(root, {**base, CHANGELOG: LOG})
        result = promote(root, "v0.28.0", "2026-11-03")
        check("promote returns rc and stable", result == ("0.29.0-rc.3", "0.29.0"), str(result))
        after = snapshot(root)
        expected_lines = {
            "Cargo.toml": [('version = "0.29.0-rc.3"', 'version = "0.29.0"')],
            "package.json": [('  "version": "0.29.0-rc.3",', '  "version": "0.29.0",')],
            ".release-please-manifest.json": [('  ".": "0.29.0-rc.3"', '  ".": "0.29.0"')],
        }
        for path, lines in expected_lines.items():
            got = changed_lines(base[path], after[path] or "")
            check(f"{path} changes only its version line", got == lines, repr(got))
        check("the manifest reads 0.29.0", after[".release-please-manifest.json"] == '{\n  ".": "0.29.0"\n}\n')
        check("the em dash in package.json survives", "Dashboard — the GGLib" in (after["package.json"] or ""))
        expected_log = LOG.replace("## [0.29.0-rc.3]", ENTRY + "## [0.29.0-rc.3]", 1)
        check("the heading goes above the candidate's", after[CHANGELOG] == expected_log, repr(after[CHANGELOG]))

    # 2. With no CHANGELOG.md, one is created.
    with tempfile.TemporaryDirectory() as root:
        plant(root, base)
        promote(root, "v0.28.0", "2026-11-03")
        got = read(root, CHANGELOG)
        check("a missing CHANGELOG.md is created", got == "# Changelog\n\n" + ENTRY, repr(got))

    # 3. Refusals write nothing.
    refusals = {
        "a stable version": ({k: v.replace("0.29.0-rc.3", "0.29.0") for k, v in base.items()}, "v0.28.0"),
        "a prerelease that is not an rc": ({**base, "Cargo.toml": CARGO.replace("rc.3", "beta.1")}, "v0.28.0"),
        "the version twice in a file": (
            {**base, "package.json": PACKAGE.replace('"^19.0.0"', '"0.29.0-rc.3"')},
            "v0.28.0",
        ),
        "the version missing from a file": ({**base, ".release-please-manifest.json": '{\n  ".": "0.29.0-rc.2"\n}\n'}, "v0.28.0"),
        "a previous tag that is not stable": (base, "v0.29.0-rc.2"),
        "a previous version without its v": (base, "0.28.0"),
    }
    for name, (files, previous) in refusals.items():
        with tempfile.TemporaryDirectory() as root:
            plant(root, {**files, CHANGELOG: LOG})
            before = snapshot(root)
            code = main(["--previous", previous], root=root, today="2026-11-03", quiet=True)
            check(f"{name} exits 1", code == 1, f"exit {code}")
            check(f"{name} writes nothing", snapshot(root) == before)

    if not failures:
        print("✓ promote.py self-test: promotion, file creation and 6 refusals behave", file=sys.stderr)
        return True
    print("✗ promote.py self-test failed:", file=sys.stderr)
    for failure in failures:
        print(f"    {failure}", file=sys.stderr)
    return False


def main(argv: Optional[List[str]] = None, root: str = REPO, today: Optional[str] = None, quiet: bool = False) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--previous", help="the last stable tag, vX.Y.Z")
    mode.add_argument("--self-test", action="store_true", help="run the fixtures only")
    args = parser.parse_args(argv)
    if args.self_test:
        return 0 if self_test() else 1
    today = today or datetime.datetime.now(datetime.timezone.utc).date().isoformat()
    try:
        rc, stable = promote(root, args.previous, today)
    except Refusal as refusal:
        if not quiet:
            print(f"::error::{refusal}", file=sys.stderr)
        return 1
    if not quiet:
        print(f"rc={rc}")
        print(f"stable={stable}")
        print(f"✓ {rc} promoted to {stable}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
