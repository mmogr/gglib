#!/usr/bin/env python3
"""The workspace's crate graph and catalogue, written into the READMEs from the manifests.

Two generated blocks, from crates/layers.toml and the members' Cargo.toml
files as scripts/crate_list.py reads them:

* the graph, between `<!-- crate-graph:start -->` and `<!-- crate-graph:end -->`,
  in crates/README.md (under `## Architecture Overview`) and in
  crates/gglib-core/README.md (under `## Architecture`, the crate's rustdoc
  front page). One band per layer, from the top of `order` down, naming its
  crates; under the bands, one line per crate listing the members it names in
  a normal or build dependency.
* the catalogue, between `<!-- crate-catalog:start -->` and
  `<!-- crate-catalog:end -->`, in crates/README.md: one table row per crate,
  its layer, its manifest's `description` and its badges.

    crate_graph.py --write      rewrite both blocks wherever they are stale
    crate_graph.py --check      self-test, then fail while a block is stale
    crate_graph.py --self-test  run the fixtures only

`--check` fails when:

1. a README this script writes into is missing, or its markers for a block
   are missing, repeated or out of order. The markers are placed by hand:
   where a block sits in its README is a reader's choice, not this script's;
2. a block is not what `--write` would write;
3. a crate's manifest has no `description`, which the catalogue shows.

What it does NOT check, so that nobody reads more into a pass than is there:

* That a crate's layer is right, or that no crate depends upward:
  `crate_list.py --check` does that, on the same reading of the manifests.
* Dependencies through another crate, dev-dependencies, or features. An edge
  is a direct normal or build dependency, on any target.
* The hand-drawn diagrams elsewhere in the READMEs.
  `check_diagram_names.py` checks the names they use, and nothing checks
  their arrows.

Exit codes:
    0  the blocks were written, or are current
    1  a block is stale, a marker or description is missing, the workspace
       and layers.toml disagree, or the self-test failed
"""

from __future__ import annotations

import argparse
import os
import posixpath
import sys
import tempfile
import textwrap
import tomllib
from typing import Callable, Dict, List, NamedTuple, Optional, Tuple

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import crate_list  # noqa: E402
from crate_list import Crate, Workspace  # noqa: E402

GRAPH = "crate-graph"
CATALOG = "crate-catalog"
# Each README this script writes into, and the blocks it holds.
TARGETS = {
    "crates/README.md": (GRAPH, CATALOG),
    "crates/gglib-core/README.md": (GRAPH,),
}
CATALOG_HOME = "crates"  # the catalogue's links are relative to crates/README.md
PER_ROW = 4
MIN_CELL = 16  # a cell widens past this only for a name that would not fit
WIDTH = 80
PREFIX = "gglib-"

LEGEND = (
    "Each band is a layer of `crates/layers.toml`, from the top down. Under the bands, "
    "each crate is listed with the workspace crates it names in a normal or build "
    "dependency, as its manifest lists them; dev-dependencies are left out."
)


class Problem(NamedTuple):
    path: str
    message: str

    def render(self) -> str:
        return f"{self.path}: {self.message}"


def label(crate: Crate) -> str:
    """The crate's directory name: the package name, or `src-tauri` for the app."""
    return posixpath.basename(crate.path)


def short(crate: Crate) -> str:
    return label(crate).removeprefix(PREFIX)


def ordered(workspace: Workspace, crates: List[Crate]) -> List[Crate]:
    rank = {layer: index for index, layer in enumerate(workspace.order)}
    return sorted(crates, key=lambda c: (rank.get(c.layer, len(rank)), short(c)))


def render_graph(workspace: Workspace) -> str:
    crates = ordered(workspace, list(workspace.crates))
    by_name = {c.name: c for c in crates}
    cell = max([MIN_CELL] + [len(short(c)) + 2 for c in crates])
    rule = 2 + PER_ROW * cell
    lines = []
    for layer in workspace.order:
        names = [short(c) for c in crates if c.layer == layer]
        if not names:
            continue
        title = layer.upper()
        lines.append(f"{title} " + "─" * (rule - len(title) - 1))
        for start in range(0, len(names), PER_ROW):
            lines.append(("  " + "".join(n.ljust(cell) for n in names[start:start + PER_ROW])).rstrip())
    lines += ["", "DEPENDS ON " + "─" * (rule - len("DEPENDS ON") - 1)]
    lead = 2 + cell
    for crate in crates:
        deps = ordered(workspace, [by_name[d] for d in crate.deps])
        text = ", ".join(short(d) for d in deps) or "none"
        lines += textwrap.wrap(text, width=WIDTH, initial_indent=f"  {short(crate).ljust(cell)}→ ",
                               subsequent_indent=" " * (lead + 2), break_long_words=False,
                               break_on_hyphens=False)
    return LEGEND + "\n\n```text\n" + "\n".join(lines) + "\n```"


def description(root: str, crate: Crate) -> Optional[str]:
    with open(os.path.join(root, crate.path, "Cargo.toml"), "rb") as handle:
        text = tomllib.load(handle).get("package", {}).get("description")
    return " ".join(text.split()) if isinstance(text, str) and text.strip() else None


def render_catalog(workspace: Workspace, problems: List[Problem]) -> str:
    rows = ["| Crate | Layer | Description | Badges |", "|-------|-------|-------------|--------|"]
    for crate in ordered(workspace, list(workspace.crates)):
        text = description(workspace.root, crate)
        if text is None:
            problems.append(Problem(f"{crate.path}/Cargo.toml", "has no `description`, which the catalogue shows"))
            text = ""
        link = posixpath.relpath(crate.path, CATALOG_HOME) + "/"
        badges = crate_list.badge_line(crate) or "—"
        rows.append(f"| [{label(crate)}]({link}) | {crate.layer} | {text.replace('|', chr(92) + '|')} | {badges} |")
    return "\n".join(rows)


def markers(name: str) -> Tuple[str, str]:
    return f"<!-- {name}:start -->", f"<!-- {name}:end -->"


def with_block(text: str, name: str, body: str) -> Optional[str]:
    """The text with the block's body replaced, or None when its markers are not one ordered pair."""
    start, end = markers(name)
    if text.count(start) != 1 or text.count(end) != 1 or text.index(start) > text.index(end):
        return None
    head, rest = text.split(start)
    _, tail = rest.split(end)
    return head + start + "\n" + body + "\n" + end + tail


def rendered(root: str, workspace: Workspace) -> Tuple[Dict[str, Optional[str]], List[Problem]]:
    """Each target README as --write would leave it (None when it cannot be written), and the problems."""
    problems: List[Problem] = []
    bodies: Dict[str, Callable[[], str]] = {
        GRAPH: lambda: render_graph(workspace),
        CATALOG: lambda: render_catalog(workspace, problems),
    }
    out: Dict[str, Optional[str]] = {}
    for path, blocks in TARGETS.items():
        try:
            with open(os.path.join(root, path), encoding="utf-8") as handle:
                text: Optional[str] = handle.read()
        except FileNotFoundError:
            problems.append(Problem(path, "is missing"))
            out[path] = None
            continue
        for name in blocks:
            assert text is not None
            new = with_block(text, name, bodies[name]())
            if new is None:
                start, end = markers(name)
                problems.append(Problem(path, f"needs one `{start}` and, after it, one `{end}`"))
            text = new if new is not None else text
        out[path] = text
    return out, problems


def stale(root: str, workspace: Workspace) -> Tuple[List[str], List[Problem], Dict[str, Optional[str]]]:
    out, problems = rendered(root, workspace)
    changed = []
    for path, text in out.items():
        if text is None:
            continue
        with open(os.path.join(root, path), encoding="utf-8") as handle:
            if handle.read() != text:
                changed.append(path)
    return changed, problems, out


def write(root: str, workspace: Workspace) -> Tuple[List[str], List[Problem]]:
    changed, problems, out = stale(root, workspace)
    for path in changed:
        with open(os.path.join(root, path), "w", encoding="utf-8") as handle:
            handle.write(out[path] or "")
    return changed, problems


# ---------------------------------------------------------------------------
# Self-test

FIXTURE = {
    "Cargo.toml": """\
        [workspace]
        members = ["crates/gglib-top", "crates/gglib-side", "crates/gglib-a-long-name", "crates/gglib-base",
                   "crates/gglib-b", "crates/gglib-c", "crates/gglib-d", "crates/gglib-e", "src-tauri"]
        """,
    "crates/layers.toml": """\
        order = ["adapter", "empty", "middle", "bottom"]
        [crate.gglib-top]
        layer = "adapter"
        [crate.gglib-side]
        layer = "adapter"
        [crate.gglib-app]
        layer = "adapter"
        badges = false
        why = "No library code."
        [crate.gglib-a-long-name]
        layer = "middle"
        coverage = false
        why = "Needs a toolchain."
        [crate.gglib-b]
        layer = "middle"
        [crate.gglib-c]
        layer = "middle"
        [crate.gglib-d]
        layer = "middle"
        [crate.gglib-e]
        layer = "middle"
        [crate.gglib-base]
        layer = "bottom"
        """,
    "crates/gglib-top/Cargo.toml": """\
        [package]
        name = "gglib-top"
        description = "The top: a | in a description is escaped."
        [dependencies]
        gglib-base = { workspace = true }
        gglib-side = { workspace = true }
        gglib-a-long-name = { workspace = true }
        gglib-b = { workspace = true }
        gglib-c = { workspace = true }
        gglib-d = { workspace = true }
        gglib-e = { workspace = true }
        [dev-dependencies]
        gglib-app = { path = "../../src-tauri" }
        """,
    "crates/gglib-side/Cargo.toml": """\
        [package]
        name = "gglib-side"
        description = \"\"\"
            Spread over
            three lines.\"\"\"
        [build-dependencies]
        gglib-base = { workspace = true }
        """,
    "crates/gglib-a-long-name/Cargo.toml": """\
        [package]
        name = "gglib-a-long-name"
        description = "Long."
        [target.'cfg(unix)'.dependencies]
        gglib-base = { workspace = true }
        """,
    "crates/gglib-b/Cargo.toml": '[package]\nname = "gglib-b"\ndescription = "B."\n',
    "crates/gglib-c/Cargo.toml": '[package]\nname = "gglib-c"\ndescription = "C."\n',
    "crates/gglib-d/Cargo.toml": '[package]\nname = "gglib-d"\ndescription = "D."\n',
    "crates/gglib-e/Cargo.toml": '[package]\nname = "gglib-e"\n',
    "crates/gglib-base/Cargo.toml": '[package]\nname = "gglib-base"\ndescription = "The base."\n',
    "src-tauri/Cargo.toml": """\
        [package]
        name = "gglib-app"
        description = "The app."
        [dependencies]
        gglib-top = { path = "../crates/gglib-top" }
        """,
    "crates/README.md": """\
        # Crates

        ## Architecture Overview

        <!-- crate-graph:start -->
        stale
        <!-- crate-graph:end -->

        ## Catalog

        <!-- crate-catalog:start -->
        <!-- crate-catalog:end -->

        The rest is kept.
        """,
    # The end marker comes first: refused, and left as it is.
    "crates/gglib-core/README.md": """\
        # gglib-core

        <!-- crate-graph:end -->
        <!-- crate-graph:start -->
        """,
}

S = crate_list.SHIELD_URL
GRAPH_BLOCK = LEGEND + """

```text
ADAPTER ──────────────────────────────────────────────────────────
  side            src-tauri       top
MIDDLE ───────────────────────────────────────────────────────────
  a-long-name     b               c               d
  e
BOTTOM ───────────────────────────────────────────────────────────
  base

DEPENDS ON ───────────────────────────────────────────────────────
  side            → base
  src-tauri       → top
  top             → side, a-long-name, b, c, d, e, base
  a-long-name     → base
  b               → none
  c               → none
  d               → none
  e               → none
  base            → none
```"""
CATALOG_BLOCK = f"""\
| Crate | Layer | Description | Badges |
|-------|-------|-------------|--------|
| [gglib-side](gglib-side/) | adapter | Spread over three lines. | ![Tests]({S}gglib-side-tests.json) ![Coverage]({S}gglib-side-coverage.json) ![LOC]({S}gglib-side-loc.json) ![Complexity]({S}gglib-side-complexity.json) |
| [src-tauri](../src-tauri/) | adapter | The app. | — |
| [gglib-top](gglib-top/) | adapter | The top: a \\| in a description is escaped. | ![Tests]({S}gglib-top-tests.json) ![Coverage]({S}gglib-top-coverage.json) ![LOC]({S}gglib-top-loc.json) ![Complexity]({S}gglib-top-complexity.json) |
| [gglib-a-long-name](gglib-a-long-name/) | middle | Long. | ![Tests]({S}gglib-a-long-name-tests.json) ![LOC]({S}gglib-a-long-name-loc.json) ![Complexity]({S}gglib-a-long-name-complexity.json) |
| [gglib-b](gglib-b/) | middle | B. | ![Tests]({S}gglib-b-tests.json) ![Coverage]({S}gglib-b-coverage.json) ![LOC]({S}gglib-b-loc.json) ![Complexity]({S}gglib-b-complexity.json) |
| [gglib-c](gglib-c/) | middle | C. | ![Tests]({S}gglib-c-tests.json) ![Coverage]({S}gglib-c-coverage.json) ![LOC]({S}gglib-c-loc.json) ![Complexity]({S}gglib-c-complexity.json) |
| [gglib-d](gglib-d/) | middle | D. | ![Tests]({S}gglib-d-tests.json) ![Coverage]({S}gglib-d-coverage.json) ![LOC]({S}gglib-d-loc.json) ![Complexity]({S}gglib-d-complexity.json) |
| [gglib-e](gglib-e/) | middle |  | ![Tests]({S}gglib-e-tests.json) ![Coverage]({S}gglib-e-coverage.json) ![LOC]({S}gglib-e-loc.json) ![Complexity]({S}gglib-e-complexity.json) |
| [gglib-base](gglib-base/) | bottom | The base. | ![Tests]({S}gglib-base-tests.json) ![Coverage]({S}gglib-base-coverage.json) ![LOC]({S}gglib-base-loc.json) ![Complexity]({S}gglib-base-complexity.json) |"""
WRITTEN = f"""\
# Crates

## Architecture Overview

<!-- crate-graph:start -->
{GRAPH_BLOCK}
<!-- crate-graph:end -->

## Catalog

<!-- crate-catalog:start -->
{CATALOG_BLOCK}
<!-- crate-catalog:end -->

The rest is kept.
"""
EXPECTED_PROBLEMS = {
    "crates/gglib-e/Cargo.toml: has no `description`, which the catalogue shows",
    "crates/gglib-core/README.md: needs one `<!-- crate-graph:start -->` and, after it, one `<!-- crate-graph:end -->`",
}
# A dependency list longer than a line wraps under its own start.
WRAPPED = """\
  top             → side, a-long-name, b, c, d, e, base, base, base, base, base,
                    base, base"""


def self_test() -> bool:
    failures = []
    with tempfile.TemporaryDirectory() as root:
        for path, body in FIXTURE.items():
            os.makedirs(os.path.join(root, os.path.dirname(path)), exist_ok=True)
            with open(os.path.join(root, path), "w", encoding="utf-8") as handle:
                handle.write(textwrap.dedent(body))
        workspace = crate_list.load(root)
        if workspace.problems:
            failures.append(f"the fixture workspace is not clean: {[p.render() for p in workspace.problems]}")
        changed, problems = write(root, workspace)
        if changed != ["crates/README.md"]:
            failures.append(f"--write rewrote {changed}, expected only crates/README.md")
        if {p.render() for p in problems} != EXPECTED_PROBLEMS or len(problems) != len(EXPECTED_PROBLEMS):
            failures.append(f"reported {sorted(p.render() for p in problems)}, expected {sorted(EXPECTED_PROBLEMS)}")
        with open(os.path.join(root, "crates/README.md"), encoding="utf-8") as handle:
            got = handle.read()
        if got != WRITTEN:
            failures.append("--write left crates/README.md other than the pinned text:\n" + got)
        with open(os.path.join(root, "crates/gglib-core/README.md"), encoding="utf-8") as handle:
            if handle.read() != textwrap.dedent(FIXTURE["crates/gglib-core/README.md"]):
                failures.append("--write changed a README whose markers it refused")
        again, _, _ = stale(root, workspace)
        if again:
            failures.append(f"a second pass still finds {again} stale")
        top = next(c for c in workspace.crates if c.name == "gglib-top")
        many = workspace._replace(crates=tuple(
            c._replace(deps=c.deps + ("gglib-base",) * 6) if c is top else c for c in workspace.crates))
        if WRAPPED not in render_graph(many):
            failures.append("a long dependency list does not wrap as pinned")
    if not failures:
        print(f"✅ self-test: graph and catalogue as pinned, {len(EXPECTED_PROBLEMS)} planted faults found, "
              "a second pass finds nothing")
        return True
    print("❌ self-test failed: the generator no longer writes what the fixture pins")
    for failure in failures:
        print(f"     {failure}")
    return False


def check() -> bool:
    workspace = crate_list.load_strict(crate_list.REPO)
    changed, problems, _ = stale(crate_list.REPO, workspace)
    print(f"Checking the crate graph and catalogue ({len(workspace.crates)} crates, {len(TARGETS)} READMEs)...")
    for problem in problems:
        print(f"❌ {problem.render()}")
    for path in changed:
        print(f"❌ {path}: a generated block is out of date; run `make readmes`")
    if problems or changed:
        return False
    print("✅ the crate graph and catalogue match the manifests")
    return True


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true", help="rewrite the stale blocks")
    mode.add_argument("--check", action="store_true", help="self-test, then fail while a block is stale")
    mode.add_argument("--self-test", action="store_true", help="run the fixtures only")
    args = parser.parse_args()
    if args.write:
        changed, problems = write(crate_list.REPO, crate_list.load_strict(crate_list.REPO))
        for path in changed:
            print(f"wrote {path}")
        for problem in problems:
            print(f"❌ {problem.render()}")
        return 1 if problems else 0
    if not self_test():
        return 1
    return 0 if args.self_test or check() else 1


if __name__ == "__main__":
    sys.exit(main())
