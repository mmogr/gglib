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

# The first cell's code span, whether or not it is wrapped in a link. Both
# spellings are in use: `| [`useModels.ts`](useModels.ts) |` under src/hooks,
# and a bare `| `Stack.tsx` |` under src/components/primitives.
ROW = re.compile(r"^\|\s*(?:\[`([^`]+)`\]\([^)]*\)|`([^`]+)`)\s*\|")


def names_a_file(span: str) -> bool:
    """Whether a first-cell code span refers to a file or directory.

    The extension is what decides, and it is what keeps the two tables that
    are *not* file listings out of this check: `src/commands/README.md`
    tabulates CLI command names (`model`, `chat`, `q`) against anchors in the
    same document, and `src/types/README.md` tabulates TypeScript type names.
    Neither carries an extension, so neither is mistaken for a missing file.
    """
    return span.endswith(CODE_SUFFIXES) or span.endswith("/")


def exists_under(dirpath: str, name: str) -> bool:
    """Whether a row's name resolves anywhere in the README's subtree.

    Not just as a direct child: several tables list files relative to a
    subdirectory rather than to themselves. `src/services/README.md` has a
    "Clients" table of `benchmark.ts`, `proxyDashboard.ts` and eight more,
    every one of which lives in `clients/` — checking only the immediate
    directory called all ten missing.

    The looser test still catches what this is for: a row naming a file that
    was deleted or renamed, which then resolves nowhere.
    """
    if os.path.exists(os.path.join(dirpath, name)):
        return True
    target = name.rstrip("/")
    for _, subdirs, files in os.walk(dirpath):
        subdirs[:] = [d for d in subdirs if d not in SKIP_DIRS]
        if target in files or target in subdirs:
            return True
    return False


def documented_names(readme: str) -> tuple[set[str], str]:
    """File-naming rows, and the whole text — prose documents a file too."""
    rows = set()
    text = open(readme, encoding="utf-8", errors="replace").read()
    for line in text.splitlines():
        match = ROW.match(line)
        if not match:
            continue
        span = match.group(1) or match.group(2)
        if names_a_file(span):
            rows.add(span)
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
    """Everything in the directory a README is expected to account for.

    `index.ts` is excluded. A barrel's contents follow from the directory it
    sits in, so the row it would get says "barrel export" and nothing else —
    thirty-odd of those across the component READMEs would be noise, and a
    check that demands noise trains people to satisfy it rather than read it.
    A barrel that does something surprising can still be described in prose.
    """
    names = {f for f in files if f.endswith(CODE_SUFFIXES) and f != "index.ts"}
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

            phantom = sorted(n for n in rows if not exists_under(dirpath, n))
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
