#!/usr/bin/env python3
"""Draw the README's status card from the repository's record.

The card is one 800x224 image, written twice, as `status-light.svg` and
`status-dark.svg`, which differ only in their colour values. It shows:

* The newest release, stable or candidate, and how many days ago it was
  published.
* llama.cpp's pin (`scripts/llama_pin.sh`) and how many whole days its release
  was published before upstream's latest. "Latest" is the release GitHub marks
  latest for ggml-org/llama.cpp, which is its newest `v*` release that is not
  a prerelease; the `b<n>` tags are prereleases. That is the release the
  Engine pins workflow takes its candidate from. A pin newer than the latest
  reads as level with it.
* Releases by week over 52 ISO weeks (UTC, Monday first), ending with the
  week of the card's date. A release counts in the week its publication date
  falls in. Each release is 2px, stable releases dark at the bottom and
  candidates grey above them. A week with more than the row holds is cut at
  the top of the row and marked with a `+` above it.
* Merged pull requests by component over the 90 days up to the card's date,
  one bar. A pull request counts once for each `component:` label it carries
  and not at all with none, which is how a bot's pull requests are left out
  when they carry none. Segment widths are proportional to the counts and
  fill the chart's width less 2px gaps, rounded to whole pixels, at least 1px
  each. The seven largest are named under their segments. A name wider than
  its segment is cut with `…`; one that would keep fewer than three
  characters moves to the rest line, which lists every other component as
  `name n` after the seventh segment, and ends with `…` when they do not
  all fit. With seven components or fewer the bar reaches the right edge and
  the rest line has no room; what it would have said is in the description.
* The commits on main since the last stable release, by the first line of
  each message: `feat:` or `feat(scope):` is a feature, `fix:` or
  `fix(scope):` a fix, and any `type!:` or `type(scope)!:` is breaking and
  only breaking.

The text is not text: each character is drawn from IBM Plex Mono's own
outlines, kept once in the SVG's `<defs>` and placed with `<use>`, so the card
looks the same wherever it is shown and needs no font. The fonts and their
licence are in `fonts/`. A `<title>` and `<desc>` say in words what the card
shows, for screen readers.

Usage:
    status_card.py collect --out data.json
    status_card.py render --data data.json --out-dir DIR [--today YYYY-MM-DD]
    status_card.py --self-test

`collect` reads the record through `gh`, which needs a token (`GH_TOKEN` in
CI; locally, `gh auth login`), and writes everything `render` reads into one
JSON file, with the date it was collected. It fails, and writes nothing,
when any read fails or when the pull request list comes back full, since a
full list may be a cut one. `render` reads only that file: the same file and
date give the same bytes. Its date is the file's unless `--today` names
another.

`render` and `--self-test` need fontTools, pinned in `requirements.txt`; the
workflow installs it into a venv first. `collect` needs only the standard
library. The self-test checks the week bins, the counts and the bot rule, the
`!` rule, the label cuts and the rest line, the clipped week, the wording,
and that the fixture renders to the golden byte for byte. To refresh the
golden after a deliberate change to the drawing:

    status_card.py render --data fixtures/data.json --out-dir /tmp/card
    cp /tmp/card/status-light.svg fixtures/status-light.svg

Exit codes:
    0  the card was collected, rendered, or passed its self-test
    1  a read failed, the data was unusable, or the self-test failed
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import math
import os
import re
import subprocess
import sys
import tempfile
from typing import Dict, List, NamedTuple, Optional, Sequence, Tuple
from xml.sax.saxutils import escape

HERE = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.dirname(os.path.dirname(HERE))
FONTS = os.path.join(HERE, "fonts")
FIXTURES = os.path.join(HERE, "fixtures")

REPO = "mmogr/gglib"
LLAMA = "ggml-org/llama.cpp"
WINDOW_DAYS = 90
WEEKS = 52
# GitHub's search returns at most 1000 results whatever `--limit` asks for, so
# a list of 1000 may be a cut one, and collect fails on it.
PR_LIMIT = 1000
RELEASE_LIMIT = 1000
MAX_BYTES = 60 * 1024
OUTPUTS = ("status-light.svg", "status-dark.svg")

FEATURE = re.compile(r"^feat(\(.+\))?:")
FIX = re.compile(r"^fix(\(.+\))?:")
BREAKING = re.compile(r"^\w+(\(.+\))?!:")
COMPONENT = "component:"

MONTHS = ("Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec")

# --- Geometry --------------------------------------------------------------
#
# The approved mockup is HTML: a 800x224 box with a 1px border and padding
# 18/24/16, so the content runs x 25..775 and y 19..207, and its five rows
# are spread by `justify-content: space-between`. IBM Plex Mono's line box is
# 1.3em with the baseline 1.025em below its top. The rows are 19.5 (15px
# text), 28 (the week columns), 29 (10px bar, 5px gap, a 14px line of 11px
# labels), 19.5 and 14.3 (11px text) high, which leaves four gaps of 19.425.
# The numbers below are the positions that gives, rounded.
WIDTH, HEIGHT = 800, 224
LEFT, RIGHT = 25, 775
CHART_X = LEFT + 112
CHART_W = RIGHT - CHART_X
ADVANCE = 0.6  # every glyph's advance in em; asserted against the fonts

HEADER_BASE = 34.4
WEEK_TOP, WEEK_BOTTOM = 58, 86  # the week columns' row
WEEK_GAP = 3
WEEK_LABEL_BASES = (68.4, 82.4)  # "releases," / "12 months", bottom-aligned
RELEASE_PX = 2
BAR_TOP, BAR_H, BAR_GAP = 105, 10, 2
BAR_LABEL_BASE = 116.9  # "90 days"
SEGMENT_LABEL_BASE = 131.5
SINCE_BASE = 169.2
SINCE_GROUP_GAP = 18
FOOTER_BASE = 204
NAMED_SEGMENTS = 7
MIN_KEPT = 3

PALETTES = {
    "light": {
        "card": "#f6f8fa",
        "border": "#d0d7de",
        "text": "#1f2328",
        "muted": "#59636e",
        "base": "#d0d7de",
        "stable": "#1f2328",
        "rc": "#b6bcc4",
        "ramp": (
            "#c9651a", "#d4701f", "#de7b27", "#e68733", "#ec9342", "#f09f55", "#f3ab69",
            "#f5b77e", "#f7c393", "#f9cfa8", "#fadabc", "#fbe3cc", "#fcebd9", "#fdf1e4",
        ),
    },
    "dark": {
        "card": "#202024",
        "border": "#3a3a40",
        "text": "#edece8",
        "muted": "#a9a8a2",
        "base": "#3a3a40",
        "stable": "#edece8",
        "rc": "#5c5c63",
        "ramp": (
            "#f7a950", "#ee9d47", "#e1913f", "#d28537", "#c27a31", "#b26f2c", "#a26427",
            "#915923", "#804e1f", "#6f441c", "#5e3a19", "#4e3117", "#402a15", "#342313",
        ),
    },
}


class Failure(Exception):
    """A read or a check that ends the run with a message."""


# --- Collect ---------------------------------------------------------------


def gh(*args: str) -> str:
    try:
        done = subprocess.run(["gh", *args], capture_output=True, text=True, check=False)
    except FileNotFoundError as err:
        raise Failure("gh is not installed") from err
    if done.returncode != 0:
        raise Failure(f"gh {' '.join(args)} failed:\n{done.stderr.strip()}")
    return done.stdout


def gh_json(*args: str):
    return json.loads(gh(*args))


def llama_pin() -> str:
    script = os.path.join(REPO_ROOT, "scripts", "llama_pin.sh")
    done = subprocess.run([script], capture_output=True, text=True, check=False)
    if done.returncode != 0:
        raise Failure(f"llama_pin.sh failed:\n{done.stderr.strip()}")
    return done.stdout.strip()


def collect(out: str, now: dt.datetime) -> None:
    today = now.date()
    raw = gh_json(
        "release", "list", "-R", REPO, "--limit", str(RELEASE_LIMIT),
        "--json", "tagName,publishedAt,isPrerelease,isDraft",
    )
    if len(raw) >= RELEASE_LIMIT:
        raise Failure(f"the release list came back full ({RELEASE_LIMIT}); it may be cut")
    releases = [
        {"tag": r["tagName"], "published_at": r["publishedAt"], "prerelease": r["isPrerelease"]}
        for r in raw
        if not r["isDraft"] and r["publishedAt"]
    ]
    stable = [r for r in releases if not r["prerelease"]]
    if not stable:
        raise Failure("there is no stable release to count changes from")
    last_stable = max(stable, key=lambda r: r["published_at"])["tag"]

    pin = llama_pin()
    pinned = gh_json("release", "view", pin, "-R", LLAMA, "--json", "tagName,publishedAt")
    latest = gh_json("release", "view", "-R", LLAMA, "--json", "tagName,publishedAt")

    since = (today - dt.timedelta(days=WINDOW_DAYS)).isoformat()
    prs = gh_json(
        "pr", "list", "-R", REPO, "--state", "merged", "--search", f"merged:>={since}",
        "--json", "number,mergedAt,title,labels", "--limit", str(PR_LIMIT),
    )
    if len(prs) >= PR_LIMIT:
        raise Failure(f"the pull request list came back full ({PR_LIMIT}); it may be cut")

    messages = gh(
        "api", f"repos/{REPO}/compare/{last_stable}...main", "--paginate",
        "--jq", '.commits[].commit.message | split("\\n")[0]',
    )

    data = {
        "today": today.isoformat(),
        "collected_at": now.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "releases": releases,
        "llama": {
            "pin": pinned["tagName"],
            "pin_published_at": pinned["publishedAt"],
            "latest": latest["tagName"],
            "latest_published_at": latest["publishedAt"],
        },
        "pull_requests": [
            {
                "number": p["number"],
                "merged_at": p["mergedAt"],
                "title": p["title"],
                "labels": sorted(label["name"] for label in p["labels"]),
            }
            for p in sorted(prs, key=lambda p: p["number"])
        ],
        "last_stable": last_stable,
        "commits_since_stable": [line for line in messages.splitlines() if line],
    }
    directory = os.path.dirname(os.path.abspath(out))
    with tempfile.NamedTemporaryFile("w", dir=directory, delete=False, encoding="utf-8") as handle:
        json.dump(data, handle, indent=1, ensure_ascii=False)
        handle.write("\n")
    os.replace(handle.name, out)


# --- What the card says ----------------------------------------------------


def instant(text: str) -> dt.datetime:
    return dt.datetime.fromisoformat(text.replace("Z", "+00:00")).astimezone(dt.timezone.utc)


def day_of(text: str) -> dt.date:
    return instant(text).date()


def monday(day: dt.date) -> dt.date:
    return day - dt.timedelta(days=day.weekday())


def week_bins(releases: Sequence[dict], today: dt.date) -> List[Tuple[int, int]]:
    """(stable, candidates) per ISO week, the oldest first, today's week last."""
    first = monday(today) - dt.timedelta(weeks=WEEKS - 1)
    bins = [[0, 0] for _ in range(WEEKS)]
    for release in releases:
        day = day_of(release["published_at"])
        if day > today:
            continue
        index = (monday(day) - first).days // 7
        if 0 <= index < WEEKS:
            bins[index][1 if release["prerelease"] else 0] += 1
    return [(s, r) for s, r in bins]


def component_counts(prs: Sequence[dict], today: dt.date) -> List[Tuple[str, int]]:
    """Merged pull requests per component in the window, the largest first."""
    start = today - dt.timedelta(days=WINDOW_DAYS)
    counts: Dict[str, int] = {}
    for pr in prs:
        if not start <= day_of(pr["merged_at"]) <= today:
            continue
        names = {
            label[len(COMPONENT):].strip()
            for label in pr["labels"]
            if label.startswith(COMPONENT) and label[len(COMPONENT):].strip()
        }
        for name in names:
            counts[name] = counts.get(name, 0) + 1
    return sorted(counts.items(), key=lambda item: (-item[1], item[0]))


def change_counts(lines: Sequence[str]) -> Tuple[int, int, int]:
    """(features, fixes, breaking) among commit subjects."""
    features = fixes = breaking = 0
    for line in lines:
        if BREAKING.match(line):
            breaking += 1
        elif FEATURE.match(line):
            features += 1
        elif FIX.match(line):
            fixes += 1
    return features, fixes, breaking


def days_word(n: int) -> str:
    return "1 day" if n == 1 else f"{n} days"


def age_words(days: int) -> str:
    if days <= 0:
        return "released today"
    if days == 1:
        return "released yesterday"
    return f"released {days} days ago"


def behind_words(days: int) -> str:
    return "level with upstream" if days <= 0 else f"{days_word(days)} behind upstream"


def plural(n: int, one: str, many: str) -> str:
    return one if n == 1 else many


def short_date(day: dt.date, year: bool) -> str:
    text = f"{day.day} {MONTHS[day.month - 1]}"
    return f"{text} {day.year}" if year else text


def window_words(today: dt.date) -> str:
    start = today - dt.timedelta(days=WINDOW_DAYS)
    return f"{short_date(start, start.year != today.year)} to {short_date(today, True)}"


def segment_widths(counts: Sequence[int], width: float) -> List[int]:
    """Whole-pixel widths proportional to `counts`, at least 1 each, summing to
    `width` less the gaps."""
    room = int(width) - BAR_GAP * (len(counts) - 1)
    if room < len(counts):
        raise Failure(f"{len(counts)} components do not fit in {width}px")
    total = sum(counts)
    exact = [c * room / total for c in counts]
    widths = [max(1, math.floor(e)) for e in exact]
    order = sorted(range(len(counts)), key=lambda i: (-(exact[i] - math.floor(exact[i])), i))
    spare = room - sum(widths)
    for i in order:
        if spare <= 0:
            break
        widths[i] += 1
        spare -= 1
    while sum(widths) > room:  # the 1px floor took more than the rounding gave back
        widths[widths.index(max(widths))] -= 1
    return widths


def fit_name(name: str, width: float, size: float) -> Optional[str]:
    """The name as shown in `width`: whole, cut with an ellipsis, or None when
    the cut would keep fewer than three characters."""
    room = math.floor(width / (ADVANCE * size) + 1e-9)
    if len(name) <= room:
        return name
    if room - 1 < MIN_KEPT:
        return None
    return name[: room - 1] + "…"


def fit_rest(items: Sequence[str], width: float, size: float) -> str:
    """As many whole items as fit, ending with an ellipsis when some do not."""
    room = math.floor(width / (ADVANCE * size) + 1e-9)
    for keep in range(len(items), -1, -1):
        text = ", ".join(items[:keep])
        if keep < len(items):
            text = f"{text}, …" if keep else "…"
        if len(text) <= room:
            return text
    return ""


class Segment(NamedTuple):
    name: str
    count: int
    x: int
    width: int
    label: Optional[str]  # None: not named under the segment


class Card(NamedTuple):
    today: dt.date
    version: str
    age: str
    pin: str
    behind: str
    weeks: List[Tuple[int, int]]
    segments: List[Segment]
    rest_x: float
    rest: str
    rest_items: List[str]
    last_stable: str
    changes: Tuple[int, int, int]


def card_of(data: dict, today: dt.date) -> Card:
    releases = data["releases"]
    if not releases:
        raise Failure("the data holds no release")
    newest = max(releases, key=lambda r: r["published_at"])
    llama = data["llama"]
    behind = (day_of(llama["latest_published_at"]) - day_of(llama["pin_published_at"])).days

    counts = component_counts(data["pull_requests"], today)
    if not counts:
        raise Failure("no merged pull request in the window carries a component label")
    widths = segment_widths([n for _, n in counts], CHART_W)
    segments: List[Segment] = []
    moved: List[str] = []
    x = CHART_X
    for index, ((name, count), width) in enumerate(zip(counts, widths)):
        label = None
        if index < NAMED_SEGMENTS:
            label = fit_name(name, width, 11)
            if label is None:
                moved.append(name)
        segments.append(Segment(name, count, x, width, label))
        x += width + BAR_GAP
    rest_names = set(moved) | {name for name, _ in counts[NAMED_SEGMENTS:]}
    rest_items = [f"{name} {count}" for name, count in counts if name in rest_names]
    named = segments[: min(NAMED_SEGMENTS, len(segments))]
    rest_x = named[-1].x + named[-1].width + BAR_GAP
    rest = fit_rest(rest_items, RIGHT - rest_x, 11) if rest_items else ""

    return Card(
        today=today,
        version=newest["tag"],
        age=age_words((today - day_of(newest["published_at"])).days),
        pin=llama["pin"],
        behind=behind_words(behind),
        weeks=week_bins(releases, today),
        segments=segments,
        rest_x=rest_x,
        rest=rest,
        rest_items=rest_items,
        last_stable=data["last_stable"].removeprefix("v"),
        changes=change_counts(data["commits_since_stable"]),
    )


def description(card: Card) -> str:
    stable = sum(s for s, _ in card.weeks)
    candidates = sum(r for _, r in card.weeks)
    components = ", ".join(f"{s.name} {s.count}" for s in card.segments)
    features, fixes, breaking = card.changes
    return (
        f"{card.version}, {card.age}. llama.cpp {card.pin}, {card.behind}. "
        f"Releases in the last 52 weeks: {stable} stable and {candidates} candidates. "
        f"Merged pull requests by component label, {window_words(card.today)}: {components}. "
        f"Since {card.last_stable}: {features} {plural(features, 'feature', 'features')}, "
        f"{fixes} {plural(fixes, 'fix', 'fixes')}, {breaking} breaking."
    )


# --- Drawing ---------------------------------------------------------------


class Font(NamedTuple):
    prefix: str
    upm: int
    advance: int
    cmap: Dict[int, str]
    paths: Dict[str, str]  # glyph name -> path data, filled on first use
    glyphs: object


def load_font(path: str, prefix: str) -> Font:
    from fontTools.ttLib import TTFont

    font = TTFont(path)
    upm = font["head"].unitsPerEm
    advances = {width for width, _ in font["hmtx"].metrics.values() if width}
    if len(advances) != 1:
        raise Failure(f"{os.path.basename(path)} is not monospaced: advances {sorted(advances)}")
    advance = advances.pop()
    if advance != round(ADVANCE * upm):
        raise Failure(f"{os.path.basename(path)}'s advance is {advance}/{upm}, not {ADVANCE}em")
    return Font(prefix, upm, advance, font.getBestCmap(), {}, font.getGlyphSet())


def glyph_path(font: Font, name: str) -> str:
    from fontTools.pens.svgPathPen import SVGPathPen

    if name not in font.paths:
        pen = SVGPathPen(font.glyphs, ntos=lambda v: str(int(round(v))))
        font.glyphs[name].draw(pen)
        font.paths[name] = pen.getCommands()
    return font.paths[name]


def number(v: float, places: int = 2) -> str:
    text = f"{v:.{places}f}".rstrip("0").rstrip(".")
    return "0" if text == "-0" else text


class Canvas:
    def __init__(self, fonts: Dict[str, Font], palette: dict):
        self.fonts = fonts
        self.palette = palette
        self.used: Dict[str, Tuple[Font, str]] = {}
        self.body: List[str] = []

    def colour(self, key: str) -> str:
        return self.palette[key]

    def rect(self, x: float, y: float, w: float, h: float, fill: str, rx: float = 0) -> None:
        corner = f' rx="{number(rx)}"' if rx else ""
        self.body.append(
            f'<rect x="{number(x)}" y="{number(y)}" width="{number(w)}" height="{number(h)}"'
            f'{corner} fill="{fill}"/>'
        )

    def text(self, x: float, base: float, text: str, size: float, colour: str,
             weight: str = "r", anchor: str = "start") -> float:
        """Draw `text` with its baseline at `base`; return its width."""
        font = self.fonts[weight]
        width = len(text) * ADVANCE * size
        if anchor == "end":
            x -= width
        scale = size / font.upm
        uses = []
        for i, char in enumerate(text):
            if char == " ":
                continue
            code = ord(char) if ord(char) in font.cmap else ord("?")
            ident = f"{font.prefix}{code}"
            self.used[ident] = (font, font.cmap[code])
            offset = f' x="{i * font.advance}"' if i else ""
            uses.append(f'<use href="#{ident}"{offset}/>')
        self.body.append(
            f'<g fill="{self.colour(colour)}" transform="translate({number(x)} {number(base)}) '
            f'scale({number(scale, 4)} -{number(scale, 4)})">{"".join(uses)}</g>'
        )
        return width

    def svg(self, title: str, desc: str) -> str:
        defs = [
            f'<path id="{ident}" d="{glyph_path(font, name)}"/>'
            for ident, (font, name) in sorted(self.used.items())
        ]
        return "\n".join([
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" '
            f'viewBox="0 0 {WIDTH} {HEIGHT}" role="img" aria-labelledby="title desc">',
            f'<title id="title">{escape(title)}</title>',
            f'<desc id="desc">{escape(desc)}</desc>',
            "<defs>", *defs, "</defs>",
            *self.body,
            "</svg>",
            "",
        ])


def ramp(stops: Sequence[str], n: int) -> List[str]:
    """`n` colours spread evenly over the stops, the first and last included."""
    def rgb(hex_colour: str) -> Tuple[int, ...]:
        return tuple(int(hex_colour[i:i + 2], 16) for i in (1, 3, 5))

    out = []
    for i in range(n):
        at = i * (len(stops) - 1) / (n - 1) if n > 1 else 0.0
        low = min(int(at), len(stops) - 2)
        t = at - low
        a, b = rgb(stops[low]), rgb(stops[low + 1])
        out.append("#" + "".join(f"{round(p + (q - p) * t):02x}" for p, q in zip(a, b)))
    return out


def draw(card: Card, fonts: Dict[str, Font], theme: str) -> str:
    c = Canvas(fonts, PALETTES[theme])
    p = c.palette
    c.body.append(
        f'<rect x="0.5" y="0.5" width="{WIDTH - 1}" height="{HEIGHT - 1}" rx="5.5" '
        f'fill="{p["card"]}" stroke="{p["border"]}"/>'
    )

    # The newest release and the pin.
    x = LEFT + c.text(LEFT, HEADER_BASE, card.version, 15, "text", "s") + 10
    c.text(x, HEADER_BASE, card.age, 13, "muted")
    x = RIGHT - c.text(RIGHT, HEADER_BASE, card.behind, 13, "muted", anchor="end") - 10
    c.text(x, HEADER_BASE, f"llama.cpp {card.pin}", 13, "text", "s", anchor="end")

    # Releases by week. Like the mockup's `column-reverse` flex with a 1px
    # gap: a 1px base, then the stable releases, then the candidates, each
    # item 1px above the last even when it is empty.
    c.text(LEFT, WEEK_LABEL_BASES[0], "releases,", 12, "muted")
    c.text(LEFT, WEEK_LABEL_BASES[1], "12 months", 12, "muted")
    column = (CHART_W - WEEK_GAP * (WEEKS - 1)) / WEEKS
    room = (WEEK_BOTTOM - WEEK_TOP) - 3
    for i, (stable, candidates) in enumerate(card.weeks):
        x = CHART_X + i * (column + WEEK_GAP)
        s = min(stable * RELEASE_PX, room)
        r = min(candidates * RELEASE_PX, room - s)
        c.rect(x, WEEK_BOTTOM - 1, column, 1, p["base"], 0.5)
        if s:
            c.rect(x, WEEK_BOTTOM - 2 - s, column, s, p["stable"], 1)
        if r:
            c.rect(x, WEEK_BOTTOM - 3 - s - r, column, r, p["rc"], 1)
        if (stable + candidates) * RELEASE_PX > room:
            c.text(x + column / 2 - ADVANCE * 9 / 2, WEEK_TOP - 2, "+", 9, "text", "s")

    # Merged pull requests by component.
    c.text(LEFT, BAR_LABEL_BASE, "90 days", 12, "muted")
    for colour, segment in zip(ramp(p["ramp"], len(card.segments)), card.segments):
        c.rect(segment.x, BAR_TOP, segment.width, BAR_H, colour, 2)
        if segment.label:
            c.text(segment.x, SEGMENT_LABEL_BASE, segment.label, 11, "muted")
    if card.rest:
        c.text(card.rest_x, SEGMENT_LABEL_BASE, card.rest, 11, "muted")

    # Changes since the last stable release.
    c.text(LEFT, SINCE_BASE, f"since {card.last_stable}", 12, "muted")
    features, fixes, breaking = card.changes
    x = CHART_X
    for n, word in (
        (features, plural(features, "feature", "features")),
        (fixes, plural(fixes, "fix", "fixes")),
        (breaking, "breaking"),
    ):
        x += c.text(x, SINCE_BASE, str(n), 15, "text", "s")
        x += c.text(x, SINCE_BASE, f" {word}", 13, "muted") + SINCE_GROUP_GAP

    # What the bar counts, over which days, and the card's date.
    c.text(LEFT, FOOTER_BASE,
           f"merged PRs by component label, bots left out · {window_words(card.today)}",
           11, "muted")
    c.text(RIGHT, FOOTER_BASE, f"generated {short_date(card.today, True)}", 11, "muted",
           anchor="end")

    title = f"gglib status card, {short_date(card.today, True)}"
    return c.svg(title, description(card))


def load_fonts() -> Dict[str, Font]:
    return {
        "r": load_font(os.path.join(FONTS, "IBMPlexMono-Regular.woff"), "r"),
        "s": load_font(os.path.join(FONTS, "IBMPlexMono-SemiBold.woff"), "s"),
    }


def render(data: dict, today: dt.date) -> Dict[str, str]:
    card = card_of(data, today)
    fonts = load_fonts()
    svgs = {name: draw(card, fonts, theme) for name, theme in zip(OUTPUTS, ("light", "dark"))}
    for name, svg in svgs.items():
        size = len(svg.encode("utf-8"))
        if size >= MAX_BYTES:
            raise Failure(f"{name} is {size} bytes, over the {MAX_BYTES}-byte budget")
    return svgs


def write_svgs(svgs: Dict[str, str], out_dir: str) -> None:
    os.makedirs(out_dir, exist_ok=True)
    for name, svg in svgs.items():
        with open(os.path.join(out_dir, name), "w", encoding="utf-8", newline="\n") as handle:
            handle.write(svg)


# --- Self-test -------------------------------------------------------------

COLOUR = re.compile(r"#[0-9a-fA-F]{6}\b")


def release(tag: str, when: str, pre: bool) -> dict:
    return {"tag": tag, "published_at": when, "prerelease": pre}


def pr(number: int, when: str, *labels: str) -> dict:
    return {"number": number, "merged_at": when, "title": f"pr {number}", "labels": list(labels)}


def self_test() -> bool:
    failures: List[str] = []
    cases = 0

    def case(name: str, got, want) -> None:
        nonlocal cases
        cases += 1
        if got != want:
            failures.append(f"{name}: got {got!r}, want {want!r}")

    today = dt.date(2026, 10, 28)  # a Wednesday; its week starts Monday 26 Oct

    bins = week_bins([
        release("v1", "2026-10-25T23:59:00Z", False),  # Sunday: the week before
        release("v2", "2026-10-26T00:01:00Z", False),  # Monday: this week
        release("v2-rc.1", "2026-10-26T09:00:00Z", True),
        release("v0", "2025-10-26T12:00:00Z", False),  # 52 weeks before this Monday: out
        release("v0.9", "2025-11-03T12:00:00Z", True),  # the first week shown
        release("v3", "2026-10-29T12:00:00Z", False),  # after the card's date, same week
    ], today)
    case("weeks: 52 columns", len(bins), WEEKS)
    case("weeks: Monday in this week, the day after the card left out", bins[-1], (1, 1))
    case("weeks: Sunday in the week before", bins[-2], (1, 0))
    case("weeks: the first column is 51 weeks back", bins[0], (0, 1))
    case("weeks: nothing else", sum(s + r for s, r in bins), 4)

    counts = component_counts([
        pr(1, "2026-10-01T10:00:00Z", "component: core", "type: feature"),
        pr(2, "2026-10-02T10:00:00Z", "component: core", "component: cli"),
        pr(3, "2026-10-03T10:00:00Z", "dependencies", "automated"),  # no component: left out
        pr(4, "2026-10-04T10:00:00Z", "component:build"),  # no space after the colon
        pr(5, "2026-07-29T23:00:00Z", "component: core"),  # the day before the window
        pr(6, "2026-07-30T00:30:00Z", "component: cli"),  # the window's first day
        pr(7, "2026-10-05T10:00:00Z", "component: core", "component: core "),  # once per component
    ], today)
    case("counts: per component, bots and old PRs out", counts,
         [("core", 3), ("cli", 2), ("build", 1)])

    case("changes: the ! rule", change_counts([
        "feat(chat): a thing (#1)",
        "feat: another",
        "fix(mcp): a fix",
        "fix: another fix",
        "feat!: breaking, not a feature",
        "feat(core)!: breaking too",
        "refactor(db)!: breaking whatever the type",
        "chore(deps): bump",
        "Bump version to 0.29.0-rc.1 (#1637)",
        "feature: not a type",
        "fixup: not a fix",
    ]), (2, 2, 3))

    case("labels: a name that fits is whole", fit_name("core", 81, 11), "core")
    case("labels: exactly the room", fit_name("mcp", 19.8, 11), "mcp")
    case("labels: a long name is cut", fit_name("downloads", 33, 11), "down…")
    case("labels: three kept is enough", fit_name("downloads", 26.4, 11), "dow…")
    case("labels: fewer than three moves", fit_name("broadcast", 26, 11), None)
    case("rest: all fit", fit_rest(["gui 6", "db 4"], 200, 11), "gui 6, db 4")
    case("rest: whole items, then an ellipsis", fit_rest(["gui 6", "db 4", "hf 3"], 80, 11),
         "gui 6, …")
    case("rest: not even one", fit_rest(["broadcast 12"], 20, 11), "…")

    widths = segment_widths([300, 2, 1, 1], 100)
    case("widths: fill the room", sum(widths), 100 - 3 * BAR_GAP)
    case("widths: at least 1px", min(widths), 1)

    moved = card_of({
        "releases": [release("v1.0.0", "2026-10-27T10:00:00Z", False)],
        "llama": {"pin": "b2", "pin_published_at": "2026-10-20T23:00:00Z",
                  "latest": "v0.8.0", "latest_published_at": "2026-10-19T10:00:00Z"},
        "pull_requests": [pr(i, "2026-10-20T10:00:00Z", "component: frontend") for i in range(200)]
        + [pr(1000 + i, "2026-10-20T10:00:00Z", f"component: {name}")
           for i, name in enumerate(["broadcast"] * 4 + ["db"] * 3)],
        "last_stable": "v1.0.0",
        "commits_since_stable": [],
    }, today)
    case("card: a cut-to-nothing name moves to the rest line",
         [s.label for s in moved.segments], ["frontend", None, None])
    case("card: the rest line lists them", moved.rest_items, ["broadcast 4", "db 3"])
    case("card: with three components the rest line has no room", moved.rest, "")
    case("card: a pin newer than the latest is level", moved.behind, "level with upstream")
    case("card: age in days", moved.age, "released yesterday")
    case("words: days behind", behind_words(1), "1 day behind upstream")
    case("words: a window across a year", window_words(dt.date(2027, 1, 15)),
         "17 Oct 2026 to 15 Jan 2027")

    case("ramp: the mockup's fourteen", ramp(PALETTES["light"]["ramp"], 14),
         list(PALETTES["light"]["ramp"]))

    with open(os.path.join(FIXTURES, "data.json"), encoding="utf-8") as handle:
        data = json.load(handle)
    fixture_today = dt.date.fromisoformat(data["today"])
    fixture = card_of(data, fixture_today)
    case("fixture: a clipped week", any((s + r) * RELEASE_PX > WEEK_BOTTOM - WEEK_TOP - 3
                                        for s, r in fixture.weeks), True)
    case("fixture: a cut label", any(s.label and s.label.endswith("…") for s in fixture.segments),
         True)
    svgs = render(data, fixture_today)
    light, dark = svgs[OUTPUTS[0]], svgs[OUTPUTS[1]]
    case("render: light and dark differ only in colour",
         COLOUR.sub("#000000", light) == COLOUR.sub("#000000", dark), True)
    case("render: light and dark do differ", light != dark, True)
    for name, svg in svgs.items():
        case(f"render: {name} under {MAX_BYTES} bytes", len(svg.encode("utf-8")) < MAX_BYTES, True)
    with open(os.path.join(FIXTURES, "status-light.svg"), encoding="utf-8") as handle:
        golden = handle.read()
    case("render: light is the golden, byte for byte", light == golden, True)

    if failures:
        print(f"❌ self-test: {len(failures)} of {cases} cases failed")
        for failure in failures:
            print(f"     {failure}")
        return False
    print(f"✅ self-test: {cases} cases pass, the golden included")
    return True


# --- Entry -----------------------------------------------------------------


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--self-test", action="store_true", help="run the cases and the golden")
    commands = parser.add_subparsers(dest="command")
    get = commands.add_parser("collect", help="read the record into one JSON file")
    get.add_argument("--out", required=True)
    draw_cmd = commands.add_parser("render", help="draw both cards from a collected file")
    draw_cmd.add_argument("--data", required=True)
    draw_cmd.add_argument("--out-dir", required=True)
    draw_cmd.add_argument("--today", type=dt.date.fromisoformat,
                          help="the card's date (default: the date the data was collected)")
    args = parser.parse_args(argv)

    try:
        if args.self_test:
            return 0 if self_test() else 1
        if args.command == "collect":
            collect(args.out, dt.datetime.now(dt.timezone.utc))
            print(f"✅ collected into {args.out}")
            return 0
        if args.command == "render":
            with open(args.data, encoding="utf-8") as handle:
                data = json.load(handle)
            today = args.today or dt.date.fromisoformat(data["today"])
            write_svgs(render(data, today), args.out_dir)
            print(f"✅ wrote {', '.join(OUTPUTS)} into {args.out_dir}")
            return 0
        parser.print_usage(sys.stderr)
        return 1
    except Failure as err:
        print(f"❌ {err}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
