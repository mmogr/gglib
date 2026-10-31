#!/usr/bin/env python3
"""Check that every link on a gglib crate's rustdoc front page goes somewhere.

A crate's README is its rustdoc front page (`target/doc/gglib_<crate>/index.html`),
and a relative link that works on GitHub breaks there. rustdoc does not warn
about a broken Markdown link, so `-D warnings` passes with one in place. Run
after `cargo doc`, this reads every `href` and `src` attribute on those pages
and fails when one is not:

* an `http:` or `https:` URL, a `mailto:` address, or a bare `#fragment`;
* a path that, with its fragment and query set aside and its `%`-escapes and
  HTML entities decoded, names a file inside the doc directory. A directory
  counts when it holds an `index.html`.

    check_doc_links.py target/doc   check the front pages under target/doc
    check_doc_links.py --self-test  run the fixtures only

What it does NOT check, so that nobody reads more into a pass than is there:

* That a URL answers, or that a `#fragment` names an anchor. It is offline.
* Pages other than each crate's `index.html`: a module page built from a
  module's README is not read.
* Links rustdoc writes into its own JavaScript rather than into the page.

Exit codes:
    0  every link resolves
    1  one does not, no front page was found, or the self-test failed
"""

from __future__ import annotations

import argparse
import glob
import html
import os
import re
import sys
import tempfile
from typing import List, NamedTuple, Tuple
from urllib.parse import unquote

ATTRIBUTE = re.compile(r"""\s(?:href|src)\s*=\s*(?:"([^"]*)"|'([^']*)')""", re.I)
ALLOWED = re.compile(r"^(?:https?:|mailto:|#)", re.I)
PAGES = "gglib_*/index.html"


class Problem(NamedTuple):
    page: str
    target: str

    def render(self) -> str:
        return f"{self.page}: `{self.target}` names nothing under the doc directory"


def resolves(doc: str, page: str, target: str) -> bool:
    path = html.unescape(target).split("#", 1)[0].split("?", 1)[0]
    if not path:
        return True  # `?query` alone, or an empty href: the page itself
    full = os.path.normpath(os.path.join(os.path.dirname(page), unquote(path)))
    root = os.path.normpath(doc)
    if os.path.commonpath([root, full]) != root:
        return False
    return os.path.isfile(full) or os.path.isfile(os.path.join(full, "index.html"))


def check_pages(doc: str) -> Tuple[List[Problem], int, int]:
    pages = sorted(glob.glob(os.path.join(doc, PAGES)))
    problems, read = [], 0
    for page in pages:
        with open(page, encoding="utf-8") as handle:
            text = handle.read()
        for match in ATTRIBUTE.finditer(text):
            target = match.group(1) if match.group(1) is not None else match.group(2)
            read += 1
            if ALLOWED.match(target.strip()) or resolves(doc, page, target.strip()):
                continue
            problems.append(Problem(os.path.relpath(page, doc), target))
    return problems, len(pages), read


# ---------------------------------------------------------------------------
# Self-test

FIXTURE = {
    "static.files/main.css": "",
    "gglib_a/sub/index.html": "",
    "gglib_a/x.html": "",
    "gglib_a/a b.html": "",
    "gglib_b/index.html": "",
    "gglib_a/index.html": """
        <link rel="stylesheet" href="../static.files/main.css">
        <script src='../static.files/main.css'></script>
        <a href="https://github.com/o/r/blob/main/README.md">ok</a>
        <a href="http://example.com">ok</a> <a href="mailto:me@example.com">ok</a>
        <a href="#heading">ok</a> <a href="x.html#frag">ok</a> <a href="x.html?search=q">ok</a>
        <a href="sub/">ok</a> <a href="./sub">ok</a> <a href="a%20b.html">ok</a>
        <a href="x.html&#35;frag">ok</a> <a href="../gglib_b/index.html">ok</a>
        <a HREF="src/lib.rs">broken</a>
        <a href="../README.md#architecture">broken</a>
        <a href="../../outside.html">broken</a>
        <img src="docs/i.png">
        <a href="missing/">broken</a>
        """,
    # Not a front page: not read.
    "gglib_a/sub/page.html": '<a href="nowhere.html">x</a>',
    "other/index.html": '<a href="nowhere.html">x</a>',
}

EXPECTED = {
    ("gglib_a/index.html", "src/lib.rs"),
    ("gglib_a/index.html", "../README.md#architecture"),
    ("gglib_a/index.html", "../../outside.html"),
    ("gglib_a/index.html", "docs/i.png"),
    ("gglib_a/index.html", "missing/"),
}


def self_test() -> bool:
    with tempfile.TemporaryDirectory() as root:
        doc = os.path.join(root, "doc")
        for path, body in FIXTURE.items():
            os.makedirs(os.path.join(doc, os.path.dirname(path)), exist_ok=True)
            with open(os.path.join(doc, path), "w", encoding="utf-8") as handle:
                handle.write(body)
        with open(os.path.join(root, "outside.html"), "w", encoding="utf-8") as handle:
            handle.write("")
        problems, pages, read = check_pages(doc)
    found = {(p.page, p.target) for p in problems}
    if found == EXPECTED and len(problems) == len(EXPECTED) and pages == 2 and read == 18:
        print(f"✅ self-test: {len(EXPECTED)} planted broken links found among {read}, nothing else")
        return True
    print("❌ self-test failed: the checker no longer reports what the fixture plants")
    print(f"     {pages} pages and {read} links read; the fixture has 2 and 18")
    for item in sorted(EXPECTED - found):
        print(f"     not reported: {item}")
    for problem in problems:
        if (problem.page, problem.target) not in EXPECTED:
            print(f"     reported but not planted: {problem.render()}")
    return False


def check(doc: str) -> bool:
    problems, pages, read = check_pages(doc)
    print(f"Checking the links on the crates' rustdoc front pages ({pages} pages, {read} links read)...")
    # Liveness: a check that read nothing prints what a clean one prints.
    if not pages or not read:
        print(f"❌ no {PAGES} with a link was found under {doc}; run `cargo doc` first")
        return False
    if not problems:
        print("✅ every link on the front pages resolves")
        return True
    for problem in problems:
        print(f"❌ {problem.render()}")
    print(f"\n❌ {len(problems)} broken link(s). A crate README is its rustdoc front page:")
    print("   crates/build_common.rs points a relative path at the repository, so check")
    print("   that the path is one it rewrites, or link a URL.")
    return False


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("doc", nargs="?", help="the rustdoc output directory, such as target/doc")
    parser.add_argument("--self-test", action="store_true", help="run the fixtures only")
    args = parser.parse_args()
    if args.self_test == bool(args.doc):
        parser.error("give the doc directory or --self-test")
    if not self_test():
        return 1
    return 0 if args.self_test or check(args.doc) else 1


if __name__ == "__main__":
    sys.exit(main())
