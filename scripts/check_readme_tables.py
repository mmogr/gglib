#!/usr/bin/env python3
"""Check that TypeScript README tables and the directories they describe agree.

`check_readmes.sh` already asks whether a README exists and carries its
markers. Nothing asked whether what it says is true, so a table could name a
file that had been deleted, or omit one that had been added, and stay green.

A first-cell code span names a file when it carries a code extension or a
trailing slash. That is what keeps the two tables which are *not* file
listings out of it: `src/commands/README.md` tabulates CLI command names and
`src/types/README.md` tabulates type names, and neither carries an extension.

What this does NOT check, so that nobody reads more into a pass than is
there:

* Only the *first* cell of a row. A second cell listing the symbols a module
  exports can name one that has been deleted and this will not notice.
* Rows are pooled per README, not per table. A README often carries tables for
  its subdirectories as well as itself, and nothing in a bare `| `detect.ts` |`
  says which one it belongs to — so adding `services/detect.ts` alongside the
  existing `services/platform/detect.ts` passes on the strength of the row for
  the other file. Attributing rows to tables would need each table's base
  directory declared, which no README does today.
* The *contents* of a README with no file-naming rows. Those directories are
  not compared — but which ones they are is now recorded in
  `scripts/readme-table-skip-baseline.txt`, and a new one fails. Reporting the
  count was not enough: the number could only go up, and nothing compared it,
  so deleting a table moved a README out of the checked set and printed a
  larger skip count above a green tick.

Usage:
    ./scripts/check_readme_tables.py            # report and fail on drift
    ./scripts/check_readme_tables.py --verbose  # also list what checked out
    ./scripts/check_readme_tables.py --update   # re-record the skip baseline

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


def prose_of(text: str) -> str:
    """The README with its table rows removed.

    The prose escape below has to read this rather than the whole document.
    Against the full text, a row in *another* table — `src/services/README.md`
    lists `clients/` and `platform/` contents in tables of their own — counted
    as documentation for a same-named file in this directory, so a genuinely
    undocumented file could pass on the strength of an unrelated row.
    """
    return "\n".join(line for line in text.splitlines() if not line.lstrip().startswith("|"))


def is_documented(name: str, rows: set[str], prose: str) -> bool:
    """A table row is the usual way; a code span in the prose also counts.

    `src/pages/README.md` is why the second half exists: it names
    `chatTabs.tsx` in a sentence explaining that it is a supporting piece and
    deliberately *not* a routed page. Demanding a row would have moved it into
    a table of pages it does not belong in — making the document less accurate
    to satisfy the check.
    """
    if name in rows:
        return True
    return f"`{name}`" in prose or f"`{name.rstrip('/')}`" in prose


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


BASELINE = os.path.join(os.path.dirname(__file__), "readme-table-skip-baseline.txt")

HEADER = """\
# READMEs that carry no file-naming rows, and so are not compared against
# their directory. Regenerate with `./scripts/check_readme_tables.py --update`.
#
# A new entry appearing here without being recorded is a failure: deleting a
# table, or dropping the extensions from its first cells, would otherwise move
# a README out of the checked set and print a larger skip count above a green
# tick. Recording one is a reviewable line in a diff, which is the same
# bargain `scripts/rust-complexity-baseline.txt` strikes.
#
# Three are exemptions by design — `src/types`, `src/commands` and
# `src/styles` tabulate type names, command names and migration phases rather
# than files. The rest are stubs from `generate_submodule_readmes.sh`, which
# emits TypeScript READMEs with no table at all; each is a directory nothing
# compares, and shortening this list is the way to fix that.
"""


def read_baseline() -> set[str] | None:
    """The exemptions recorded as deliberate. `None` when the file is absent."""
    if not os.path.exists(BASELINE):
        return None
    with open(BASELINE, encoding="utf-8") as handle:
        return {line.strip() for line in handle if line.strip() and not line.startswith("#")}


def main() -> int:
    verbose = "--verbose" in sys.argv
    update = "--update" in sys.argv
    problems: list[tuple[str, list[str], list[str]]] = []
    skipped: list[str] = []
    checked = 0

    for root in ROOTS:
        for dirpath, subdirs, files in os.walk(root):
            subdirs[:] = [d for d in subdirs if d not in SKIP_DIRS]
            if "README.md" not in files:
                continue

            readme = os.path.join(dirpath, "README.md")
            rows, text = documented_names(readme)
            if not rows:
                # No file-naming rows, so there is no table to compare. Counted
                # rather than passed over in silence: a reader of a green run
                # should be able to see how much of the tree it did not reach.
                skipped.append(readme)
                continue

            checked += 1
            present = present_names(dirpath, subdirs, files)
            prose = prose_of(text)

            phantom = sorted(n for n in rows if not exists_under(dirpath, n))
            unlisted = sorted(n for n in present if not is_documented(n, rows, prose))

            if phantom or unlisted:
                problems.append((readme, phantom, unlisted))
            elif verbose:
                print(f"  ok  {readme} ({len(rows)} rows)")

    skipped.sort()

    if update:
        with open(BASELINE, "w", encoding="utf-8") as handle:
            handle.write(HEADER)
            for readme in skipped:
                handle.write(f"{readme}\n")
        print(f"✅ baseline updated: {len(skipped)} README(s) carry no file-naming rows")
        return 0

    print(f"Checking README tables against their directories ({checked} with tables)...")
    print(f"  ({len(skipped)} README(s) carry no file-naming rows and are not compared)")
    print("================================================")
    if verbose:
        for readme in skipped:
            print(f"  skip {readme} (no file rows)")

    # Liveness: a walk that compared nothing prints exactly what a clean run
    # prints, which is the shape `check_file_complexity.sh` guards against for
    # the same reason.
    if checked == 0:
        print("❌ no README with file-naming rows was found — nothing was compared.")
        return 1

    baseline = read_baseline()
    if baseline is None:
        print(f"❌ missing baseline: {BASELINE} (run with --update to create it)")
        return 1

    # The skip list is a ratchet, not a free pass. Deleting a table — or
    # dropping the extensions from its first cells — used to move a README
    # from the checked set to the skipped set and print a *larger* number
    # above a green tick. The count was reported but never compared, so
    # coverage could only shrink, and `generate_submodule_readmes.sh` emits
    # table-less TypeScript stubs, so it shrank without anyone choosing it.
    new_skips = sorted(set(skipped) - baseline)
    recovered = sorted(baseline - set(skipped))

    for readme in recovered:
        print(f"  ✓ {readme} now carries a table (run --update to drop it from the baseline)")

    if not problems and not new_skips:
        print("✅ every table names what is there, and nothing that is not")
        return 0

    for readme in new_skips:
        print(f"\n❌ {readme}")
        print("     carries no file-naming rows, so its directory was not compared.")
        print("     Add a table, or run --update to record the exemption deliberately.")

    for readme, phantom, unlisted in problems:
        print(f"\n❌ {readme}")
        for name in phantom:
            print(f"     names `{name}`, which does not exist")
        for name in unlisted:
            print(f"     does not name `{name}`, which does")

    if problems:
        print("\n❌ README tables disagree with their directories.")
        print("   Update the table, or remove the row for a file that is gone.")
    else:
        print("\n❌ README table coverage shrank.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
