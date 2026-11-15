#!/usr/bin/env python3
"""Hold one OS's `cargo test` run to its list of known failures.

`ci.yml`'s `test-cross-os` job runs the workspace's tests on macOS and
Windows, where some do not pass yet. Each OS has a list of those,
`scripts/known-failures/<os>.txt`, and this script decides the job:

* a test that fails and is not listed fails the job;
* a listed test that passes fails the job too, as does a row that is ignored
  in the run or names no test in it, so a list only shrinks, and never holds
  a row for a test that was fixed, renamed or deleted;
* a test binary that printed no `test result:` line fails the job: it
  crashed, or was stopped, and the tests it had not reached are unknown;
* a run this script may have misread fails the job: a binary whose own
  result counts differ from the lines read, one cargo says failed although
  every test in it passed, and two targets of one name in one package.

A row is `<package> <target> <test>`, one per test, and the rows are sorted.
`<target>` is the cargo target the test is built from (a library, a binary or
a file under `tests/`), or `doctests`. A doctest is named without its line
number and its file with `/`, and a file a build script wrote is named from
its `out/` folder, not by the path to it. So a row does not move when a line
is added above it, reads the same on every OS and in every checkout, and
stands for every doctest of its item: it fails while any of them does. A
line starting `#` is a comment.

Usage:
    ./scripts/known_failures.py check --list <list> [--cargo-status N] [--update] <artifacts.json> <test-output.txt>
    ./scripts/known_failures.py --self-test

`<artifacts.json>` is the output of `cargo test --no-run --message-format=json`
and `<test-output.txt>` that of the `cargo test --no-fail-fast` run, as
`split_test_output.py` takes them; both are in the job's artifact, so a list
can be checked or rewritten from a finished run. `--cargo-status` is the
status `cargo test` exited with. `--update` rewrites the list as the run's
failures, keeping the comment lines at its head.

What it does NOT do, so nobody reads more into a pass than is there:

* It does not say why a listed test fails, or that it fails for the reason
  it was listed for.
* It does not run on Linux. Linux's `test` job has no list: every test
  passes there.

Exit codes:
    0  the run failed exactly as listed, or the self-test passed
    1  the run and the list disagree, a binary did not finish, the list is
       malformed, or the self-test failed
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import split_test_output as split_output  # noqa: E402

TEST_LINE = re.compile(r"^test (?P<name>.+?) \.\.\. (?P<status>ok|FAILED|ignored)\b")
RESULT_LINE = re.compile(r"^test result: \S+ (?P<ok>\d+) passed; (?P<FAILED>\d+) failed; (?P<ignored>\d+) ignored;")
# What cargo prints after a test binary, or a crate's doctests, that exited
# with a failure, before it starts the next.
CARGO_ERROR = re.compile(r"^error: (?:test|doctest) failed, to rerun pass ")
# libtest's notice for a test still running after a minute. In a binary that
# never finished, these are the tests it was stopped in.
SLOW_LINE = re.compile(r"^test (?P<name>.+?) has been running for over \d+ seconds$")
# A failed binary prints two blocks after its tests, each under this heading:
# what each failing test wrote, lines that look like results among it, and
# then the failing tests' names, indented, which its result line follows.
FAILURES_BLOCK = re.compile(r"^failures:$")
FAILED_NAME = re.compile(r"^    \S")
STATUSES = ("ok", "FAILED", "ignored")
VARIANT = re.compile(r" - (?:should panic|compile fail|compile)$")
LINE_NUMBER = re.compile(r" \(line \d+\)")
# A build script's output folder, target/<profile>/build/<package>-<hash>/out,
# which rustdoc names in full for a doctest in a file written there.
BUILD_OUT = re.compile(r"^\S*/build/[^/ ]+-[0-9a-f]{16}/out/")
DOCTESTS = "doctests"


class Section:
    """One test binary's part of the run, or one crate's doctests."""

    def __init__(self, package: str, target: str):
        self.package = package
        self.target = target
        self.statuses: dict[str, set[str]] = {}
        # Tests per status: as this script read their lines, and as the
        # binary's own `test result:` lines count them.
        self.counted = dict.fromkeys(STATUSES, 0)
        self.reported = dict.fromkeys(STATUSES, 0)
        self.finished = False
        self.cargo_failed = False
        self.slow: list[str] = []

    def row(self, test: str) -> str:
        return f"{self.package} {self.target} {test}"


def test_name(name: str, doctest: bool) -> str:
    """A test's name as a row holds it."""
    name = VARIANT.sub("", name)
    if doctest:
        name = BUILD_OUT.sub("", LINE_NUMBER.sub("", name).replace("\\", "/"))
        name = re.sub(r" -$", "", name.rstrip())
    return name.rstrip()


def read_run(artifacts_path: str, output_path: str) -> tuple[list[Section], list[str]]:
    """The run's sections, and the binaries and doctest sets no package owns."""
    by_binary: dict[str, tuple[str, str]] = {}
    by_lib: dict[str, str] = {}
    for package, name, kinds, binary in split_output.artifacts(artifacts_path):
        if binary:
            by_binary[binary] = (package, name)
        if any(kind in ("lib", "rlib", "proc-macro") for kind in kinds):
            by_lib[name] = package

    sections: list[Section] = []
    unmapped: list[str] = []
    current: Section | None = None
    # Where in a failed binary's two blocks the line is: None outside them,
    # "opened" on a heading, then "output" or "names".
    block: str | None = None

    with open(output_path, encoding="utf-8", errors="replace") as stream:
        for raw in stream:
            plain = split_output.ANSI.sub("", raw.rstrip("\r\n"))
            running = split_output.RUNNING_PLAIN.match(plain) or split_output.RUNNING_VERBOSE.match(plain)
            doc = split_output.DOC_TESTS.match(plain)

            if running or doc:
                block = None
                current = None
                if running:
                    key = split_output.binary_name(running.group("path"))
                    owner = by_binary.get(key)
                else:
                    key = f"Doc-tests {doc.group('lib')}"
                    package = by_lib.get(doc.group("lib"))
                    owner = (package, DOCTESTS) if package else None
                if owner is None:
                    unmapped.append(key)
                    continue
                current = Section(*owner)
                sections.append(current)
                continue
            if current is None:
                continue

            if FAILURES_BLOCK.match(plain):
                block = "opened"
                continue
            if block == "opened" and plain.strip():
                block = "names" if FAILED_NAME.match(plain) else "output"
            if block == "output":
                continue

            result = RESULT_LINE.match(plain)
            if result:
                current.finished = True
                for status in STATUSES:
                    current.reported[status] += int(result.group(status))
                block = None
            elif CARGO_ERROR.match(plain):
                current.cargo_failed = True
            elif block is None:
                test = TEST_LINE.match(plain)
                slow = SLOW_LINE.match(plain)
                doctest = current.target == DOCTESTS
                if test:
                    name = test_name(test.group("name"), doctest)
                    current.statuses.setdefault(name, set()).add(test.group("status"))
                    current.counted[test.group("status")] += 1
                elif slow:
                    current.slow.append(test_name(slow.group("name"), doctest))

    return sections, unmapped


def read_list(list_path: str) -> tuple[list[str], list[str], list[tuple[str, str]]]:
    """The list's head comments, its rows, and what is wrong with its form."""
    head: list[str] = []
    rows: list[str] = []
    problems: list[tuple[str, str]] = []
    if not os.path.exists(list_path):
        return head, rows, [("list", f"{list_path} does not exist")]
    with open(list_path, encoding="utf-8") as stream:
        for number, raw in enumerate(stream, 1):
            line = raw.rstrip("\r\n")
            if line.startswith("#"):
                if not rows:
                    head.append(line)
                continue
            if not line.strip():
                continue
            if len(line.split(" ", 2)) < 3 or line != line.strip():
                problems.append(("list", f"{list_path}:{number} is not `<package> <target> <test>`: {line!r}"))
                continue
            rows.append(line)
    for before, after in zip(rows, rows[1:]):
        if before == after:
            problems.append(("list", f"{list_path} lists {after} twice"))
        elif before > after:
            problems.append(("list", f"{list_path} is not sorted: {after} comes after {before}"))
    return head, rows, problems


def judge(
    sections: list[Section], unmapped: list[str], listed: list[str], cargo_status: int | None
) -> tuple[list[tuple[str, str]], list[str], dict[str, int]]:
    """What is wrong, as (kind, sentence); the rows that fail; and the counts."""
    problems: list[tuple[str, str]] = []
    failed: set[str] = set()
    passed: set[str] = set()
    ignored: set[str] = set()
    unfinished: set[tuple[str, str]] = set()

    for name in sorted(set(unmapped)):
        problems.append(("unmapped", f"{name} belongs to no package in the build's JSON"))
    if not sections and not unmapped:
        problems.append(("empty", "the run printed no test binary and no doctest set"))

    seen: set[tuple[str, str]] = set()
    for section in sections:
        name = f"{section.package} {section.target}"
        if (section.package, section.target) in seen:
            problems.append(("duplicate", f"{name} ran twice, as two targets of one name: their rows cannot be told apart"))
        seen.add((section.package, section.target))
        if not section.finished:
            unfinished.add((section.package, section.target))
            stopped_in = f"; still running: {', '.join(section.slow)}" if section.slow else ""
            problems.append((
                "unfinished",
                f"{name} printed no `test result:` line: it crashed or was stopped"
                f" after {len(section.statuses)} tests{stopped_in}",
            ))
        elif section.counted != section.reported:
            read, said = (
                ", ".join(f"{counts[status]} {status}" for status in STATUSES)
                for counts in (section.counted, section.reported)
            )
            problems.append(("miscount", f"{name}: its lines read as {read}, and its own result says {said}"))
        elif section.cargo_failed and not section.counted["FAILED"]:
            problems.append(("crashed", f"{name} printed its result with no failure, and cargo says it failed: it exited badly after its tests"))
        elif section.counted["FAILED"] and not section.cargo_failed and cargo_status is not None:
            problems.append(("unreported", f"{name} has a failing test, and cargo did not say it failed"))
        for test, statuses in section.statuses.items():
            row = section.row(test)
            if "FAILED" in statuses:
                failed.add(row)
            elif "ok" in statuses:
                passed.add(row)
            else:
                ignored.add(row)

    listed_set = set(listed)
    for row in sorted(failed - listed_set):
        problems.append(("unlisted", f"fails and is not listed: {row}"))
    for row in listed:
        if row in failed:
            continue
        if row in passed:
            problems.append(("passes", f"is listed and passes: delete the row {row}"))
        elif row in ignored:
            problems.append(("ignored", f"is listed and is ignored in this run: delete the row {row}"))
        elif tuple(row.split(" ", 2)[:2]) not in unfinished:
            problems.append(("missing", f"is listed and names no test in this run: delete the row {row}"))

    if cargo_status not in (None, 0) and not unfinished and not any(section.cargo_failed for section in sections):
        problems.append((
            "cargo",
            f"cargo test exited {cargo_status} and named no test binary that failed: read the run's log",
        ))

    counts = {
        "binaries": len(sections),
        "tests": sum(sum(section.counted.values()) for section in sections),
        "passed": len(passed),
        "failed": len(failed),
        "listed": len(failed & listed_set),
        "ignored": len(ignored),
    }
    return problems, sorted(failed), counts


def check(args: argparse.Namespace) -> int:
    if not os.path.exists(args.output):
        print(f"❌ {args.output} does not exist: the tests did not run")
        return 1
    sections, unmapped = read_run(args.artifacts, args.output)
    head, listed, list_problems = read_list(args.list)
    problems, failed, counts = judge(sections, unmapped, listed, args.cargo_status)

    print(
        f"{counts['tests']} tests in {counts['binaries']} test binaries and doctest sets, as {counts['passed']} "
        f"rows that pass, {counts['failed']} that fail ({counts['listed']} of them listed in {args.list}) "
        f"and {counts['ignored']} ignored"
    )

    if args.update:
        with open(args.list, "w", encoding="utf-8", newline="\n") as handle:
            handle.write("".join(line + "\n" for line in head + failed))
        print(f"✏️  wrote {len(failed)} rows to {args.list}")
        problems = [problem for problem in problems if problem[0] not in ("unlisted", "passes", "ignored", "missing")]
    else:
        problems = list_problems + problems

    if not problems:
        print(f"✅ the run fails exactly as {args.list} says")
        return 0
    print(f"❌ the run and {args.list} disagree:")
    for _, sentence in problems:
        print(f"     {sentence}")
    return 1


# ---------------------------------------------------------------------------
# Self-test

# (package id's tail, target kind, target name, binary, the source as cargo
#  shows it, what cargo says to rerun)
PLANTED_BINARIES = [
    ("crates/gglib-core#0.37.0", "lib", "gglib_core", "gglib_core-0123456789abcdef", ("unittests ", "src", "lib.rs"), "-p gglib-core --lib"),
    ("crates/gglib-agent#0.37.0", "test", "integration_guards", "integration_guards-fedcba9876543210", ("", "tests", "integration_guards.rs"), "-p gglib-agent --test integration_guards"),
]
CORE, GUARDS, DOCS = "gglib_core", "integration_guards", DOCTESTS
GREEN, RED, BOLD, PLAIN = "\x1b[92m", "\x1b[91m", "\x1b[1m", "\x1b[0m"
COUNTED = re.compile(r" \.\.\. (ok|FAILED|ignored)\b")


def planted_run(tests: dict[str, list[tuple[str, str]]], windows: bool = False, **odd):
    """The JSON stream and the output of a run whose sections hold `tests`,
    as (name, status) per target, laid out as cargo and libtest lay one out.

    What a case plants beyond its tests, each by target:
      stopped   prints no result: it hung, or crashed before one
      crashed   cargo says it failed, after a result with no failure
      silent    cargo does not say it failed, though a test in it did
      twice     runs a second time, as a second target of the same name
      glued     {target: lines}, each a result libtest counted that another
                process's output runs into, so it does not start with `test`
      captured  {target: lines} a failing test wrote, shown in its block
      colour    True for a run with CARGO_TERM_COLOR=always
    """
    sep, suffix, root = ("\\", ".exe", "D:\\a\\gglib\\gglib") if windows else ("/", "", "/home/runner/work/gglib/gglib")
    url = "path+file://" + ("" if root.startswith("/") else "/") + root.replace("\\", "/")
    colour = odd.get("colour", False)

    def status_line(word: str, rest: str, shade: str = GREEN) -> str:
        return f"{BOLD}{shade}{word:>12}{PLAIN} {rest}" if colour else f"{word:>12} {rest}"

    def error_line(rerun: str) -> list[str]:
        head = f"{BOLD}{RED}error{PLAIN}{BOLD}:{PLAIN}" if colour else "error:"
        return [f"{head} test failed, to rerun pass `{rerun}`", ""]

    artifacts, output = [], [status_line("Finished", "`test` profile [unoptimized + debuginfo] target(s) in 1.00s")]

    def results(target: str, rerun: str) -> list[str]:
        glued = odd.get("glued", {}).get(target, [])
        lines = ["", f"running {len(tests[target]) + len(glued)} tests"]
        lines += [f"test {name} ... {status}" + (", not on this OS" if status == "ignored" else "") for name, status in tests[target]]
        lines += glued
        if target in odd.get("stopped", ()):
            return lines + ["test planted::hangs has been running for over 60 seconds"]
        tally = dict.fromkeys(STATUSES, 0)
        for line in lines:
            counted = COUNTED.search(line)
            if counted:
                tally[counted.group(1)] += 1
        failures = [name for name, status in tests[target] if status == "FAILED"]
        if failures:
            lines += ["", "failures:", ""]
            for name in failures:
                lines += [f"---- {name} stdout ----", "thread 'planted' panicked at src/lib.rs:1:1:"]
                lines += odd.get("captured", {}).get(target, ["test planted::printed_by_a_test ... FAILED"]) + [""]
            lines += ["", "failures:"] + [f"    {name}" for name in failures]
        verdict = "FAILED" if tally["FAILED"] else "ok"
        lines += ["", f"test result: {verdict}. {tally['ok']} passed; {tally['FAILED']} failed; {tally['ignored']} ignored; 0 measured; 0 filtered out; finished in 0.00s", ""]
        failed = bool(tally["FAILED"]) and target not in odd.get("silent", ())
        return lines + (error_line(rerun) if failed or target in odd.get("crashed", ()) else [])

    for tail, kind, name, binary, (label, folder, source), rerun in PLANTED_BINARIES:
        executable = sep.join([root, "target", "debug", "deps", binary + suffix])
        artifacts.append({"reason": "compiler-artifact", "package_id": f"{url}/{tail}",
                          "target": {"kind": [kind], "name": name}, "executable": executable})
        shown = sep.join(["target", "debug", "deps", binary + suffix])
        for _ in range((name in tests) + (name in odd.get("twice", ()))):
            output += [status_line("Running", f"{label}{folder}{sep}{source} ({shown})")] + results(name, rerun)
    if DOCS in tests:
        output += [status_line("Doc-tests", "gglib_core")] + results(DOCS, "-p gglib-core --doc")
    return "\n".join(json.dumps(message) for message in artifacts) + "\n", "\n".join(output) + "\n"


OK, FAILED, IGNORED = STATUSES
BASE = {CORE: [("paths::a", OK), ("paths::b", OK)], GUARDS: [("guard", OK)], DOCS: [("crates/gglib-core/src/lib.rs - paths (line 3)", OK)]}
ROW_B = "gglib-core gglib_core paths::b"
ROW_GUARD = "gglib-agent integration_guards guard"
ROW_DOC = "gglib-core doctests crates/gglib-core/src/lib.rs - paths"
BUILT_README = "target/debug/build/gglib-core-2f79a457bb12c34a/out/README_GENERATED.md"


def with_tests(changes: dict[str, list[tuple[str, str]]]) -> dict[str, list[tuple[str, str]]]:
    return {**BASE, **changes}


B_FAILS = with_tests({CORE: [("paths::a", OK), ("paths::b", FAILED)]})

# (name, the run's tests, what else it plants, Windows spelling, the list's
#  rows, cargo's status or None when the run was stopped, the kinds of
#  problem that must be found)
PLANTED = [
    ("every test passes and nothing is listed", BASE, {}, False, [], 0, []),
    ("a passing run read by hand, without cargo's status", BASE, {}, False, [], None, []),
    ("a failure that is not listed", B_FAILS, {}, False, [], 101, ["unlisted"]),
    ("a failure that is listed", B_FAILS, {}, False, [ROW_B], 101, []),
    ("a listed failure, in colour", B_FAILS, {"colour": True}, False, [ROW_B], 101, []),
    ("a listed test that passes", BASE, {}, False, [ROW_B], 0, ["passes"]),
    ("a listed test that is ignored", with_tests({CORE: [("paths::a", OK), ("paths::b", IGNORED)]}), {}, False, [ROW_B], 0, ["ignored"]),
    ("a row that names no test", BASE, {}, False, ["gglib-core gglib_core paths::gone"], 0, ["missing"]),
    ("a binary that crashed before its result", BASE, {"stopped": (GUARDS,)}, False, [], 101, ["unfinished"]),
    ("a binary stopped by the step's time limit", BASE, {"stopped": (GUARDS,)}, False, [], None, ["unfinished"]),
    # Its tests are unknown, so its rows are neither passing nor missing.
    ("a listed test in a binary that did not finish", with_tests({GUARDS: []}), {"stopped": (GUARDS,)}, False, [ROW_GUARD], 101, ["unfinished"]),
    ("cargo failed and no test binary did", BASE, {}, False, [], 101, ["cargo"]),
    # The next four would pass behind a listed failure if only cargo's
    # status were read: each is a failure with no line of its own to list.
    ("a listed failure, and a binary that crashed after its result", B_FAILS, {"crashed": (GUARDS,)}, False, [ROW_B], 101, ["crashed"]),
    (
        "a listed failure, and a failure another process's output runs into",
        B_FAILS, {"glued": {GUARDS: ["a child wrote this test guard_two ... FAILED"]}}, False, [ROW_B], 101, ["miscount"],
    ),
    ("a failing binary that cargo did not name", B_FAILS, {"silent": (CORE,)}, False, [ROW_B], 101, ["cargo", "unreported"]),
    ("two targets of one name", B_FAILS, {"twice": (CORE,)}, False, [ROW_B], 101, ["duplicate"]),
    # What a failing test wrote is not the run's: a result line and a
    # failure among it are neither this binary's result nor a test.
    (
        "a failing test that printed a result of its own",
        B_FAILS,
        {"captured": {CORE: ["test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out", "test planted::fake ... FAILED"]}},
        False, [ROW_B], 101, [],
    ),
    ("a list out of order", with_tests({CORE: [("paths::a", FAILED), ("paths::b", FAILED)]}), {}, False, [ROW_B, "gglib-core gglib_core paths::a"], 101, ["list"]),
    ("a row listed twice", B_FAILS, {}, False, [ROW_B, ROW_B], 101, ["list"]),
    ("a row with two fields", B_FAILS, {}, False, ["gglib-core paths::b", ROW_B], 101, ["list"]),
    # Windows spells the binary with `.exe` and every path with `\`; a
    # doctest's row has neither, nor its line number or its variant.
    (
        "a Windows run with a listed doctest",
        with_tests({DOCS: [("crates\\gglib-core\\src\\lib.rs - paths (line 9) - should panic", FAILED)]}),
        {}, True, [ROW_DOC], 101, [],
    ),
    ("a Windows run with an unlisted failure", B_FAILS, {}, True, [], 101, ["unlisted"]),
    ("a test that panics as it should", with_tests({GUARDS: [("guard - should panic", FAILED)]}), {}, False, [ROW_GUARD], 101, []),
    ("a doctest that must not compile", with_tests({DOCS: [("crates/gglib-core/src/lib.rs - paths (line 3) - compile fail", FAILED)]}), {}, False, [ROW_DOC], 101, []),
    # A README a build script wrote: rustdoc names it by its whole path, and
    # the row by what follows `out/`. Two doctests of one item are one row.
    (
        "doctests in a file a build script wrote",
        with_tests({DOCS: [
            (f"/home/runner/work/gglib/gglib/{BUILT_README} - (line 104) - compile", OK),
            (f"/home/runner/work/gglib/gglib/{BUILT_README} - (line 122)", FAILED),
        ]}),
        {}, False, ["gglib-core doctests README_GENERATED.md"], 101, [],
    ),
    (
        "the same file on Windows",
        with_tests({DOCS: [("D:\\a\\gglib\\gglib\\" + BUILT_README.replace("/", "\\") + " - (line 122)", FAILED)]}),
        {}, True, ["gglib-core doctests README_GENERATED.md"], 101, [],
    ),
]


def self_test() -> int:
    wrong: list[str] = []
    with tempfile.TemporaryDirectory() as tmp:
        artifacts_path, output_path, list_path = (os.path.join(tmp, name) for name in ("a.json", "o.txt", "l.txt"))

        def write(path: str, text: str) -> None:
            with open(path, "w", encoding="utf-8") as handle:
                handle.write(text)

        def run_check(update: bool, status: int | None) -> int:
            arguments = argparse.Namespace(
                artifacts=artifacts_path, output=output_path, list=list_path, cargo_status=status, update=update
            )
            stdout, sys.stdout = sys.stdout, open(os.devnull, "w", encoding="utf-8")
            try:
                return check(arguments)
            finally:
                sys.stdout.close()
                sys.stdout = stdout

        for name, tests, odd, windows, rows, status, expected in PLANTED:
            artifacts, output = planted_run(tests, windows, **odd)
            write(artifacts_path, artifacts)
            write(output_path, output)
            write(list_path, "# planted\n" + "".join(row + "\n" for row in rows))
            sections, unmapped = read_run(artifacts_path, output_path)
            _, listed, list_problems = read_list(list_path)
            problems, _, _ = judge(sections, unmapped, listed, status)
            found = sorted({kind for kind, _ in list_problems + problems})
            if found != sorted(expected):
                wrong.append(f"{name}: found {found or 'nothing'}, planted {expected or 'nothing'}")
            if run_check(False, status) != (1 if expected else 0):
                wrong.append(f"{name}: `check` did not exit {1 if expected else 0}")

        # --update writes the run's failures, sorted, under the list's head.
        artifacts, output = planted_run(with_tests({CORE: [("paths::a", OK), ("paths::b", FAILED)], GUARDS: [("guard", FAILED)]}))
        write(artifacts_path, artifacts)
        write(output_path, output)
        write(list_path, "# planted\ngglib-core gglib_core paths::gone\n")
        status = run_check(True, 101)
        with open(list_path, encoding="utf-8") as handle:
            written = handle.read()
        if status != 0 or written != f"# planted\n{ROW_GUARD}\n{ROW_B}\n":
            wrong.append(f"--update: exited {status} and wrote {written!r}")

    if wrong:
        print("❌ self-test: a planted run was not judged as planted:")
        for line in wrong:
            print(f"     {line}")
        return 1
    faults = sum(1 for case in PLANTED if case[6])
    print(f"✅ self-test: {len(PLANTED)} planted runs, {faults} with a fault and each found, nothing else; --update as pinned")
    return 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    commands = parser.add_subparsers(dest="command", required=True)
    checker = commands.add_parser("check", help="hold a run to a list")
    checker.add_argument("--list", required=True, help="the OS's list, scripts/known-failures/<os>.txt")
    checker.add_argument("--cargo-status", type=int, help="the status `cargo test` exited with")
    checker.add_argument("--update", action="store_true", help="rewrite the list as this run's failures")
    checker.add_argument("artifacts", help="output of `cargo test --no-run --message-format=json`")
    checker.add_argument("output", help="output of the `cargo test --no-fail-fast` run")
    return check(parser.parse_args())


if __name__ == "__main__":
    sys.exit(main())
