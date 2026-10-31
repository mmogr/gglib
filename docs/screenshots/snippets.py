#!/usr/bin/env python3
"""Record the CLI reference's output blocks, and splice them into its pages.

Usage: python3 snippets.py [--splice | --check] [name...]

Each steps/cli/<name>.json is a record.py script that runs one command
against the demo copy (see README.md, "Terminal GIFs"). For each one,
record.py records it, render.mjs --text writes what the terminal showed to
../cli/snippets/<name>.txt, and that text is spliced into every page under
../cli/ between

    <!-- snippet: <name> -->
    <!-- /snippet -->

as a ```console block. Names on the command line limit the recording to
those snippets; the splice always covers every page.

Two renames are applied, as the GIFs rename this machine's user: $HOME
becomes /Users/mmogr, and the demo copy's folder becomes the folder gglib
uses by default on a Mac, so a page shows the paths a reader would see.

A recording that times out is named at the end and the others still run.
--splice records nothing and splices the .txt files already there, after a
page's prose has been edited. --check writes nothing and exits 1 when a
page's block differs from its .txt, a marker names a snippet with no .txt,
or a .txt is used by no page.
"""

import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
STEPS = HERE / "steps" / "cli"
CLI = HERE.parent / "cli"
SNIPPETS = CLI / "snippets"
MARKER = re.compile(r"(<!-- snippet: (\S+) -->\n)(.*?)(<!-- /snippet -->)", re.S)
HOME = os.path.expanduser("~")
RENAMES = [
    f"{HOME}=/Users/mmogr",
    "/Users/mmogr/.local/share/gglib-demo=/Users/mmogr/Library/Application Support/gglib",
]


def record(names):
    SNIPPETS.mkdir(parents=True, exist_ok=True)
    failed = []
    with tempfile.TemporaryDirectory(prefix="gglib-snippets-") as tmp:
        for name in names:
            steps = STEPS / f"{name}.json"
            if not steps.exists():
                sys.exit(f"no {steps}")
            cast = Path(tmp) / f"{name}.cast"
            if subprocess.run([sys.executable, HERE / "record.py", steps, cast], cwd=HERE).returncode:
                failed.append(name)
                continue
            render = ["node", HERE / "render.mjs", cast, "--text", SNIPPETS / f"{name}.txt"]
            for pair in RENAMES:
                render += ["--rename", pair]
            subprocess.run(render, check=True, cwd=HERE)
    if failed:
        sys.exit(f"not recorded: {' '.join(failed)}")


def spliced(page_text, used):
    def block(match):
        name = match.group(2)
        used.add(name)
        txt = SNIPPETS / f"{name}.txt"
        if not txt.exists():
            sys.exit(f"a page names snippet {name!r}, and there is no {txt}")
        return f"{match.group(1)}```console\n{txt.read_text()}```\n{match.group(4)}"

    return MARKER.sub(block, page_text)


def splice(check):
    used = set()
    stale = []
    for page in sorted(CLI.glob("*.md")):
        before = page.read_text()
        after = spliced(before, used)
        if after != before:
            if check:
                stale.append(page)
            else:
                page.write_text(after)
                print(f"spliced {page.relative_to(CLI.parent)}")
    unused = sorted(p.stem for p in SNIPPETS.glob("*.txt") if p.stem not in used)
    for name in unused:
        print(f"snippets/{name}.txt is used by no page", file=sys.stderr)
    for page in stale:
        print(f"{page.relative_to(CLI.parent)} differs from its snippets", file=sys.stderr)
    if check and (unused or stale):
        sys.exit(1)


def main(argv):
    mode = argv[0] if argv and argv[0].startswith("--") else None
    names = argv[1:] if mode else argv
    if mode not in (None, "--splice", "--check"):
        sys.exit(__doc__)
    if mode is None:
        record(names or sorted(p.stem for p in STEPS.glob("*.json")))
    splice(check=mode == "--check")


if __name__ == "__main__":
    main(sys.argv[1:])
