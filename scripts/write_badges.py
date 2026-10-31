#!/usr/bin/env python3
"""Write the badge files on the `badges` branch from one CI or Coverage run of main.

    write_badges.py write --event ci|coverage --artifacts DIR --branch DIR [--scc PATH]
    write_badges.py --self-test

`.github/workflows/badges.yml` checks out the commit the run tested, the
`badges` branch into DIR, and the run's artifact into another DIR, and runs
this. The files the branch should hold come from crates/layers.toml, through
`crate_list.py`: `<crate>-<metric>.json` for each badged crate, and
`version.json`. Each run computes what its own artifact or the checkout can
tell it, and leaves the rest:

| File                 | `--event ci`                       | `--event coverage`                 |
|----------------------|------------------------------------|------------------------------------|
| `<crate>-tests`      | from `rust-test-<crate>.txt`       | kept                               |
| `<crate>-coverage`   | kept                               | from `<crate>-lcov.info`           |
| `<crate>-loc`        | scc over the crate's directory     | kept                               |
| `<crate>-complexity` | scc over the crate's directory     | kept                               |
| `version.json`       | `[workspace.package]` in Cargo.toml | kept                              |

A file the run computes reads `none` when the artifact holds no file for that
crate. A file the run keeps and the branch does not have yet reads `pending`.
Then the branch is reconciled: every `.json` file outside that set is deleted,
in any directory but `.git`, and every other file (`.nojekyll`, the status
card's SVGs) is left alone. Last, the branch's README.md is rewritten to say
what each file is and which run writes it.

The tests count is every `test result:` line, `ok.` and `FAILED.` alike: the
passed figure over passed plus failed. Coverage is the lines hit over the lines
found, summed over the lcov records, rounded down.

What it does NOT do, so that nobody reads more into a pass than is there:

* It does not decide whether the run deserved badges. badges.yml runs it only
  after a successful run of a push, so a red run leaves the badges as they were.
* It does not commit or push. badges.yml publishes the directory.
* It does not check the counts against anything. A crate whose tests stopped
  running reads `none`; one whose suite shrank reads the smaller number.

Exit codes:
    0  the branch directory holds the expected files
    1  the artifact held no file for any badged crate (nothing is changed), scc
       failed, layers.toml and the workspace disagree, or the self-test failed
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import textwrap
from typing import Callable, Dict, List, Optional, Tuple

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import crate_list  # noqa: E402  (beside this file, not an installed module)

ANSI = re.compile(r"\x1b\[[0-9;]*m")
TEST_RESULT = re.compile(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed")

Badge = Dict[str, object]
Measure = Callable[[str], Tuple[int, int]]  # crate directory -> (lines of code, complexity)


class Fault(Exception):
    """A run that cannot write badges; nothing has been changed."""


def badge(label: str, message: str, color: str) -> Badge:
    return {"schemaVersion": 1, "label": label, "message": message, "color": color}


def label(crate: str, metric: str) -> str:
    return {"tests": crate, "coverage": f"{crate} cov"}.get(metric, metric)


def tests_badge(crate: str, text: str) -> Badge:
    passed = failed = 0
    for match in TEST_RESULT.finditer(ANSI.sub("", text)):
        passed += int(match.group(1))
        failed += int(match.group(2))
    total = passed + failed
    if not total:
        return badge(crate, "0/0", "yellow")
    percent = passed * 100 // total
    color = ("brightgreen" if percent == 100 else "green" if percent > 90 else
             "yellowgreen" if percent > 80 else "yellow" if percent > 60 else "red")
    return badge(crate, f"{passed}/{total}", color)


def coverage_badge(crate: str, text: str) -> Badge:
    found = hit = 0
    for line in text.splitlines():
        if line.startswith("LF:"):
            found += int(line[3:])
        elif line.startswith("LH:"):
            hit += int(line[3:])
    if not found:
        return badge(f"{crate} cov", "0%", "red")
    percent = hit * 100 // found
    color = ("brightgreen" if percent > 90 else "green" if percent > 80 else
             "yellowgreen" if percent > 70 else "yellow" if percent > 60 else "red")
    return badge(f"{crate} cov", f"{percent}%", color)


def loc_badge(lines: int) -> Badge:
    color = ("brightgreen" if lines < 200 else "green" if lines < 500 else
             "yellowgreen" if lines < 1000 else "yellow" if lines < 2000 else "orange")
    return badge("loc", str(lines), color)


def complexity_badge(complexity: int) -> Badge:
    color = ("brightgreen" if complexity < 10 else "green" if complexity < 25 else
             "yellowgreen" if complexity < 50 else "yellow" if complexity < 100 else "orange")
    return badge("complexity", str(complexity), color)


def parse_scc(output: str) -> Tuple[int, int]:
    """Rust's lines of code and complexity out of `scc --format json`."""
    rust = [entry for entry in json.loads(output) if entry.get("Name") == "Rust"]
    return sum(e.get("Code", 0) for e in rust), sum(e.get("Complexity", 0) for e in rust)


def scc_measure(scc: str) -> Measure:
    def measure(directory: str) -> Tuple[int, int]:
        try:
            out = subprocess.run([scc, directory, "--format", "json"], check=True,
                                 capture_output=True, text=True).stdout
        except (OSError, subprocess.CalledProcessError) as error:
            raise Fault(f"scc could not count {directory}: {error}") from error
        return parse_scc(out)
    return measure


def read(path: str) -> Optional[str]:
    try:
        with open(path, encoding="utf-8", errors="replace") as handle:
            return handle.read()
    except FileNotFoundError:
        return None


def computed(event: str, workspace: crate_list.Workspace, artifacts: str, measure: Measure) -> Dict[str, Badge]:
    """The files this run writes, by name. Raises Fault before anything is written."""
    out: Dict[str, Badge] = {}
    if event == "ci":
        crates, metric, source = workspace.named("badged"), "tests", "rust-test-{}.txt"
    else:
        crates, metric, source = workspace.named("coverage"), "coverage", "{}-lcov.info"
    found = 0
    for crate in crates:
        text = read(os.path.join(artifacts, source.format(crate.name)))
        if text is None:
            out[f"{crate.name}-{metric}.json"] = badge(label(crate.name, metric), "none", "lightgrey")
            continue
        found += 1
        make = tests_badge if metric == "tests" else coverage_badge
        out[f"{crate.name}-{metric}.json"] = make(crate.name, text)
    if not found:
        raise Fault(f"{artifacts} holds no {source.format('<crate>')} for any of the {len(crates)} crates; "
                    "the artifact is missing or its files were renamed")
    if event == "ci":
        for crate in workspace.named("badged"):
            lines, complexity = measure(os.path.join(workspace.root, crate.path))
            out[f"{crate.name}-loc.json"] = loc_badge(lines)
            out[f"{crate.name}-complexity.json"] = complexity_badge(complexity)
        manifest = crate_list.read_toml(os.path.join(workspace.root, "Cargo.toml"))
        version = manifest["workspace"]["package"]["version"]
        out[crate_list.VERSION_FILE] = badge("version", f"v{version}", "blue")
    return out


def branch_readme(workspace: crate_list.Workspace) -> str:
    badged = workspace.named("badged")
    unmeasured = [c.name for c in badged if not c.coverage]
    lines = [
        "# badges",
        "",
        "The files the shields.io badges in gglib's READMEs read. Nothing here is",
        "written by hand: `.github/workflows/badges.yml` runs `scripts/write_badges.py`",
        "after each successful CI or Coverage run of a push to `main`, which rewrites",
        "this README and deletes any JSON file not listed below.",
        "",
        "| File | What it is | Written after |",
        "|---|---|---|",
        "| `<crate>-tests.json` | Tests passed of tests run, from the crate's `rust-test-<crate>.txt` | CI |",
        "| `<crate>-coverage.json` | Lines covered, from the crate's `<crate>-lcov.info` | Coverage |",
        "| `<crate>-loc.json`, `<crate>-complexity.json` | Lines of Rust and their complexity, counted by scc | CI |",
        "| `version.json` | The workspace version in `Cargo.toml` | CI |",
        "| `status-light.svg`, `status-dark.svg` | The README's status card | `status-card.yml`, daily |",
        "",
        f"`<crate>` is each crate with badges in `crates/layers.toml`: {', '.join(c.name for c in badged)}.",
    ]
    if unmeasured:
        lines.append(f"These have no coverage file: {', '.join(unmeasured)}.")
    lines += [
        "",
        "A file reads `none` when the run's artifact held nothing for its crate, and",
        "`pending` until the first run that writes it.",
        "",
    ]
    return "\n".join(lines)


def write(event: str, workspace: crate_list.Workspace, artifacts: str, branch: str, measure: Measure) -> Dict[str, List[str]]:
    """Reconcile the branch directory. Returns what happened to each file, by outcome."""
    new = computed(event, workspace, artifacts, measure)
    expected = {f for f in crate_list.expected_branch_files(workspace) if f.endswith(".json")}
    report: Dict[str, List[str]] = {"written": [], "kept": [], "pending": [], "deleted": []}

    for directory, subdirs, files in os.walk(branch):
        subdirs[:] = [d for d in subdirs if d != ".git"]
        for name in files:
            relative = os.path.relpath(os.path.join(directory, name), branch).replace(os.sep, "/")
            if name.endswith(".json") and relative not in expected:
                os.remove(os.path.join(directory, name))
                report["deleted"].append(relative)

    for name in sorted(expected):
        path = os.path.join(branch, name)
        if name in new:
            content, outcome = new[name], "written"
        elif os.path.exists(path):
            report["kept"].append(name)
            continue
        else:
            crate, metric = name[:-len(".json")].rsplit("-", 1)
            content, outcome = badge(label(crate, metric), "pending", "lightgrey"), "pending"
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(json.dumps(content, separators=(",", ":"), ensure_ascii=False) + "\n")
        report[outcome].append(name)

    with open(os.path.join(branch, "README.md"), "w", encoding="utf-8") as handle:
        handle.write(branch_readme(workspace))
    return report


# ---------------------------------------------------------------------------
# Self-test

FIXTURE_REPO = {
    "Cargo.toml": """\
        [workspace]
        members = ["crates/a", "crates/b", "crates/c", "app"]
        [workspace.package]
        version = "1.2.3-rc.4"
        """,
    "crates/layers.toml": """\
        order = ["adapter", "core"]
        [crate.gglib-a]
        layer = "core"
        [crate.gglib-b]
        layer = "core"
        [crate.gglib-c]
        layer = "core"
        coverage = false
        why = "Fixture."
        [crate.gglib-app]
        layer = "adapter"
        badges = false
        why = "Fixture."
        """,
    "crates/a/Cargo.toml": '[package]\nname = "gglib-a"\n',
    "crates/b/Cargo.toml": '[package]\nname = "gglib-b"\n',
    "crates/c/Cargo.toml": '[package]\nname = "gglib-c"\n',
    "app/Cargo.toml": '[package]\nname = "gglib-app"\n',
}

# gglib-a: an ok section, a FAILED one, ANSI escapes: 8 of 10, 80%, yellow.
# gglib-b and gglib-c have no file and read `none`. gglib-app's is not read.
FIXTURE_CI = {
    "rust-test-gglib-a.txt": "\x1b[1mtest result: \x1b[32mok\x1b[0m. 5 passed; 0 failed; 0 ignored\n"
                             "test result: FAILED. 3 passed; 2 failed; 1 ignored\n",
    "rust-test-gglib-app.txt": "test result: ok. 9 passed; 0 failed\n",
    "rust-test-output.txt": "test result: ok. 99 passed; 0 failed\n",
}

# 272 of 300 lines over two records, 90.7%: rounded down to 90, green rather
# than brightgreen. gglib-b reads `none`.
FIXTURE_COVERAGE = {
    "gglib-a-lcov.info": "SF:a.rs\nLF:200\nLH:190\nend_of_record\nSF:b.rs\nLF:100\nLH:82\nend_of_record\n",
    "gglib-c-lcov.info": "SF:c.rs\nLF:10\nLH:10\nend_of_record\n",
    "lcov.info": "LF:1\nLH:1\n",
}

KEPT_COVERAGE = '{"schemaVersion":1,"label":"gglib-a cov","message":"50%","color":"red"}\n'
FIXTURE_BRANCH = {
    ".nojekyll": "",
    "README.md": "old\n",
    "status-light.svg": "<svg/>",
    "gglib-a-coverage.json": KEPT_COVERAGE,
    "ts-tests.json": "{}",            # an orphan
    "gglib-app-tests.json": "{}",     # unbadged crate
    "gglib-c-coverage.json": "{}",    # coverage = false
    "old/gglib-a-tests.json": "{}",   # an orphan in a directory
    ".git/config.json": "{}",         # never read
}

SCC_OUTPUT = json.dumps([
    {"Name": "Rust", "Code": 1234, "Complexity": 56},
    {"Name": "TOML", "Code": 20, "Complexity": 0},
])


def fake_measure(directory: str) -> Tuple[int, int]:
    return {"a": (150, 9), "b": (600, 30), "c": (2500, 140)}[os.path.basename(directory)]


def compact(label_: str, message: str, color: str) -> str:
    return json.dumps(badge(label_, message, color), separators=(",", ":")) + "\n"


AFTER_CI = {
    ".nojekyll": "",
    "status-light.svg": "<svg/>",
    ".git/config.json": "{}",
    "gglib-a-tests.json": compact("gglib-a", "8/10", "yellow"),
    "gglib-b-tests.json": compact("gglib-b", "none", "lightgrey"),
    "gglib-c-tests.json": compact("gglib-c", "none", "lightgrey"),
    "gglib-a-coverage.json": KEPT_COVERAGE,
    "gglib-b-coverage.json": compact("gglib-b cov", "pending", "lightgrey"),
    "gglib-a-loc.json": compact("loc", "150", "brightgreen"),
    "gglib-b-loc.json": compact("loc", "600", "yellowgreen"),
    "gglib-c-loc.json": compact("loc", "2500", "orange"),
    "gglib-a-complexity.json": compact("complexity", "9", "brightgreen"),
    "gglib-b-complexity.json": compact("complexity", "30", "yellowgreen"),
    "gglib-c-complexity.json": compact("complexity", "140", "orange"),
    "version.json": compact("version", "v1.2.3-rc.4", "blue"),
}

AFTER_COVERAGE = dict(AFTER_CI)
AFTER_COVERAGE.update({
    "gglib-a-coverage.json": compact("gglib-a cov", "90%", "green"),
    "gglib-b-coverage.json": compact("gglib-b cov", "none", "lightgrey"),
})


def plant(root: str, files: Dict[str, str]) -> None:
    for path, body in files.items():
        full = os.path.join(root, path)
        os.makedirs(os.path.dirname(full), exist_ok=True)
        with open(full, "w", encoding="utf-8") as handle:
            handle.write(textwrap.dedent(body))


def contents(root: str) -> Dict[str, str]:
    out = {}
    for directory, _, files in os.walk(root):
        for name in files:
            path = os.path.join(directory, name)
            with open(path, encoding="utf-8") as handle:
                out[os.path.relpath(path, root).replace(os.sep, "/")] = handle.read()
    return out


def compare(failures: List[str], when: str, actual: Dict[str, str], expected: Dict[str, str]) -> None:
    actual = {k: v for k, v in actual.items() if k != "README.md"}
    for name in sorted(expected.keys() - actual.keys()):
        failures.append(f"{when}: {name} is missing")
    for name in sorted(actual.keys() - expected.keys()):
        failures.append(f"{when}: {name} is there and should not be")
    for name in sorted(expected.keys() & actual.keys()):
        if actual[name] != expected[name]:
            failures.append(f"{when}: {name} is {actual[name]!r}, expected {expected[name]!r}")


def self_test() -> bool:
    failures: List[str] = []
    if parse_scc(SCC_OUTPUT) != (1234, 56):
        failures.append(f"scc output read as {parse_scc(SCC_OUTPUT)}, expected (1234, 56)")
    with tempfile.TemporaryDirectory() as tmp:
        repo, ci, cov, branch, empty = (os.path.join(tmp, d) for d in ("repo", "ci", "cov", "branch", "empty"))
        plant(repo, FIXTURE_REPO)
        plant(ci, FIXTURE_CI)
        plant(cov, FIXTURE_COVERAGE)
        plant(branch, FIXTURE_BRANCH)
        os.makedirs(empty)
        workspace = crate_list.load_strict(repo)

        report = write("ci", workspace, ci, branch, fake_measure)
        compare(failures, "after ci", contents(branch), AFTER_CI)
        if sorted(report["deleted"]) != ["gglib-app-tests.json", "gglib-c-coverage.json", "old/gglib-a-tests.json", "ts-tests.json"]:
            failures.append(f"after ci: deleted {sorted(report['deleted'])}")
        readme = contents(branch).get("README.md", "")
        if "gglib-a, gglib-b, gglib-c." not in readme or "These have no coverage file: gglib-c." not in readme:
            failures.append("after ci: the README does not name the crates and the one without coverage")

        write("coverage", workspace, cov, branch, fake_measure)
        compare(failures, "after coverage", contents(branch), AFTER_COVERAGE)

        before = contents(branch)
        for event, artifacts in (("ci", empty), ("coverage", empty), ("ci", cov), ("coverage", ci),
                                 ("ci", os.path.join(tmp, "absent"))):
            try:
                write(event, workspace, artifacts, branch, fake_measure)
                failures.append(f"{event} with {os.path.basename(artifacts)}: no fault for an artifact with no crate files")
            except Fault:
                pass
        if contents(branch) != before:
            failures.append("an artifact with no crate files changed the branch")
    if not failures:
        print("✅ self-test: counts, kept and pending files, the prune and the empty-artifact fault as fixed")
        return True
    print("❌ self-test failed: the writer no longer leaves the branch the fixture expects")
    for failure in failures:
        print(f"     {failure}")
    return False


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--self-test", action="store_true", help="run the fixtures only")
    commands = parser.add_subparsers(dest="command")
    run = commands.add_parser("write", help="reconcile a checkout of the badges branch")
    run.add_argument("--event", required=True, choices=("ci", "coverage"))
    run.add_argument("--artifacts", required=True, help="the run's downloaded artifact")
    run.add_argument("--branch", required=True, help="a checkout of the badges branch")
    run.add_argument("--scc", default="scc", help="the scc binary (default: scc on PATH)")
    args = parser.parse_args()
    if args.self_test == bool(args.command):
        parser.error("give one of --self-test or write")
    if args.self_test:
        return 0 if self_test() else 1

    workspace = crate_list.load_strict(crate_list.REPO)
    try:
        report = write(args.event, workspace, args.artifacts, args.branch, scc_measure(args.scc))
    except Fault as fault:
        print(f"❌ {fault}")
        return 1
    for outcome, names in report.items():
        print(f"{outcome} ({len(names)}): {' '.join(names) if names else '-'}")
    expected = [f for f in crate_list.expected_branch_files(workspace) if f.endswith(".json")]
    print(f"✅ {args.branch} holds the {len(expected)} badge files crates/layers.toml calls for, and nothing else in JSON")
    return 0


if __name__ == "__main__":
    sys.exit(main())
