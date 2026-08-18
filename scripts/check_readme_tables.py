#!/usr/bin/env python3
"""Check that TypeScript README tables and the directories they describe agree.

`check_readmes.sh` already asks whether a README exists and carries its
markers. Nothing asked whether what it says is true, so a table could name a
file that had been deleted, or omit one that had been added, and stay green.

A row counts as describing a file only when its link target is a *relative
path*: `| [`useModels.ts`](useModels.ts) |` does, `| [`model`](#models) |`
does not. That distinction matters — `src/commands/README.md` tabulates CLI
command names against anchors in the same document, and every one of them
would otherwise read as a file that does not exist.

Usage:
    ./scripts/check_readme_tables.py            # report and fail on drift
    ./scripts/check_readme_tables.py --verbose  # also list what checked out

Exit codes:
    0  every table agrees with its directory
    1  at least one row names something absent, or one file is unnamed
"""

from __future__ import annotations

import os
import re
import sys

ROOTS = ("src", "tests")
CODE_SUFFIXES = (".ts", ".tsx", ".css")

# Directories whose contents are not hand-documented.
SKIP_DIRS = {"node_modules", "generated", "__snapshots__"}

# `| [`name`](target) |` — the target decides whether this is a file reference.
ROW = re.compile(r"^\|\s*\[`([^`]+)`\]\(([^)]+)\)")


def is_path_target(target: str) -> bool:
    """A relative path, as opposed to an anchor or an external URL."""
    if target.startswith("#") or target.startswith("/"):
        return False
    return "://" not in target


def documented_names(readme: str) -> tuple[set[str], str]:
    """Row targets, and the whole text — a file can be documented in prose."""
    rows = set()
    text = open(readme, encoding="utf-8", errors="replace").read()
    for line in text.splitlines():
        match = ROW.match(line)
        if match and is_path_target(match.group(2)):
            rows.add(match.group(2))
    return rows, text


def is_documented(name: str, rows: set[str], text: str) -> bool:
    """A table row is the usual way; a code span in the prose also counts.

    `src/pages/README.md` is why the second half exists: it names
    `chatTabs.tsx` in a sentence explaining that it is a supporting piece and
    deliberately *not* a routed page. Demanding a row would have moved it into
    a table of pages it does not belong in — making the document less accurate
    to satisfy the check.
    """
    if name in rows:
        return True
    return f"`{name}`" in text or f"`{name.rstrip('/')}`" in text


def present_names(dirpath: str, subdirs: list[str], files: list[str]) -> set[str]:
    names = {f for f in files if f.endswith(CODE_SUFFIXES)}
    names |= {d + "/" for d in subdirs}
    return names


def main() -> int:
    verbose = "--verbose" in sys.argv
    problems: list[tuple[str, list[str], list[str]]] = []
    checked = 0

    for root in ROOTS:
        for dirpath, subdirs, files in os.walk(root):
            subdirs[:] = [d for d in subdirs if d not in SKIP_DIRS]
            if "README.md" not in files:
                continue

            readme = os.path.join(dirpath, "README.md")
            rows, text = documented_names(readme)
            if not rows:
                continue  # a README with no file table is not this check's business

            checked += 1
            present = present_names(dirpath, subdirs, files)

            phantom = sorted(n for n in rows if not os.path.exists(os.path.join(dirpath, n)))
            unlisted = sorted(n for n in present if not is_documented(n, rows, text))

            if phantom or unlisted:
                problems.append((readme, phantom, unlisted))
            elif verbose:
                print(f"  ok  {readme} ({len(named)} rows)")

    print(f"Checking README tables against their directories ({checked} with tables)...")
    print("================================================")

    if not problems:
        print("✅ every table names what is there, and nothing that is not")
        return 0

    for readme, phantom, unlisted in problems:
        print(f"\n❌ {readme}")
        for name in phantom:
            print(f"     names `{name}`, which does not exist")
        for name in unlisted:
            print(f"     does not name `{name}`, which does")

    print("\n❌ README tables disagree with their directories.")
    print("   Update the table, or remove the row for a file that is gone.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
