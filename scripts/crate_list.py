#!/usr/bin/env python3
"""The workspace's crate lists, from crates/layers.toml, and the checks that keep it true.

crates/layers.toml names every workspace member's layer and whether it has
badges. Everything that needs a list of crates asks this script for one, so
there is one list to keep and nothing else to drift from it:

    crate_list.py list members       every workspace member
    crate_list.py list badged        the members with badges
    crate_list.py list coverage      the badged members coverage.yml measures
    crate_list.py list adapter       the adapter layer: the surface crates
    crate_list.py list non-adapter   every other member
    crate_list.py readmes [--write]  each badged crate README's header block
    crate_list.py --check            self-test, then check the repository
    crate_list.py --self-test        run the fixtures only

`list` prints one package name per line, in the order the root Cargo.toml lists
the members, and refuses (exit 1) while layers.toml and the workspace disagree,
so a workflow never measures a list that is short of a crate.

`readmes` writes the block between `<!-- crate-header:start -->` and
`<!-- crate-header:end -->` in each badged crate's README: one line of shields
badges for its tests, coverage, lines of code and complexity. A README with no
block gets one under its title. Without `--write` it names the READMEs whose
block is missing or out of date.

`--check` fails when:

1. a workspace member has no row in layers.toml, or a row names no member;
2. a row's layer is not in `order`, a row has a key this script does not read,
   or a `false` has no `why` (or a `why` explains no `false`);
3. a crate has a normal or build dependency, on any target, on a crate in a
   layer above its own. Dev-dependencies are not read;
4. a tracked Markdown file links a file on the `badges` branch that the
   pipeline does not write: `<crate>-<metric>.json` for each badged crate,
   `version.json`, and the status card's two SVGs;
5. a badged crate has no README, or its header block is not what `readmes
   --write` would write.

What it does NOT check, so that nobody reads more into a pass than is there:

* That a crate's layer is the right one for what it does. It checks only that
  the dependencies agree with the layers as written.
* Dependencies a crate reaches through another crate. Only direct edges are
  read, from the manifests, without running cargo.
* Workspace members given as globs. The root Cargo.toml lists each path.
* Badge links in files that are not Markdown, and links to the branch written
  any other way than through raw.githubusercontent.com.

Exit codes:
    0  the lists were printed, the READMEs are current, or every check passed
    1  layers.toml and the workspace disagree, a check failed, or the
       self-test failed
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
import tempfile
import textwrap
import tomllib
from typing import Dict, List, NamedTuple, Optional, Set, Tuple

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LAYERS = "crates/layers.toml"

BRANCH_URL = "https://raw.githubusercontent.com/mmogr/gglib/badges/"
SHIELD_URL = "https://img.shields.io/endpoint?url=" + BRANCH_URL
BRANCH_LINK = re.compile(r"raw\.githubusercontent\.com/mmogr/gglib/badges/([^\s)\"'<>]+)")

# (metric, alt text), in the order the header shows them.
METRICS = (("tests", "Tests"), ("coverage", "Coverage"), ("loc", "LOC"), ("complexity", "Complexity"))
VERSION_FILE = "version.json"
STATUS_CARD = ("status-light.svg", "status-dark.svg")

HEADER_START = "<!-- crate-header:start -->"
HEADER_END = "<!-- crate-header:end -->"

ROW_KEYS = {"layer", "badges", "coverage", "why"}
DEPENDENCY_TABLES = ("dependencies", "build-dependencies")
KINDS = ("members", "badged", "coverage", "adapter", "non-adapter")


class Crate(NamedTuple):
    name: str
    path: str  # the member's directory, relative to the root
    layer: str
    badges: bool
    coverage: bool  # measured by coverage.yml: badged, and not `coverage = false`
    deps: Tuple[str, ...]  # members it names in a normal or build dependency


class Problem(NamedTuple):
    path: str
    kind: str
    subject: str
    message: str

    def render(self) -> str:
        return f"{self.path}: {self.message}"


class Workspace(NamedTuple):
    root: str
    order: Tuple[str, ...]
    crates: Tuple[Crate, ...]
    problems: Tuple[Problem, ...]

    def named(self, kind: str) -> List[Crate]:
        if kind == "members":
            return list(self.crates)
        if kind == "badged":
            return [c for c in self.crates if c.badges]
        if kind == "coverage":
            return [c for c in self.crates if c.coverage]
        if kind == "adapter":
            return [c for c in self.crates if c.layer == "adapter"]
        if kind == "non-adapter":
            return [c for c in self.crates if c.layer != "adapter"]
        raise ValueError(kind)


def read_toml(path: str) -> dict:
    with open(path, "rb") as handle:
        return tomllib.load(handle)


def dependency_names(manifest: dict) -> Set[str]:
    """Every package a manifest names in a normal or build dependency, on any target."""
    tables = [manifest.get(name) or {} for name in DEPENDENCY_TABLES]
    for target in (manifest.get("target") or {}).values():
        tables += [target.get(name) or {} for name in DEPENDENCY_TABLES]
    names = set()
    for table in tables:
        for key, value in table.items():
            if isinstance(value, dict) and "package" in value:
                names.add(value["package"])
            else:
                names.add(key)
    return names


def load(root: str) -> Workspace:
    """The workspace as layers.toml and the manifests describe it, and where they disagree."""
    problems: List[Problem] = []
    members = read_toml(os.path.join(root, "Cargo.toml"))["workspace"]["members"]
    packages: Dict[str, Tuple[str, Set[str]]] = {}
    for path in members:
        manifest = read_toml(os.path.join(root, path, "Cargo.toml"))
        packages[manifest["package"]["name"]] = (path, dependency_names(manifest))

    layers = read_toml(os.path.join(root, LAYERS))
    order = layers.get("order")
    if not isinstance(order, list) or not order or len(set(order)) != len(order):
        problems.append(Problem(LAYERS, "order", "order", "`order` must be a list of distinct layer names"))
        order = []
    rows = layers.get("crate") or {}

    for name, row in rows.items():
        for key in sorted(set(row) - ROW_KEYS):
            problems.append(Problem(LAYERS, "key", name, f"[crate.{name}] has `{key}`, which nothing reads"))
        if row.get("layer") not in order:
            problems.append(Problem(LAYERS, "layer", name, f"[crate.{name}] layer {row.get('layer')!r} is not in `order`"))
        falses = [key for key in ("badges", "coverage") if row.get(key) is False]
        if falses and not str(row.get("why", "")).strip():
            said = " and ".join(f"`{key} = false`" for key in falses)
            problems.append(Problem(LAYERS, "why", name, f"[crate.{name}] sets {said} with no `why`"))
        if "why" in row and not falses:
            problems.append(Problem(LAYERS, "why", name, f"[crate.{name}] has a `why` but nothing set to false"))
        if name not in packages:
            problems.append(Problem(LAYERS, "no-member", name, f"[crate.{name}] names no workspace member"))

    crates = []
    for name, (path, deps) in packages.items():
        row = rows.get(name)
        if row is None:
            problems.append(Problem(LAYERS, "no-row", name, f"workspace member {name} ({path}) has no [crate.{name}] row"))
            continue
        badges = row.get("badges", True) is not False
        coverage = badges and row.get("coverage", True) is not False
        crates.append(Crate(name, path, str(row.get("layer")), badges, coverage,
                            tuple(sorted(d for d in deps if d in packages and d != name))))

    rank = {layer: index for index, layer in enumerate(order)}
    layer_of = {c.name: c.layer for c in crates}
    for crate in crates:
        for dep in crate.deps:
            if crate.layer in rank and layer_of.get(dep) in rank and rank[layer_of[dep]] < rank[crate.layer]:
                problems.append(Problem(
                    os.path.join(crate.path, "Cargo.toml"), "edge", f"{crate.name}->{dep}",
                    f"{crate.name} ({crate.layer}) depends on {dep} ({layer_of[dep]}), a layer above its own"))
    return Workspace(root, tuple(order), tuple(crates), tuple(problems))


def load_strict(root: str = REPO) -> Workspace:
    """`load`, but a disagreement between layers.toml and the workspace ends the run."""
    workspace = load(root)
    if workspace.problems:
        for problem in workspace.problems:
            print(f"❌ {problem.render()}", file=sys.stderr)
        print(f"❌ {LAYERS} and the workspace disagree; `crate_list.py --check` says the same", file=sys.stderr)
        raise SystemExit(1)
    return workspace


def badge_metrics(crate: Crate) -> List[Tuple[str, str]]:
    return [(metric, alt) for metric, alt in METRICS if crate.badges and (metric != "coverage" or crate.coverage)]


def badge_files(crate: Crate) -> List[str]:
    return [f"{crate.name}-{metric}.json" for metric, _ in badge_metrics(crate)]


def expected_branch_files(workspace: Workspace) -> Set[str]:
    """Every file on the `badges` branch a README may link: what the pipeline writes."""
    files = {VERSION_FILE, *STATUS_CARD}
    for crate in workspace.crates:
        files.update(badge_files(crate))
    return files


def header_lines(crate: Crate) -> List[str]:
    """The header block's lines. Each is its own paragraph."""
    badges = " ".join(f"![{alt}]({SHIELD_URL}{crate.name}-{metric}.json)" for metric, alt in badge_metrics(crate))
    return [badges]


def header_block(crate: Crate) -> str:
    return HEADER_START + "\n" + "\n\n".join(header_lines(crate)) + "\n" + HEADER_END


def with_header(text: str, crate: Crate) -> Optional[str]:
    """The README text with its header block current, or None when it cannot be placed."""
    block = header_block(crate)
    if text.count(HEADER_START) == 1 and text.count(HEADER_END) == 1:
        start, end = text.index(HEADER_START), text.index(HEADER_END)
        if start > end:
            return None
        return text[:start] + block + text[end + len(HEADER_END):]
    if HEADER_START in text or HEADER_END in text:
        return None
    lines = text.split("\n")
    for index, line in enumerate(lines):
        if line.startswith("# "):
            rest = "\n".join(lines[index + 1:]).lstrip("\n")
            return "\n".join(lines[:index + 1]) + "\n\n" + block + "\n\n" + rest
    return None


def readme_path(crate: Crate) -> str:
    return os.path.join(crate.path, "README.md")


def header_problems(workspace: Workspace) -> List[Problem]:
    problems = []
    for crate in workspace.named("badged"):
        path = readme_path(crate)
        try:
            with open(os.path.join(workspace.root, path), encoding="utf-8") as handle:
                text = handle.read()
        except FileNotFoundError:
            problems.append(Problem(path, "readme", crate.name, f"{crate.name} has badges but no README"))
            continue
        current = with_header(text, crate)
        if current is None:
            problems.append(Problem(path, "header", crate.name, "the crate-header markers are unpaired, or there is no `# ` title to put the block under"))
        elif current != text:
            problems.append(Problem(path, "header", crate.name, "the crate-header block is missing or out of date; run `make readmes`"))
    return problems


def write_readmes(workspace: Workspace) -> Tuple[List[str], List[Problem]]:
    written, problems = [], []
    for problem in header_problems(workspace):
        if problem.kind != "header":
            problems.append(problem)
            continue
        crate = next(c for c in workspace.crates if c.name == problem.subject)
        full = os.path.join(workspace.root, readme_path(crate))
        with open(full, encoding="utf-8") as handle:
            current = with_header(handle.read(), crate)
        if current is None:
            problems.append(problem)
            continue
        with open(full, "w", encoding="utf-8") as handle:
            handle.write(current)
        written.append(problem.path)
    return written, problems


def link_problems(workspace: Workspace, markdown: List[str]) -> Tuple[List[Problem], int]:
    expected = expected_branch_files(workspace)
    problems, read = [], 0
    for path in markdown:
        with open(os.path.join(workspace.root, path), encoding="utf-8") as handle:
            text = handle.read()
        for match in BRANCH_LINK.finditer(text):
            read += 1
            name = match.group(1)
            if name not in expected:
                line = text.count("\n", 0, match.start()) + 1
                problems.append(Problem(f"{path}:{line}", "link", name, f"links {name} on the badges branch, which the pipeline does not write"))
    return problems, read


def tracked_markdown(root: str) -> List[str]:
    out = subprocess.run(["git", "ls-files", "-z", "--", "*.md"], cwd=root, check=True,
                         capture_output=True, text=True).stdout
    return sorted(path for path in out.split("\0") if path)


def all_problems(root: str, markdown: List[str]) -> Tuple[Workspace, List[Problem], int]:
    workspace = load(root)
    problems = list(workspace.problems) + header_problems(workspace)
    links, read = link_problems(workspace, markdown)
    return workspace, problems + links, read


# ---------------------------------------------------------------------------
# Self-test

SHIELD = SHIELD_URL
FIXTURE = {
    "Cargo.toml": """\
        [workspace]
        members = ["crates/top", "crates/mid", "crates/low", "crates/norow", "crates/quiet", "app"]
        """,
    "crates/layers.toml": """\
        order = ["adapter", "middle", "bottom"]

        [crate.gglib-top]
        layer = "adapter"

        [crate.gglib-mid]
        layer = "middle"
        coverage = false
        why = "Its build script needs a toolchain the coverage runner lacks."

        [crate.gglib-low]
        layer = "bottom"
        badge = false

        [crate.gglib-quiet]
        layer = "sideways"
        badges = false

        [crate.gglib-app]
        layer = "adapter"
        badges = false
        why = "No library code."

        [crate.gglib-gone]
        layer = "bottom"
        why = "Explains nothing."
        """,
    "crates/top/Cargo.toml": """\
        [package]
        name = "gglib-top"
        [dependencies]
        gglib-mid = { workspace = true }
        gglib-app = { path = "../../app" }
        serde = "1"
        """,
    "crates/mid/Cargo.toml": """\
        [package]
        name = "gglib-mid"
        [dependencies]
        gglib-low = { path = "../low" }
        [build-dependencies]
        renamed = { package = "gglib-top", path = "../top" }
        [dev-dependencies]
        gglib-top = { path = "../top" }
        """,
    "crates/low/Cargo.toml": """\
        [package]
        name = "gglib-low"
        [target.'cfg(unix)'.dependencies]
        gglib-mid = { path = "../mid" }
        [target.'cfg(windows)'.dev-dependencies]
        gglib-top = { path = "../top" }
        """,
    "crates/norow/Cargo.toml": """\
        [package]
        name = "gglib-norow"
        """,
    "crates/quiet/Cargo.toml": """\
        [package]
        name = "gglib-quiet"
        """,
    "app/Cargo.toml": """\
        [package]
        name = "gglib-app"
        """,
    # Current, and kept byte for byte by --write.
    "crates/top/README.md": f"""\
        # gglib-top

        {HEADER_START}
        ![Tests]({SHIELD}gglib-top-tests.json) ![Coverage]({SHIELD}gglib-top-coverage.json) ![LOC]({SHIELD}gglib-top-loc.json) ![Complexity]({SHIELD}gglib-top-complexity.json)
        {HEADER_END}

        The top crate.
        """,
    # Out of date: still shows a coverage badge after `coverage = false`.
    "crates/mid/README.md": f"""\
        # gglib-mid

        {HEADER_START}
        ![Tests]({SHIELD}gglib-mid-tests.json) ![Coverage]({SHIELD}gglib-mid-coverage.json)
        {HEADER_END}

        The middle crate.
        """,
    # No block yet: --write puts one under the title.
    "crates/low/README.md": """\
        # gglib-low


        The bottom crate.
        """,
    # Unbadged crates are not read for a header.
    "app/README.md": """\
        # The app
        """,
    "docs/links.md": f"""\
        ![v]({SHIELD}version.json)
        <img src="{BRANCH_URL}status-dark.svg">
        ![t]({SHIELD}gglib-top-tests.json) ![old]({SHIELD}tests.json)
        ![app]({SHIELD}gglib-app-tests.json)
        ![cov]({SHIELD}gglib-low-coverage.json)
        """,
}

# The faults the fixture plants. The dev-dependency edges (mid -> top, and
# low -> top on a target), a same-layer edge (top -> app) and an edge to a
# crate below are not faults.
EXPECTED = {
    (LAYERS, "no-row", "gglib-norow"),
    (LAYERS, "no-member", "gglib-gone"),
    (LAYERS, "why", "gglib-gone"),
    (LAYERS, "key", "gglib-low"),
    (LAYERS, "layer", "gglib-quiet"),
    (LAYERS, "why", "gglib-quiet"),
    ("crates/mid/Cargo.toml", "edge", "gglib-mid->gglib-top"),
    ("crates/low/Cargo.toml", "edge", "gglib-low->gglib-mid"),
    ("crates/mid/README.md", "header", "gglib-mid"),
    ("crates/low/README.md", "header", "gglib-low"),
    ("crates/mid/README.md:4", "link", "gglib-mid-coverage.json"),
    ("docs/links.md:3", "link", "tests.json"),
    ("docs/links.md:4", "link", "gglib-app-tests.json"),
}

# What --write leaves in the two READMEs it rewrites: the format, pinned.
WRITTEN = {
    "crates/mid/README.md": f"""\
        # gglib-mid

        {HEADER_START}
        ![Tests]({SHIELD}gglib-mid-tests.json) ![LOC]({SHIELD}gglib-mid-loc.json) ![Complexity]({SHIELD}gglib-mid-complexity.json)
        {HEADER_END}

        The middle crate.
        """,
    "crates/low/README.md": f"""\
        # gglib-low

        {HEADER_START}
        ![Tests]({SHIELD}gglib-low-tests.json) ![Coverage]({SHIELD}gglib-low-coverage.json) ![LOC]({SHIELD}gglib-low-loc.json) ![Complexity]({SHIELD}gglib-low-complexity.json)
        {HEADER_END}

        The bottom crate.
        """,
}

LISTS = {
    "members": ["gglib-top", "gglib-mid", "gglib-low", "gglib-quiet", "gglib-app"],
    "badged": ["gglib-top", "gglib-mid", "gglib-low"],
    "coverage": ["gglib-top", "gglib-low"],
    "adapter": ["gglib-top", "gglib-app"],
    "non-adapter": ["gglib-mid", "gglib-low", "gglib-quiet"],
}


def self_test() -> bool:
    failures = []
    with tempfile.TemporaryDirectory() as root:
        for path, body in FIXTURE.items():
            os.makedirs(os.path.join(root, os.path.dirname(path)), exist_ok=True)
            with open(os.path.join(root, path), "w", encoding="utf-8") as handle:
                handle.write(textwrap.dedent(body))
        markdown = sorted(p for p in FIXTURE if p.endswith(".md"))
        workspace, problems, read = all_problems(root, markdown)
        found = {(p.path, p.kind, p.subject) for p in problems}
        for item in sorted(EXPECTED - found):
            failures.append(f"not reported: {item}")
        for problem in problems:
            if (problem.path, problem.kind, problem.subject) not in EXPECTED:
                failures.append(f"reported but not planted: {problem.render()}")
        if len(problems) != len(found):
            failures.append("one fault was reported more than once")
        if read != 12:
            failures.append(f"read {read} badge links, the fixture has 12")
        for kind, names in LISTS.items():
            if [c.name for c in workspace.named(kind)] != names:
                failures.append(f"list {kind}: {[c.name for c in workspace.named(kind)]}, expected {names}")

        written, left = write_readmes(workspace)
        if sorted(written) != sorted(WRITTEN) or left:
            failures.append(f"--write rewrote {sorted(written)} and left {[p.render() for p in left]}")
        for path, body in WRITTEN.items():
            with open(os.path.join(root, path), encoding="utf-8") as handle:
                if handle.read() != textwrap.dedent(body):
                    failures.append(f"--write left {path} other than the pinned text")
        with open(os.path.join(root, "crates/top/README.md"), encoding="utf-8") as handle:
            if handle.read() != textwrap.dedent(FIXTURE["crates/top/README.md"]):
                failures.append("--write changed a README that was current")
        if header_problems(workspace):
            failures.append("a header is still reported after --write")
    if not failures:
        print(f"✅ self-test: {len(EXPECTED)} planted faults found, nothing else; lists and --write as pinned")
        return True
    print("❌ self-test failed: the checker no longer reports what the fixture plants")
    for failure in failures:
        print(f"     {failure}")
    return False


def check() -> bool:
    markdown = tracked_markdown(REPO)
    workspace, problems, read = all_problems(REPO, markdown)
    badged = workspace.named("badged")
    print(f"Checking {LAYERS} ({len(workspace.crates)} crates, {len(badged)} with badges, {read} badge links read)...")
    # Liveness: a check that read nothing prints what a clean one prints.
    if not workspace.crates or not badged or not read:
        print("❌ no crate, no badged crate or no badge link was found; nothing was checked")
        return False
    if not problems:
        print(f"✅ {LAYERS} matches the workspace, no crate depends upward, and every badge link and header is current")
        return True
    for problem in problems:
        print(f"❌ {problem.render()}")
    print(f"\n❌ {len(problems)} problem(s). Fix the row in {LAYERS} or the dependency it names;")
    print("   link only files the pipeline writes, and run `make readmes` for a header.")
    return False


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--check", action="store_true", help="self-test, then check the repository")
    parser.add_argument("--self-test", action="store_true", help="run the fixtures only")
    commands = parser.add_subparsers(dest="command")
    listing = commands.add_parser("list", help="print one list of crates, one name per line")
    listing.add_argument("kind", choices=KINDS)
    readmes = commands.add_parser("readmes", help="name, or with --write rewrite, the stale README headers")
    readmes.add_argument("--write", action="store_true")
    args = parser.parse_args()
    if sum(bool(x) for x in (args.check, args.self_test, args.command)) != 1:
        parser.error("give one of --check, --self-test, list or readmes")

    if args.check or args.self_test:
        if not self_test():
            return 1
        return 0 if args.self_test or check() else 1
    workspace = load_strict(REPO)
    if args.command == "list":
        for crate in workspace.named(args.kind):
            print(crate.name)
        return 0
    if args.write:
        written, left = write_readmes(workspace)
        for path in written:
            print(f"wrote {path}")
    else:
        left = header_problems(workspace)
    for problem in left:
        print(f"❌ {problem.render()}")
    return 1 if left else 0


if __name__ == "__main__":
    sys.exit(main())
