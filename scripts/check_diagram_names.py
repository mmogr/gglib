#!/usr/bin/env python3
"""Check that every type and file a crate README's diagram names still exists.

The diagrams in the crate READMEs are drawn by hand, inside ```text fences,
and nothing compiles them: a type renamed in the code stays in the box that
names it until someone reads the box. This reads the ```text fences in
`crates/*/README.md` and `src-tauri/README.md` and fails when:

1. a CamelCase word (two or more capitalised humps, such as `ModelRepository`)
   is not a word anywhere in the Rust sources under `crates/` or
   `src-tauri/src/`, comments and strings included;
2. a `name.rs` word names no file of that name anywhere under `crates/` or
   `src-tauri/`;
3. either kind of word is cut short: followed by `…`, or by a `.` that does
   not start a field or method name (`NormalizingSt.` fails, `Library.serve`
   does not);
4. a row of scripts/diagram-names-allowlist.txt excuses nothing the scan
   found.

A word that fails but is right, such as the name of a type in another
repository, goes in scripts/diagram-names-allowlist.txt as
`<crate directory><TAB><word><TAB><why>`, where the crate directory is the
README's (`gglib-db`, `src-tauri`).

What it does NOT check, so that nobody reads more into a pass than is there:

* That a box names the right type, or that an arrow between boxes is a real
  dependency. A word that exists anywhere passes, wherever it is defined, and
  so does a file name: `mod.rs` always passes, and `servers.rs` passes for
  either of the two files of that name.
* Lower-case names other than `.rs` files: a process (`llama-cli`), a module
  directory (`ports_impl/`), a crate. Nor a CamelCase word with one hump,
  such as `Library`, or an all-capitals one, such as `GGUF`.
* Fences with another info string, or none, and diagrams anywhere but the
  crate-level READMEs.

Usage:
    ./scripts/check_diagram_names.py --check      # self-test, then scan
    ./scripts/check_diagram_names.py --self-test  # run the fixtures only

Exit codes:
    0  every name was found
    1  one was not, an allowlist row is stale or malformed, the self-test
       failed, or nothing was read
"""

from __future__ import annotations

import argparse
import glob
import os
import re
import sys
import tempfile
import textwrap
from typing import Dict, List, NamedTuple, Set, Tuple

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ALLOWLIST = "scripts/diagram-names-allowlist.txt"
READMES = ("crates/*/README.md", "src-tauri/README.md")
SOURCE_ROOTS = ("crates", "src-tauri/src")
FILE_ROOTS = ("crates", "src-tauri")
SKIP_DIRS = {"target", "node_modules", ".git"}

FENCE_OPEN = re.compile(r"^\s*```text\s*$")
FENCE_CLOSE = re.compile(r"^\s*```\s*$")
CAMEL = re.compile(r"\b[A-Z][a-z0-9]+(?:[A-Z][a-z0-9]+)+\b")
RUST_FILE = re.compile(r"\b[a-z][a-z0-9_-]*\.rs\b")
# A word cut short: `…`, or a `.` that no identifier follows.
CUT = re.compile(r"…|\.(?![A-Za-z_])")
WORD = re.compile(r"\b\w+\b")


class Problem(NamedTuple):
    path: str
    line: int
    kind: str
    token: str

    def render(self) -> str:
        why = {
            "missing": "is not a word in any Rust source under crates/ or src-tauri/src/",
            "no-file": "names no file under crates/ or src-tauri/",
            "cut": "is cut short; write the name out",
            "stale": "is allowlisted but the scan found nothing to excuse",
            "malformed": "is not `<crate directory><TAB><word><TAB><why>`",
        }[self.kind]
        return f"{self.path}:{self.line}: `{self.token}` {why}"


def walk_files(root: str) -> List[str]:
    found = []
    for directory, dirs, files in os.walk(root):
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
        found += [os.path.join(directory, name) for name in files]
    return found


def source_words(root: str) -> Set[str]:
    words: Set[str] = set()
    for base in SOURCE_ROOTS:
        for path in walk_files(os.path.join(root, base)):
            if path.endswith(".rs"):
                with open(path, encoding="utf-8", errors="replace") as handle:
                    words.update(WORD.findall(handle.read()))
    return words


def file_names(root: str) -> Set[str]:
    return {os.path.basename(p) for base in FILE_ROOTS for p in walk_files(os.path.join(root, base))}


def read_allowlist(root: str) -> Tuple[Dict[Tuple[str, str], int], List[Problem]]:
    rows: Dict[Tuple[str, str], int] = {}
    problems = []
    path = os.path.join(root, ALLOWLIST)
    if not os.path.exists(path):
        return rows, problems
    with open(path, encoding="utf-8") as handle:
        for number, line in enumerate(handle, 1):
            line = line.rstrip("\n")
            if not line.strip() or line.startswith("#"):
                continue
            fields = line.split("\t")
            if len(fields) != 3 or not all(f.strip() for f in fields):
                problems.append(Problem(ALLOWLIST, number, "malformed", line))
                continue
            rows[(fields[0], fields[1])] = number
    return rows, problems


def text_fences(text: str) -> List[Tuple[int, str]]:
    """Each line inside a ```text fence, with its line number."""
    lines, inside = [], False
    for number, line in enumerate(text.split("\n"), 1):
        if inside:
            if FENCE_CLOSE.match(line):
                inside = False
            else:
                lines.append((number, line))
        elif FENCE_OPEN.match(line):
            inside = True
    return lines


class Scan(NamedTuple):
    problems: List[Problem]
    fences_lines: int
    tokens: int


def scan(root: str) -> Scan:
    words, files = source_words(root), file_names(root)
    allowed, problems = read_allowlist(root)
    used: Set[Tuple[str, str]] = set()
    readmes = sorted(p for pattern in READMES for p in glob.glob(os.path.join(root, pattern)))
    fence_lines = tokens = 0
    for full in readmes:
        rel = os.path.relpath(full, root)
        crate = os.path.basename(os.path.dirname(full))
        with open(full, encoding="utf-8") as handle:
            lines = text_fences(handle.read())
        fence_lines += len(lines)
        for number, line in lines:
            for pattern, exists in ((CAMEL, lambda t: t in words), (RUST_FILE, lambda t: t in files)):
                for match in pattern.finditer(line):
                    tokens += 1
                    token = match.group(0)
                    if CUT.match(line, match.end()):
                        kind = "cut"
                    elif not exists(token):
                        kind = "missing" if pattern is CAMEL else "no-file"
                    else:
                        continue
                    if (crate, token) in allowed:
                        used.add((crate, token))
                        continue
                    problems.append(Problem(rel, number, kind, token))
    for key, number in allowed.items():
        if key not in used:
            problems.append(Problem(ALLOWLIST, number, "stale", "\t".join(key)))
    return Scan(problems, fence_lines, tokens)


# ---------------------------------------------------------------------------
# Self-test

FIXTURE = {
    "crates/gglib-a/src/lib.rs": """\
        pub struct ModelRepository;
        // HubClientPort is named only in this comment.
        pub fn serve() {}
        """,
    "crates/gglib-a/src/ports/mod.rs": "",
    "crates/gglib-b/src/lib.rs": "pub struct OtherCrateType;\n",
    "src-tauri/src/main.rs": "struct DaemonSnapshot;\n",
    # A type defined only under a target/ directory is not a source.
    "crates/gglib-a/target/gen.rs": "struct BuildOutputType;\n",
    "crates/gglib-a/README.md": """\
        # gglib-a

        Prose names GhostType and ghost.rs, and is not read.

        ```text
        ┌──────────────────┐   ┌──────────────┐
        │ ModelRepository  │──►│ GhostType    │
        │ OtherCrateType   │   │ HubClientPort│
        │ NormalizingSt.   │   │ ModelRepos…  │
        │ ModelRepository.serve()              │
        │ lib.rs  mod.rs   │   │ ghost.rs     │
        │ BuildOutputType  │   │ ExcusedType  │
        │ main.rs          │   │ lib.rs.      │
        └──────────────────┘   └──────────────┘
        ```

        ```rust
        let x = GhostType::new();
        ```
        """,
    "src-tauri/README.md": """\
        # The app

          ```text
          │ DaemonSnapshot │ ModelRepository │ PhantomWindow │
          ```
        """,
    ALLOWLIST: (
        "# crate<TAB>word<TAB>why\n"
        "gglib-a\tExcusedType\tDefined in another repository.\n"
        "gglib-a\tUnusedType\tNothing names it any more.\n"
        "gglib-a\tno why here\n"
    ),
}

EXPECTED = {
    ("crates/gglib-a/README.md", 7, "missing", "GhostType"),
    ("crates/gglib-a/README.md", 9, "cut", "NormalizingSt"),
    ("crates/gglib-a/README.md", 9, "cut", "ModelRepos"),
    ("crates/gglib-a/README.md", 11, "no-file", "ghost.rs"),
    ("crates/gglib-a/README.md", 12, "missing", "BuildOutputType"),
    ("crates/gglib-a/README.md", 13, "cut", "lib.rs"),
    ("src-tauri/README.md", 4, "missing", "PhantomWindow"),
    (ALLOWLIST, 3, "stale", "gglib-a\tUnusedType"),
    (ALLOWLIST, 4, "malformed", "gglib-a\tno why here"),
}


def self_test() -> bool:
    with tempfile.TemporaryDirectory() as root:
        for path, body in FIXTURE.items():
            os.makedirs(os.path.join(root, os.path.dirname(path)), exist_ok=True)
            with open(os.path.join(root, path), "w", encoding="utf-8") as handle:
                handle.write(textwrap.dedent(body) if path.endswith(".md") or path.endswith(".rs") else body)
        result = scan(root)
    found = {(p.path, p.line, p.kind, p.token) for p in result.problems}
    if found == EXPECTED and len(result.problems) == len(EXPECTED):
        print(f"✅ self-test: {len(EXPECTED)} planted faults found, nothing else")
        return True
    print("❌ self-test failed: the checker no longer reports what the fixture plants")
    for item in sorted(EXPECTED - found):
        print(f"     not reported: {item}")
    for problem in result.problems:
        if (problem.path, problem.line, problem.kind, problem.token) not in EXPECTED:
            print(f"     reported but not planted: {problem.render()}")
    return False


def check() -> bool:
    result = scan(REPO)
    print(f"Checking the names in the crate READMEs' diagrams ({result.fences_lines} diagram lines, "
          f"{result.tokens} names read)...")
    # Liveness: a scan that read nothing prints what a clean scan prints.
    if not result.fences_lines or not result.tokens:
        print("❌ no ```text fence or no name in one was found; nothing was checked")
        return False
    if not result.problems:
        print("✅ every type and file the diagrams name exists")
        return True
    for problem in result.problems:
        print(f"❌ {problem.render()}")
    print(f"\n❌ {len(result.problems)} problem(s). Write the name as the code spells it now,")
    print(f"   or, for a name that is right but not in this repository, add a row to {ALLOWLIST}.")
    return False


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true", help="self-test, then scan the repository")
    mode.add_argument("--self-test", action="store_true", help="run the fixtures only")
    args = parser.parse_args()
    if not self_test():
        return 1
    if args.self_test:
        return 0
    return 0 if check() else 1


if __name__ == "__main__":
    sys.exit(main())
