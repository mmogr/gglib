#!/usr/bin/env python3
"""Split one `cargo test` run's output into the per-crate files badges.yml reads.

`ci.yml` used to run the workspace suite once for the aggregate badge and then
again, fifteen times over, one `cargo test -p <crate>` per crate, purely so
`badges.yml` would have a `rust-test-<crate>.txt` to count. That loop cost
23m15s of a 57m job, and it did not merely repeat work: naming a crate with
`-p` changes feature unification, so `cargo test -p gglib-runtime` ran 329
tests where the workspace build runs 352 — the 23 in `llama::{build,config,
deps,download,update}` are behind features `gglib-cli` turns on. The loop
tested a configuration no real build uses, and the badge was 23 short.

The whole-workspace run already contains everything the loop produced; it is
just not divided by crate. This divides it.

The map from binary to package cannot come from the text. Unit-test binaries
carry their crate name but integration-test binaries do not: `integration_guards`
belongs to `gglib-agent` and says so nowhere, and `gglib-cli`'s `[[bin]] name =
"gglib"` builds a binary called `gglib-<hash>` that matches no package at all.
So the map comes from `cargo test --no-run --message-format=json`, which names
the package for every artifact it builds. That stream is emitted on a fully
warm build too, with `"fresh": true` and `executable` still populated, so the
map does not evaporate when nothing recompiles.

Lines are copied out **verbatim**, ANSI escapes and all. `badges.yml` parses
two different things out of these files — `test result: ok. N passed` for the
per-crate badges and `^test <module>::<name> ... ok` for some fifty per-module
ones — and the cheapest guarantee that both keep working is that the bytes are
the ones cargo wrote.

What this does NOT do, so nobody reads more into a pass than is there:

* It does not check that the tests passed. A file for a crate whose suite
  failed is written the same way, carrying its `test result: FAILED.` line.
  The exit status of `cargo test` is what gates the job.
* It does not verify per-crate coverage against any list. A crate with no test
  targets gets no file, exactly as it got none from the loop.

Usage:
    ./scripts/split_test_output.py <artifacts.json> <test-output.txt>
    ./scripts/split_test_output.py <artifacts.json> <test-output.txt> --out DIR

Exit codes:
    0  every section was attributed, or the run produced no sections to split
    1  a section mapped to no package, or the doctests went missing
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys

ANSI = re.compile(r"\x1b\[[0-9;]*m")

# Cargo prints the binary two ways. Plain `cargo test` names the source and
# parenthesises the path; `--verbose` backticks an absolute path. CI runs the
# plain form, but the parity fixture is a `--verbose` capture from before this
# change, so both have to read.
#
# Both patterns end at the `<name>-<hash>` and then the line ends. That
# anchoring is what keeps `--verbose`'s thousands of rustc invocations out:
# they carry `/deps/` too, in `--extern foo=…/deps/libfoo-<hash>.rmeta`, but
# they do not end there.
BINARY = r"[A-Za-z0-9_.\-]+-[0-9a-f]{8,}"
RUNNING_PLAIN = re.compile(rf"^\s+Running\s+[^(]*\((?P<path>[^)]*[/\\]deps[/\\]{BINARY})\)\s*$")
RUNNING_VERBOSE = re.compile(rf"^\s+Running\s+`(?P<path>[^`]*[/\\]deps[/\\]{BINARY})`\s*$")

# Doctests are attributed by lib name, not by binary — there is no executable.
DOC_TESTS = re.compile(r"^\s+Doc-tests\s+(?P<lib>[A-Za-z0-9_]+)\s*$")

# A build-status line ends whatever section was open. Under `--verbose` the
# rustdoc invocation inside a Doc-tests section is not one of these, so the
# section survives it.
BUILD_STATUS = re.compile(
    r"^\s+(Compiling|Fresh|Finished|Checking|Building|Downloading|Downloaded|Updating|Blocking|Installing)\b"
)


def package_name(package_id: str) -> str:
    """The package name out of a cargo PackageId.

    Cargo spells the same thing two ways, and both are in this workspace:
    `…/crates/gglib-core#0.15.3` when the directory matches the package name,
    and `…/src-tauri#gglib-app@0.15.3` when it does not.
    """
    path, _, fragment = package_id.rpartition("#")
    if "@" in fragment:
        return fragment.rpartition("@")[0]
    return path.rstrip("/").rpartition("/")[2]


def build_maps(artifacts_path: str) -> tuple[dict[str, str], dict[str, str]]:
    """Binary basename → package, and lib name → package, from the JSON stream."""
    by_binary: dict[str, str] = {}
    by_lib: dict[str, str] = {}

    with open(artifacts_path, encoding="utf-8", errors="replace") as stream:
        for line in stream:
            line = line.strip()
            if not line.startswith("{"):
                continue
            try:
                message = json.loads(line)
            except ValueError:
                continue
            if message.get("reason") != "compiler-artifact":
                continue

            target = message.get("target") or {}
            kinds = target.get("kind") or []
            # Build scripts share the basename `build-script-build` across every
            # package that has one. Nothing ever runs them as a test, but they
            # would collide in the map, so they do not go in it.
            if "custom-build" in kinds:
                continue

            package = package_name(message.get("package_id", ""))
            if not package:
                continue

            executable = message.get("executable")
            if executable:
                by_binary[os.path.basename(executable)] = package
            if any(kind in ("lib", "rlib", "proc-macro") for kind in kinds):
                by_lib[target.get("name", "")] = package

    by_lib.pop("", None)
    return by_binary, by_lib


def split(output_path: str, by_binary: dict[str, str], by_lib: dict[str, str]):
    """Group the run's lines by package, and report what could not be grouped."""
    sections: dict[str, list[str]] = {}
    unmapped: list[str] = []
    doc_sections = 0
    test_sections = 0
    current: str | None = None

    with open(output_path, encoding="utf-8", errors="replace") as stream:
        for raw in stream:
            raw = raw.rstrip("\n")
            plain = ANSI.sub("", raw)

            running = RUNNING_PLAIN.match(plain) or RUNNING_VERBOSE.match(plain)
            doc = DOC_TESTS.match(plain)

            if running:
                key = os.path.basename(running.group("path"))
                current = by_binary.get(key)
                test_sections += 1
                if current is None:
                    unmapped.append(key)
                    continue
            elif doc:
                lib = doc.group("lib")
                current = by_lib.get(lib)
                doc_sections += 1
                if current is None:
                    unmapped.append(f"Doc-tests {lib}")
                    continue
            elif BUILD_STATUS.match(plain):
                current = None
                continue

            if current is not None:
                sections.setdefault(current, []).append(raw)

    return sections, unmapped, test_sections, doc_sections


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("artifacts", help="output of `cargo test --no-run --message-format=json`")
    parser.add_argument("output", help="output of the `cargo test` run itself")
    parser.add_argument("--out", default=".", help="directory to write rust-test-<crate>.txt into")
    args = parser.parse_args()

    if not os.path.exists(args.output):
        print(f"ℹ {args.output} does not exist — the build failed before any test ran, nothing to split")
        return 0

    by_binary, by_lib = build_maps(args.artifacts)
    if not by_binary:
        print(f"❌ {args.artifacts} named no test binaries.")
        print("   `cargo test --no-run --message-format=json` should emit a compiler-artifact")
        print("   message with a populated `executable` for every one, warm build or cold.")
        return 1

    sections, unmapped, test_sections, doc_sections = split(args.output, by_binary, by_lib)

    if not sections and not unmapped:
        print(f"ℹ {args.output} contains no test sections — the build failed before any test ran")
        return 0

    if unmapped:
        print("❌ these sections could not be attributed to a package:")
        for name in sorted(set(unmapped)):
            print(f"     {name}")
        print("\n   Every test binary should appear in the --message-format=json stream.")
        print("   If one does not, the two cargo invocations resolved different builds.")
        return 1

    # The doctests are only run by the plain `cargo test` above — the separate
    # `cargo test --doc` step was deleted because it re-ran these very sections
    # for an identical result. If they ever stop appearing, that deletion has
    # silently cost coverage, and this is the thing that says so.
    #
    # Only checked when the run got far enough to produce test sections at all,
    # and only when nothing failed: cargo runs the doctests last, so a red run
    # without --no-fail-fast legitimately stops before reaching them.
    failed = any(
        line.startswith("test result: FAILED.")
        for lines in sections.values()
        for line in lines
    )
    if test_sections and not doc_sections and not failed:
        print("❌ the run produced no `Doc-tests` sections.")
        print("   Plain `cargo test` runs doctests, which is why ci.yml has no separate")
        print("   `cargo test --doc` step. Adding `--all-targets` to the run would drop")
        print("   them silently; if that is what happened, restore the --doc step.")
        return 1

    os.makedirs(args.out, exist_ok=True)
    for package in sorted(sections):
        path = os.path.join(args.out, f"rust-test-{package}.txt")
        with open(path, "w", encoding="utf-8") as handle:
            handle.write("\n".join(sections[package]) + "\n")

    print(f"✅ split {test_sections} test binaries and {doc_sections} doctest sets across {len(sections)} crates")
    for package in sorted(sections):
        passed = sum(
            int(match.group(1))
            for line in sections[package]
            for match in [re.search(r"test result: (?:ok|FAILED)\. (\d+) passed", line)]
            if match
        )
        print(f"     {package:28s} {passed:5d} passed")
    if failed:
        print("\n⚠ at least one binary reported failures; cargo's exit status is what gates the job")
    return 0


if __name__ == "__main__":
    sys.exit(main())
