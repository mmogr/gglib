#!/usr/bin/env python3
"""Print what moved between two copies of a lockfile, one line per change.

    python3 scripts/lock_changes.py cargo OLD_Cargo.lock NEW_Cargo.lock
    python3 scripts/lock_changes.py npm OLD_package-lock.json NEW_package-lock.json

.github/workflows/update-deps.yml writes its commit message and pull request
body from this. A package whose version changed prints as `name old -> new`,
with `(none)` on the side where it is absent, and every version that left or
arrived when more than one did.

For cargo it then prints each package whose own version did not change but
which now builds against a different copy of a dependency, as
`package version: dependency old -> new`. `cargo update -p` rewrites those
entries even for packages unrelated to the one named (#1112), and a reviewer
reading only the version changes would miss them. A dependency edge that only
follows that dependency's own version change is not printed twice. The
workflow tells the two kinds of line apart by that colon.

Prints nothing when nothing moved.
"""

import json
import sys
import tomllib


def by_name(entries):
    """{name: {version, ...}} for a list of (name, version) pairs."""
    out = {}
    for name, version in entries:
        out.setdefault(name, set()).add(version)
    return out


def version_changes(old, new):
    lines = []
    for name in sorted(old.keys() | new.keys()):
        gone = sorted(old.get(name, set()) - new.get(name, set()))
        came = sorted(new.get(name, set()) - old.get(name, set()))
        if gone or came:
            lines.append(f"{name} {', '.join(gone) or '(none)'} -> {', '.join(came) or '(none)'}")
    return lines


def cargo_lock(path):
    with open(path, "rb") as f:
        packages = tomllib.load(f).get("package", [])
    return {(p["name"], p["version"]): p.get("dependencies", []) for p in packages}


def cargo_edges(dependencies, versions):
    """{name: {version, ...}} for one package's dependency list.

    Cargo.lock writes a dependency as its bare name when the lock holds one
    version of it, and as `name version` or `name version (source)` otherwise.
    """
    out = {}
    for dep in dependencies:
        name, *rest = dep.split(" ")
        version = rest[0] if rest else next(iter(versions.get(name, {"?"})))
        out.setdefault(name, set()).add(version)
    return out


def cargo_changes(old_path, new_path):
    old, new = cargo_lock(old_path), cargo_lock(new_path)
    old_versions, new_versions = by_name(old), by_name(new)
    lines = version_changes(old_versions, new_versions)
    removed, added = old.keys() - new.keys(), new.keys() - old.keys()
    for key in sorted(old.keys() & new.keys()):
        before = cargo_edges(old[key], old_versions)
        after = cargo_edges(new[key], new_versions)
        moves = []
        for dep in sorted(before.keys() | after.keys()):
            gone = before.get(dep, set()) - after.get(dep, set())
            came = after.get(dep, set()) - before.get(dep, set())
            followed = (gone and came
                        and all((dep, v) in removed for v in gone)
                        and all((dep, v) in added for v in came))
            if (gone or came) and not followed:
                moves.append(f"{dep} {', '.join(sorted(gone)) or '(none)'}"
                             f" -> {', '.join(sorted(came)) or '(none)'}")
        if moves:
            lines.append(f"{key[0]} {key[1]}: {'; '.join(moves)}")
    return lines


def npm_versions(path):
    with open(path, encoding="utf-8") as f:
        packages = json.load(f).get("packages", {})
    # The key is the install path; the root project is "". A nested copy
    # counts under its own name, so moving where a copy is installed is not a
    # change, while a second version of a package is.
    return by_name((key.rsplit("node_modules/", 1)[-1], meta["version"])
                   for key, meta in packages.items() if key and "version" in meta)


def main(argv):
    if len(argv) != 4 or argv[1] not in ("cargo", "npm"):
        sys.exit(f"usage: {argv[0]} cargo|npm OLD_LOCK NEW_LOCK")
    if argv[1] == "cargo":
        lines = cargo_changes(argv[2], argv[3])
    else:
        lines = version_changes(npm_versions(argv[2]), npm_versions(argv[3]))
    for line in lines:
        print(line)


if __name__ == "__main__":
    main(sys.argv)
