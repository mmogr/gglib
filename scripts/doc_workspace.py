#!/usr/bin/env python3
"""Document the workspace so that every link to another crate's items resolves.

`cargo doc --workspace --no-deps` renders a crate as soon as its dependencies'
metadata is ready, before their pages exist, and cargo empties a crate's doc
directory just before rendering it. rustdoc links to another crate's items
only when that crate's doc directory exists at the moment it renders, so on a
cold `target/doc` a link such as `[X](gglib_core::domain::X)` from a crate
rendered early is left as the literal path in the href, with no warning. A
warm cache hides this, since a crate that is not re-rendered keeps its pages.

This runs `cargo doc` once for everything, then once more per dependency
level, from the crates that depend on one workspace crate up to the crates
that depend on the most, each time deleting that level's doc directories first
so that only those crates render again, with every crate they link to already
on disk. Crates on one level never depend on each other.

    doc_workspace.py [-- <cargo doc arguments>]

Everything after `--` goes to each `cargo doc` call unchanged; `--exclude
<crate>` there also leaves that crate out of the levels. `RUSTDOCFLAGS` is
read by cargo as usual. `--self-test` runs the fixtures only.

Exit codes:
    0  every pass succeeded
    1  a `cargo doc` call or `cargo metadata` failed, or the self-test did
"""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Set


def levels(packages: list, excluded: Set[str]) -> List[List[str]]:
    """Workspace crates by dependency level, lowest first.

    A crate's level is one more than the highest level among the workspace
    crates it depends on normally (not for its tests or its build script),
    so each level's crates depend only on lower levels. Level 1, the crates
    that depend on no workspace crate, is left out: nothing they link to is
    rendered in the same run.
    """
    names = {p["name"] for p in packages} - excluded
    deps: Dict[str, Set[str]] = {
        p["name"]: {
            d["name"]
            for d in p["dependencies"]
            if d["name"] in names and d.get("kind") is None
        }
        for p in packages
        if p["name"] in names
    }
    level: Dict[str, int] = {}

    def of(name: str) -> int:
        if name not in level:
            level[name] = 1 + max((of(d) for d in deps[name]), default=0)
        return level[name]

    by_level: Dict[int, List[str]] = {}
    for name in names:
        by_level.setdefault(of(name), []).append(name)
    return [sorted(by_level[n]) for n in sorted(by_level) if n > 1]


def doc_dir(doc_root: Path, crate: str) -> Path:
    """Where rustdoc writes `crate`'s pages: its name with `-` as `_`."""
    return doc_root / crate.replace("-", "_")


def excluded_in(args: List[str]) -> Set[str]:
    """The crates `--exclude` names in cargo's arguments."""
    names = set()
    for i, arg in enumerate(args):
        if arg == "--exclude" and i + 1 < len(args):
            names.add(args[i + 1])
        elif arg.startswith("--exclude="):
            names.add(arg.split("=", 1)[1])
    return names


def cargo_doc(args: List[str]) -> None:
    subprocess.run(["cargo", "doc", "--workspace", "--no-deps", *args], check=True)


def main(args: List[str]) -> int:
    if args and args[0] == "--":
        args = args[1:]
    metadata = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )
    doc_root = Path(metadata["target_directory"]) / "doc"
    plan = levels(metadata["packages"], excluded_in(args))

    print("doc pass 1: the workspace")
    cargo_doc(args)
    for n, crates in enumerate(plan, start=2):
        print(f"doc pass {n}: {', '.join(crates)}")
        for crate in crates:
            shutil.rmtree(doc_dir(doc_root, crate), ignore_errors=True)
        cargo_doc(args)
    return 0


def self_test() -> int:
    def package(name: str, deps: Dict[str, str]) -> dict:
        return {
            "name": name,
            "dependencies": [{"name": d, "kind": k} for d, k in deps.items()],
        }

    packages = [
        package("core", {}),
        package("test-http", {}),
        package("db", {"core": None}),
        package("hf", {"core": None, "test-http": "dev"}),
        package("download", {"core": None, "hf": None}),
        package("cli", {"core": None, "download": None, "db": None}),
        package("app", {"cli": None}),
        package("serde", {}),  # not in the workspace: never in `packages`
    ][:-1]

    plan = levels(packages, {"app"})
    assert plan == [["db", "hf"], ["download"], ["cli"]], plan
    assert "app" not in sum(plan, []), "an excluded crate is never rendered"
    assert levels([package("alone", {})], set()) == [], "level 1 only: no pass"
    assert doc_dir(Path("t/doc"), "gglib-core") == Path("t/doc/gglib_core")
    assert excluded_in(["--exclude", "a", "--exclude=b", "--x"]) == {"a", "b"}
    print("✅ self-test: 4 levels planned in order, exclusions and names honoured")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        sys.exit(self_test())
    try:
        sys.exit(main(sys.argv[1:]))
    except subprocess.CalledProcessError as error:
        print(f"❌ {' '.join(error.cmd)} exited {error.returncode}", file=sys.stderr)
        sys.exit(1)
