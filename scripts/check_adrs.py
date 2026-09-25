#!/usr/bin/env python3
"""Check that every link into docs/adr/ resolves, and every ADR reference is defined.

Over the files git tracks, this checks:

1. A relative link in a Markdown file that points into `docs/adr/`, or whose
   path has an `adr` segment: an inline `[text](target)` or image, a reference
   definition `[label]: target`, and an HTML `href`. The target must be a
   tracked file or a directory holding one, and a `#fragment` must name a
   heading's id in it or an HTML `id`/`name` anchor.
2. A `https://github.com/mmogr/gglib/blob/main/docs/adr/...` URL in any
   tracked text file, fragment included. URLs into other repositories, such
   as modelpipe's ADRs, are not read.
3. A bare `docs/adr/NNNN-name.md` or `docs/adr/log-NNNN.md` mention in any
   tracked text file, resolved from the repository root, fragment included.
4. Inside `docs/adr/*.md`, the `log-*.md` files included, every reference
   link (`[text][label]`, `[text][]`, `[label]`) has its `[label]: ...`
   definition in the same file.

What it does NOT check, so that nobody reads more into a pass than is there:

* Links out of `docs/adr/` into the rest of the repository.
* Relative paths in files that are not Markdown. Only the URL and the bare
  `docs/adr/` mention are read there.
* A bracket whose text has no letter, no `#` and no code span, such as the
  interval `[+0.017, +0.116]`, is read as prose rather than a reference, and
  so is a task-list box `- [x]`. Any other bracketed text in an ADR is read
  as a reference, so a bracket that is not a link is written `\\[...\\]`.
* Fragments are compared exactly, case included. Only fenced code blocks are
  recognised as code; an indented block is read as prose.
* Heading ids are computed here, not fetched from GitHub. Each heading in
  the self-test's fixture, which holds emphasis, entities, escapes, code
  spans, links, images, an HTML comment, an autolink and setext underlines,
  gets the id GitHub gives it, apart from the setext case below. A heading
  written another way may not. Emphasis is paired here without CommonMark's
  rule of three, and across a link's brackets, which GitHub does not do.
* A setext heading is found only when its text is one line, after a blank
  line or an ATX heading, that does not start with `#`, `>`, `|` or a list
  marker. GitHub also renders, for example, one whose text starts with `#`
  or `|`, runs over several lines, or sits in a list item or blockquote; a
  link to such a heading fails here.
* This file's own source, whose self-test fixtures name ADRs that exist only
  in the fixture tree.

Usage:
    ./scripts/check_adrs.py --check      # self-test, then scan the repository
    ./scripts/check_adrs.py --self-test  # run the fixtures only

Exit codes:
    0  every link resolves and every reference is defined
    1  at least one does not, the self-test failed, or nothing was read
"""

from __future__ import annotations

import argparse
import html
import html.entities
import os
import posixpath
import re
import string
import subprocess
import sys
import tempfile
import textwrap
import unicodedata
from typing import Dict, List, NamedTuple, Optional, Set, Tuple
from urllib.parse import unquote

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SELF = "scripts/check_adrs.py"
ADR_DIR = "docs/adr"
ADR_FILE = re.compile(r"^docs/adr/\d{4}-[^/]+\.md$")

FENCE_OPEN = re.compile(r"^ {0,3}(`{3,}|~{3,})")
HTML_COMMENT = re.compile(r"<!--.*?-->", re.S)
BLANK_LINE = re.compile(r"\n[ \t]*\n")

INLINE_TARGET = re.compile(r"\]\(\s*(?:<([^>\n]+)>|([^\s)]+))")
DEFINITION = re.compile(r"^ {0,3}\[((?:[^\[\]\\]|\\.)+)\]:[ \t]*(?:<([^>\n]+)>|(\S+))", re.M)
HREF = re.compile(r"""<a\s[^>]*?\bhref\s*=\s*["']([^"']+)["']""", re.I)
SCHEME = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:|^//")
ADR_SEGMENT = re.compile(r"(?:^|/)adr(?:/|$)")

GGLIB_URL = re.compile(r"https?://github\.com/mmogr/gglib/blob/main/(docs/adr/[\w.%/-]*)(?:#([\w%-]+))?")
BARE_MENTION = re.compile(r"(?<![\w./-])(docs/adr/(?:\d{4}-[\w-]+|log-\d{4})\.md)(?:#([\w%-]+))?")

ATX = re.compile(r"^ {0,3}#{1,6}(?:[ \t]+(.*?))?(?:[ \t]+#+)?[ \t]*$")
SETEXT_UNDERLINE = re.compile(r"^ {0,3}(?:=+|-+)[ \t]*$")
# A line that a setext underline below it does not make a heading here: a
# list item, a thematic break, and any line that starts with `#`, `>` or `|`.
NOT_A_SETEXT_TEXT = re.compile(
    r"^ {0,3}(?:[#>|]|[*+-](?:[ \t]|$)|\d+[.)](?:[ \t]|$)|([-*_])(?:[ \t]*\1){2,}[ \t]*$)"
)
HTML_ANCHOR = re.compile(r"""<[A-Za-z][^>]*\s(?:id|name)\s*=\s*["']([^"']+)["']""")

IMAGE = re.compile(r"!\[[^\]]*\]\([^)]*\)")
LINK = re.compile(r"\[([^\]]*)\](?:\([^)]*\)|\[[^\]]*\])")
AUTOLINK = re.compile(r"<([A-Za-z][A-Za-z0-9+.-]{1,31}:[^\s<>]*)>")
HTML_TAG = re.compile(r"<!--.*?-->|</?[A-Za-z][A-Za-z0-9-]*(?:\s[^<>]*)?/?>", re.S)
ESCAPE = re.compile(r"\\([!-/:-@\[-`{-~])")
ENTITY = r"&(?:#\d{1,7}|#[xX][0-9A-Fa-f]{1,6}|[A-Za-z][A-Za-z0-9]{1,31});"
ESCAPE_OR_ENTITY = re.compile(ESCAPE.pattern + "|" + ENTITY)
DELIMITER_RUN = re.compile(r"\*+|_+")

# `[text](url)`, `[text][label]`, `[text][]` and `[label]`, over text whose
# code has been masked. Group 2 is the `(` of an inline link; group 3 is the
# label of a full or collapsed reference. A definition `[label]:` reads as a
# shortcut to itself, which its own line defines.
BRACKET = re.compile(r"(?<!\\)\[([^\[\]]*)\](?:(\()|\[([^\[\]]*)\])?")
TASK_ITEM_PREFIX = re.compile(r"^[ \t]*(?:[-*+]|\d+[.)])[ \t]+$")
LOOKS_LIKE_LABEL = re.compile(r"[^\W\d_]|#|`")


class Problem(NamedTuple):
    path: str
    line: int
    kind: str  # "missing", "fragment" or "label"
    detail: str

    def render(self) -> str:
        if self.kind == "missing":
            text = f"links {self.detail}, which is not a tracked file or directory"
        elif self.kind == "fragment":
            text = f"links {self.detail}, whose fragment names no heading or anchor there"
        else:
            text = f"uses [{self.detail}], which has no definition in this file"
        return f"{self.path}:{self.line}: {text}"


def line_of(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def blank(text: str, start: int, end: int, fill: str = " ") -> str:
    """Replace a span with `fill`, keeping newlines so offsets and lines hold."""
    middle = "".join(c if c == "\n" else fill for c in text[start:end])
    return text[:start] + middle + text[end:]


def without_fences(text: str) -> str:
    """The text with every fenced code block, fences included, blanked in place."""
    lines = text.split("\n")
    fence: Optional[str] = None
    for i, line in enumerate(lines):
        if fence is None:
            match = FENCE_OPEN.match(line)
            if match:
                fence = match.group(1)
                lines[i] = " " * len(line)
        else:
            stripped = line.strip()
            indent = len(line) - len(line.lstrip(" "))
            if indent <= 3 and len(stripped) >= len(fence) and set(stripped) == {fence[0]}:
                fence = None
            lines[i] = " " * len(line)
    return "\n".join(lines)


def code_spans(text: str) -> List[Tuple[int, int]]:
    """(start, end) of each code span's content, backticks excluded.

    A run of n backticks opens a span that the next run of exactly n closes,
    within one paragraph. A run with no partner is literal.
    """
    runs = [(m.start(), m.end()) for m in re.finditer(r"`+", text)]
    spans = []
    k = 0
    while k < len(runs):
        start, end = runs[k]
        width = end - start
        for j in range(k + 1, len(runs)):
            close_start, close_end = runs[j]
            if BLANK_LINE.search(text, end, close_start):
                break
            if close_end - close_start == width:
                spans.append((end, close_start))
                k = j
                break
        k += 1
    return spans


def masked(text: str) -> str:
    """Fences and HTML comments blanked, code-span content replaced by `x`.

    Lengths and newlines are kept, so an offset into the result is an offset
    into the original, and a label can be read back from the original.
    """
    text = without_fences(text)
    for start, end in code_spans(text):
        text = blank(text, start, end, "x")
    for match in HTML_COMMENT.finditer(text):
        text = blank(text, match.start(), match.end())
    return text


def is_punctuation(char: str) -> bool:
    """Punctuation as GitHub's emphasis reads it: ASCII punctuation or Unicode P*."""
    return char in string.punctuation or unicodedata.category(char).startswith("P")


def emphasis_delimiters(probe: str) -> Set[int]:
    """Offsets of the `*` and `_` that GitHub reads as emphasis and so does not show.

    Delimiter runs are classed as flanking and paired as CommonMark does,
    except for its rule of three. In `probe`, code spans, autolinks, HTML
    tags and escaped characters are already hidden.
    """

    class Opener:
        def __init__(self, char: str, start: int, remaining: int):
            self.char, self.start, self.remaining = char, start, remaining

    openers: List[Opener] = []
    dropped: Set[int] = set()
    for match in DELIMITER_RUN.finditer(probe):
        char, start, end = match.group(0)[0], match.start(), match.end()
        before = probe[start - 1] if start else " "
        after = probe[end] if end < len(probe) else " "
        left_flanking = not after.isspace() and (
            not is_punctuation(after) or before.isspace() or is_punctuation(before)
        )
        right_flanking = not before.isspace() and (
            not is_punctuation(before) or after.isspace() or is_punctuation(after)
        )
        opens, closes = left_flanking, right_flanking
        if char == "_":
            opens = left_flanking and (not right_flanking or is_punctuation(before))
            closes = right_flanking and (not left_flanking or is_punctuation(after))
        remaining = end - start
        while closes and remaining:
            k = next((k for k in reversed(range(len(openers))) if openers[k].char == char), None)
            if k is None:
                break
            opener = openers[k]
            used = min(opener.remaining, remaining)
            opener_end = opener.start + opener.remaining
            dropped.update(range(opener_end - used, opener_end))
            dropped.update(range(end - remaining, end - remaining + used))
            opener.remaining -= used
            remaining -= used
            del openers[k + 1 :]
            if not opener.remaining:
                del openers[k]
        if opens and remaining:
            openers.append(Opener(char, end - remaining, remaining))
    return dropped


def flattened(raw: str, pattern: re.Pattern[str]) -> str:
    """`raw` with each match of `pattern` replaced by its group 1, or removed
    when it has none. Code-span content is hidden from `pattern`, so link or
    image syntax inside a code span is not read."""
    probe = raw
    for start, end in code_spans(raw):
        probe = blank(probe, start, end, "x")
    out, last = [], 0
    for match in pattern.finditer(probe):
        out.append(raw[last : match.start()])
        if pattern.groups:
            out.append(raw[match.start(1) : match.end(1)])
        last = match.end()
    out.append(raw[last:])
    return "".join(out)


def decoded(match: re.Match[str]) -> str:
    """An escaped character, or an entity. A full HTML5 entity name is
    decoded, and any other name, such as `&copyright;`, stays as written,
    as CommonMark reads them. A numeric reference is decoded by
    `html.unescape`, which differs from CommonMark, and from GitHub, for
    most of `&#127;` to `&#159;` and some control characters."""
    if match.group(1):
        return match.group(1)
    entity = match.group(0)
    if entity[1] == "#" or entity[1:] in html.entities.html5:
        return html.unescape(entity)
    return entity


def heading_text(raw: str) -> str:
    """What GitHub renders for a heading's inline content, as plain text."""
    raw = flattened(flattened(raw, IMAGE), LINK)
    # Code spans, autolinks and HTML tags, each with the text it shows. Where
    # two overlap, the one that starts first is the one read.
    found = []
    for start, end in code_spans(raw):
        width = start - len(raw[:start].rstrip("`"))
        code = raw[start:end]
        found.append((start - width, end + width, code.strip(" ") or code))
    found += [(m.start(), m.end(), m.group(1)) for m in AUTOLINK.finditer(raw)]
    found += [(m.start(), m.end(), "") for m in HTML_TAG.finditer(raw)]
    spans: List[Tuple[int, int, str]] = []
    for start, end, shown in sorted(found):
        if not spans or start >= spans[-1][1]:
            spans.append((start, end, shown))
    # Emphasis is paired over the text with each span's inside and each
    # escaped character hidden, so that neither opens or closes it.
    probe = list(raw)
    for start, end, _ in spans:
        probe[start + 1 : end - 1] = "x" * (end - start - 2)
    dropped = emphasis_delimiters(ESCAPE.sub("\\\\!", "".join(probe)))

    def shown_text(start: int, end: int) -> str:
        kept = "".join(raw[k] for k in range(start, end) if k not in dropped)
        return ESCAPE_OR_ENTITY.sub(decoded, kept)

    out = []
    last = 0
    for start, end, shown in spans:
        out.append(shown_text(last, start) + shown)
        last = end
    out.append(shown_text(last, len(raw)))
    return "".join(out)


def slug(text: str) -> str:
    """GitHub's heading id: lower case, punctuation dropped, spaces to hyphens."""
    return re.sub(r"[^\w\- ]", "", heading_text(text).lower()).replace(" ", "-")


def anchors(text: str) -> Set[str]:
    """Every fragment a link into this Markdown text can name."""
    lines = without_fences(text).split("\n")
    seen: Dict[str, int] = {}
    found: Set[str] = set()

    def add(heading: str) -> None:
        original = result = slug(heading)
        while result in seen:
            seen[original] += 1
            result = f"{original}-{seen[original]}"
        seen[result] = 0
        found.add(result)

    for i, line in enumerate(lines):
        atx = ATX.match(line)
        if atx:
            add((atx.group(1) or "").strip())
        elif (
            i + 1 < len(lines)
            and line.strip()
            and SETEXT_UNDERLINE.match(lines[i + 1])
            and not NOT_A_SETEXT_TEXT.match(line)
            and (i == 0 or not lines[i - 1].strip() or ATX.match(lines[i - 1]))
        ):
            add(line.strip())
    found.update(HTML_ANCHOR.findall(text))
    return found


class Repo:
    """The tracked files, and what reading them has found."""

    def __init__(self, root: str, files: List[str]):
        self.root = root
        self.files = set(files)
        self.dirs: Set[str] = set()
        for path in files:
            parent = posixpath.dirname(path)
            while parent and parent not in self.dirs:
                self.dirs.add(parent)
                parent = posixpath.dirname(parent)
        self._text: Dict[str, Optional[str]] = {}
        self._anchors: Dict[str, Set[str]] = {}
        self._resolved: Set[Tuple[str, int, str]] = set()
        self.problems: Set[Problem] = set()

    def text(self, path: str) -> Optional[str]:
        """The file's text, or None when it is binary."""
        if path not in self._text:
            with open(os.path.join(self.root, path), "rb") as handle:
                data = handle.read()
            self._text[path] = None if b"\0" in data else data.decode("utf-8", errors="replace")
        return self._text[path]

    @property
    def links_read(self) -> int:
        return len(self._resolved)

    def resolve(self, source: str, line: int, written: str, target: str, fragment: str) -> None:
        """Record a problem unless `target` exists and holds `fragment`.

        A root README's `](docs/adr/...)` is both a relative link and a bare
        mention; it is resolved, and counted, once.
        """
        if (source, line, written) in self._resolved:
            return
        self._resolved.add((source, line, written))
        if target not in self.files and target not in self.dirs:
            self.problems.add(Problem(source, line, "missing", written))
            return
        if not fragment:
            return
        if target not in self.files or not target.endswith(".md"):
            self.problems.add(Problem(source, line, "fragment", written))
            return
        if target not in self._anchors:
            self._anchors[target] = anchors(self.text(target) or "")
        if unquote(fragment) not in self._anchors[target]:
            self.problems.add(Problem(source, line, "fragment", written))


def markdown_links(text: str) -> List[Tuple[int, str]]:
    """(offset, target) of every inline link, definition and href outside code."""
    body = masked(text)
    links = []
    for match in INLINE_TARGET.finditer(body):
        links.append((match.start(), match.group(1) or match.group(2)))
    for match in DEFINITION.finditer(body):
        links.append((match.start(), match.group(2) or match.group(3)))
    for match in HREF.finditer(body):
        links.append((match.start(), match.group(1)))
    return links


def check_relative_links(repo: Repo, path: str, text: str) -> None:
    for offset, written in markdown_links(text):
        if SCHEME.match(written):
            continue
        location, _, fragment = written.partition("#")
        location = unquote(location.split("?", 1)[0])
        if not location:
            target = path
        elif location.startswith("/"):
            target = posixpath.normpath(location.lstrip("/"))
        else:
            target = posixpath.normpath(posixpath.join(posixpath.dirname(path), location))
        into_adrs = target == ADR_DIR or target.startswith(ADR_DIR + "/")
        if into_adrs or ADR_SEGMENT.search(location):
            repo.resolve(path, line_of(text, offset), written, target, fragment)


def check_mentions(repo: Repo, path: str, text: str) -> None:
    for match in GGLIB_URL.finditer(text):
        target = match.group(1).rstrip(".")
        written = match.group(0)
        repo.resolve(path, line_of(text, match.start()), written, unquote(target).rstrip("/"), match.group(2) or "")
    for match in BARE_MENTION.finditer(text):
        repo.resolve(path, line_of(text, match.start()), match.group(0), match.group(1), match.group(2) or "")


def normal_label(label: str) -> str:
    return " ".join(label.split()).casefold()


def check_reference_labels(repo: Repo, path: str, text: str) -> None:
    body = masked(text)
    defined = {normal_label(text[m.start(1) : m.end(1)]) for m in DEFINITION.finditer(body)}
    for match in BRACKET.finditer(body):
        if match.group(2):
            continue
        if match.group(3) is not None:
            start, end = match.span(3) if match.group(3).strip() else match.span(1)
        else:
            start, end = match.span(1)
            label = text[start:end]
            if not LOOKS_LIKE_LABEL.search(label):
                continue
            prefix = body[body.rfind("\n", 0, match.start()) + 1 : match.start()]
            if label in ("x", "X") and TASK_ITEM_PREFIX.match(prefix):
                continue
        label = text[start:end]
        if BLANK_LINE.search(label) or not label.strip():
            continue
        if normal_label(label) not in defined:
            repo.problems.add(Problem(path, line_of(text, match.start()), "label", label))


def scan(root: str, files: List[str]) -> Repo:
    repo = Repo(root, files)
    for path in sorted(files):
        if path == SELF:
            continue
        text = repo.text(path)
        if text is None:
            continue
        if path.endswith(".md"):
            check_relative_links(repo, path, text)
            if posixpath.dirname(path) == ADR_DIR:
                check_reference_labels(repo, path, text)
        check_mentions(repo, path, text)
    return repo


def tracked_files(root: str) -> List[str]:
    out = subprocess.run(["git", "-C", root, "ls-files", "-z"], check=True, capture_output=True).stdout
    names = [name for name in out.decode("utf-8").split("\0") if name]
    return [name for name in names if os.path.isfile(os.path.join(root, name))]


# ── Self-test ────────────────────────────────────────────────────────────────
#
# One small tree that plants faults for each of the four checks beside links
# that resolve, and holds constructs that must NOT be read as links. The ids
# that docs/headings.md links to were compared with GitHub's rendering of
# docs/adr/0004-headings.md: the first twenty-seven are ids GitHub gives, the
# last three are not. The scan has to report exactly EXPECTED: a missing entry
# is a check that stopped firing, an extra one is a check that fires on
# something it should not.

FIXTURE = {
    "README.md": """\
        [The ADRs](docs/adr/) and [one of them](docs/adr/0001-alpha.md#out-of-scope).
        """,
    "docs/guide.md": """\
        # Guide

        [ADR 0001](adr/0001-alpha.md#adr-0001--alpha-the-first_thing-stays),
        [its second notes](adr/0001-alpha.md#notes-1), [ADR 0002](adr/0002-beta.md#adr-0002--beta).
        [a missing ADR](adr/0099-missing.md)
        [a third notes](adr/0001-alpha.md#notes-2)
        [elsewhere](other.md#nothing) is not a link into the ADRs.
        <a href="adr/0002-beta.md#no-anchor">an href</a> and [an anchor](adr/0002-beta.md#custom-anchor).
        [ref]: adr/0001-alpha.md#no-such-heading

        A stray ` opens no code span past the end of its paragraph,

        [so this link is read](adr/0098-gone.md)

        and this stray ` closes none.

        A span ``a`b`` ends on a run as wide, [so this is read](adr/0096-gone.md), and `so is this`.

        ````markdown
        ```
        [inside a longer fence](adr/0097-gone.md)
        ````

        [after the fence](adr/0095-gone.md)
        """,
    "docs/headings.md": """\
        [private](adr/0004-headings.md#the-_private-field)
        [trailing](adr/0004-headings.md#trailing_-name-snake_case_name-and-emphasis)
        [entities](adr/0004-headings.md#a--b--amp)
        [autolink](adr/0004-headings.md#see-httpsexamplecom_x_)
        [interleaved](adr/0004-headings.md#a-_b-c_-and-1--2--0)
        [escapes](adr/0004-headings.md#_escaped_-_x_-and-a-b_-c)
        [code](adr/0004-headings.md#httpsexamplecom_y_-in-code)
        [symbols](adr/0004-headings.md#_a_-1-x-and-c)
        [nested](adr/0004-headings.md#nested-emphasis)
        [bold](adr/0004-headings.md#bold-line)
        [dash](adr/0004-headings.md#-dash)
        [ratio](adr/0004-headings.md#15-ratio)
        [flanking](adr/0004-headings.md#paren-and-more)
        [punctuation](adr/0004-headings.md#a)
        [partial](adr/0004-headings.md#xa)
        [innermost](adr/0004-headings.md#a-b-c)
        [comment](adr/0004-headings.md#x--y)
        [padded](adr/0004-headings.md#a_b-spaced)
        [image](adr/0004-headings.md#image--dropped)
        [link](adr/0004-headings.md#retracted-log-0001)
        [not an entity](adr/0004-headings.md#copyright-notice)
        [link in code](adr/0004-headings.md#use-xy-in-code)
        [image in code](adr/0004-headings.md#a-ij-code)
        [code in a link](adr/0004-headings.md#see-a_b-here)
        [reference link](adr/0004-headings.md#see-ref-text-here)
        [three](adr/0004-headings.md#ab)
        [after atx](adr/0004-headings.md#setext-after-atx)
        [a list item](adr/0004-headings.md#--item)
        [a thematic break](adr/0004-headings.md#---)
        [a paragraph's last line](adr/0004-headings.md#text-two)
        """,
    "crates/c/README.md": """\
        [one level short](../docs/adr/0001-alpha.md)
        [right](../../docs/adr/0001-alpha.md)
        ![a missing image](../../docs/adr/0094-gone.png)
        """,
    "docs/adr/0001-alpha.md": """\
        # ADR 0001 — Alpha: the `first_thing` stays

        - **Log:** [log-0001](log-0001.md#2026-01-02-a-reading)

        ## Notes

        A full reference [the tracker][tracker], a collapsed one [Tracker][],
        a shortcut [#12], a code-span shortcut [`probe`] and
        an interval [+0.1, 0.2], which is prose. [Back](#notes-1), [out](#out-of-scope).

        ## Notes

        [an undefined label][nowhere]
        and [`missing_probe`]
        and [a missing heading](#no-such-heading).

        ~~~text
        ## Notes
        [inside a fence][nowhere] and [a link](0099-missing.md)
        ~~~

        `[in a code span][nowhere]` and \\[escaped\\].
        <!-- [in a comment][nowhere] -->

        ## Out of scope

        [tracker]: https://example.com/tracker
        [#12]: https://example.com/12
        [`probe`]: https://example.com/probe
        """,
    "docs/adr/0002-beta.md": """\
        ADR 0002 — Beta
        ===============

        Back to [alpha](0001-alpha.md) and on to [nothing](0003-gone.md).
        <a id="custom-anchor"></a>
        """,
    "docs/adr/log-0001.md": """\
        # ADR 0001 log

        ## 2026-01-02: a reading

        A moved reading cites [the probe run][probe-run], whose definition stayed behind.

        - [x] A done task is not a reference.

        A collapsed reference [Nowhere][] has no definition either.
        """,
    "docs/adr/0004-headings.md": """\
        ## The _private field

        ## trailing_ name, snake_case_name and _emphasis_

        ## A &mdash; B &amp; `&amp;`

        ## See <https://example.com/_x_>

        ## *a _b* c_ and 1 < 2 > 0

        ## \\_escaped\\_, &#95;x&#95; and _a `b_` c_

        ## `<https://example.com/_y_>` in code

        ## ©_a_, 1 <_x_> and «_c_»

        ## ___nested_ emphasis__

        **Bold line**
        ---

        -dash
        ===

        1.5 ratio
        ---

        - item
        ---

        ---
        ---

        Text one
        Text two
        ---

        ## _(paren)_ and more

        ## (_(a)_)

        ## x*a**

        ## _a _b_ c_

        ## x <!-- a_b_c --> y

        ## ` a_b ` spaced

        ## Image ![alt_x](https://example.com/i.png) dropped

        ## Retracted: [log-0001](log-0001.md)

        ## &copyright; notice

        ## Use `[x](y)` in code

        ## A `![i](j)` code

        ## See [`a_b`](https://example.com/) here

        ## See [ref text][r] here

        ## *a**b*

        ## A heading
        Setext after ATX
        ---

        [r]: https://example.com/
        """,
    "src/lib.rs": """\
        //! [ADR 0001]: https://github.com/mmogr/gglib/blob/main/docs/adr/0001-alpha.md#notes
        //! [ADR 0002]: https://github.com/mmogr/gglib/blob/main/docs/adr/0002-renamed.md
        //! See docs/adr/0003-never-written.md and docs/adr/0001-alpha.md.
        //! Elsewhere: https://github.com/mmogr/modelpipe/blob/main/docs/adr/0002-theirs.md
        //! https://github.com/mmogr/gglib/blob/main/docs/adr/0001-alpha.md#gone.
        //! docs/adr/0001-alpha.md#notes resolves; docs/adr/0001-alpha.md#gone does not.
        """,
}

EXPECTED = {
    ("crates/c/README.md", 1, "missing"),
    ("crates/c/README.md", 3, "missing"),
    ("docs/adr/0001-alpha.md", 13, "label"),
    ("docs/adr/0001-alpha.md", 14, "label"),
    ("docs/adr/0001-alpha.md", 15, "fragment"),
    ("docs/adr/0002-beta.md", 4, "missing"),
    ("docs/adr/log-0001.md", 5, "label"),
    ("docs/adr/log-0001.md", 9, "label"),
    ("docs/guide.md", 5, "missing"),
    ("docs/guide.md", 6, "fragment"),
    ("docs/guide.md", 8, "fragment"),
    ("docs/guide.md", 9, "fragment"),
    ("docs/guide.md", 13, "missing"),
    ("docs/guide.md", 17, "missing"),
    ("docs/guide.md", 24, "missing"),
    ("docs/headings.md", 28, "fragment"),
    ("docs/headings.md", 29, "fragment"),
    ("docs/headings.md", 30, "fragment"),
    ("src/lib.rs", 2, "missing"),
    ("src/lib.rs", 3, "missing"),
    ("src/lib.rs", 5, "fragment"),
    ("src/lib.rs", 6, "fragment"),
}


def self_test() -> bool:
    with tempfile.TemporaryDirectory() as root:
        for path, body in FIXTURE.items():
            os.makedirs(os.path.join(root, posixpath.dirname(path)), exist_ok=True)
            with open(os.path.join(root, path), "w", encoding="utf-8") as handle:
                handle.write(textwrap.dedent(body))
        repo = scan(root, list(FIXTURE))
    found = {(p.path, p.line, p.kind) for p in repo.problems}
    if found == EXPECTED and len(repo.problems) == len(EXPECTED):
        print(f"✅ self-test: {len(EXPECTED)} planted faults found, nothing else")
        return True
    print("❌ self-test failed: the checker no longer reports what the fixture plants")
    for item in sorted(EXPECTED - found):
        print(f"     not reported: {item}")
    for problem in sorted(repo.problems):
        if (problem.path, problem.line, problem.kind) not in EXPECTED:
            print(f"     reported but not planted: {problem.render()}")
    if found == EXPECTED:
        print("     (one line carries more problems than it should)")
    return False


def check() -> bool:
    files = tracked_files(REPO)
    adrs = sorted(path for path in files if ADR_FILE.match(path))
    repo = scan(REPO, files)
    print(f"Checking links into {ADR_DIR}/ ({len(adrs)} ADRs, {repo.links_read} links read)...")
    # Liveness: a scan that read nothing prints what a clean scan prints.
    if not adrs or not repo.links_read:
        print("❌ no ADR or no link into docs/adr/ was found; nothing was checked")
        return False
    if not repo.problems:
        print("✅ every link into docs/adr/ resolves, and every ADR reference is defined")
        return True
    for problem in sorted(repo.problems):
        print(f"❌ {problem.render()}")
    print(f"\n❌ {len(repo.problems)} problem(s). Fix the link or the heading it names;")
    print("   define a reference in the file that uses it, or escape a bracket that is not a link.")
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
