#!/usr/bin/env python3
"""The rules that keep main and a maintenance line from releasing into each other.

    scripts/release_lines.py line      --branch B
    scripts/release_lines.py guard-pr  --branch B --version V --tags FILE [--branches FILE]
    scripts/release_lines.py guard-tag --branch B --version V --branches FILE
    scripts/release_lines.py candidate --input I --tags FILE
    scripts/release_lines.py previous  --stable S --tags FILE
    scripts/release_lines.py cut       --stable vS --tags FILE --branches FILE
    scripts/release_lines.py --self-test

Releases come from two kinds of branch. `main` carries the next minor,
0.30.0-rc.N, and a maintenance line `release/X.Y.x`, cut from the stable
tag vX.Y.0, carries that line's patches, X.Y.1-rc.N and so on. Both run the
same release PR, tag and Promote machinery; these are the checks that tell
the two apart, in one place so each has fixtures. Each subcommand prints
`key=value` lines on stdout for $GITHUB_OUTPUT and nothing else; a refusal
is one `::error::` line on stderr and exit 1, and says what to do.

* `line`: which line a branch releases. `line=` for main, `line=X.Y` for
  release/X.Y.x; any other branch is refused, since nothing else releases.
* `guard-pr`: the release PR's version (release-please.yml). It must be a
  release candidate of a version that has not shipped. On a line it must
  also be that line's next patch: a feat or a breaking change cherry-picked
  there proposes the next minor, and is refused with a word to revert it.
  On main it must not fall inside a line that has a branch, where that
  line's patches are cut. Each refusal names the `Release-As:` line that
  puts the release PR right.
* `guard-tag`: the same line rule at the tag (release-tag.yml), for a
  version set by hand or by an admin's push that the release PR never saw.
* `candidate`: which candidate Promote promotes. Named, it must be the
  newest candidate of its version, and the version unshipped. Blank takes
  the newest candidate when exactly one version has candidates out, and
  is refused when two lines do, naming both.
* `previous`: the stable version a promotion's notes and changelog start
  from: the newest stable below it. Refused when a newer stable exists,
  since the stable channels carry one version each and would be downgraded;
  that is the rule that only the newest line is maintained.
* `cut`: the line a stable tag starts (cut-release-branch.yml): its branch
  and its backport label. Refused for a candidate, an untagged version, a
  line that is cut already, a version with a newer patch (the line starts at
  its newest), and a version below the newest stable, by the same rule.
  Whether the tag's own workflows know lines is the workflow's check, since
  it needs git.

FILE holds one ref per line: bare tags, or `git ls-remote` output, whose
sha and `refs/tags/` are dropped and whose `^{}` entries are skipped. `-`
is stdin. Nothing here runs git; the workflows pass what they already read.

Exit codes:
    0  passed, or the self-test passed
    1  refused (the reason is printed), or the self-test failed
"""

from __future__ import annotations

import argparse
import re
import sys
from typing import Dict, List, Optional, Sequence, Tuple

VERSION = re.compile(r"^(\d+)\.(\d+)\.(\d+)(?:-rc\.(\d+))?$")
LINE_BRANCH = re.compile(r"^release/(\d+)\.(\d+)\.x$")
CHANNELS = "apt, dnf, pacman, the AUR, Homebrew and the docs site"


class Refusal(Exception):
    pass


# --- versions ---------------------------------------------------------------

Version = Tuple[int, int, int, Optional[int]]


def parse(text: str) -> Optional[Version]:
    """`X.Y.Z` or `X.Y.Z-rc.N`, with or without a leading v; None otherwise."""
    found = VERSION.match(text[1:] if text.startswith("v") else text)
    if not found:
        return None
    major, minor, patch, rc = found.groups()
    return int(major), int(minor), int(patch), (int(rc) if rc is not None else None)


def spell(version: Version) -> str:
    major, minor, patch, rc = version
    return f"{major}.{minor}.{patch}" + (f"-rc.{rc}" if rc is not None else "")


def stable_of(version: Version) -> Version:
    return version[0], version[1], version[2], None


def sort_key(version: Version) -> Tuple[int, int, int, int, int]:
    # A stable sorts after every candidate of its version.
    major, minor, patch, rc = version
    return major, minor, patch, (1 if rc is None else 0), (rc or 0)


def refs(lines: Sequence[str], prefix: str) -> List[str]:
    """Bare names from a file of refs, as `git tag -l` or `git ls-remote` write them."""
    names = []
    for raw in lines:
        line = raw.strip()
        if not line:
            continue
        name = line.split()[-1]
        if name.endswith("^{}"):
            continue
        if name.startswith(prefix):
            name = name[len(prefix) :]
        names.append(name)
    return names


def versions(tags: Sequence[str]) -> List[Version]:
    found = [parse(tag) for tag in tags if tag.startswith("v")]
    return sorted((v for v in found if v is not None), key=sort_key)


def stables(tags: Sequence[str]) -> List[Version]:
    return [v for v in versions(tags) if v[3] is None]


def candidates(tags: Sequence[str]) -> List[Version]:
    return [v for v in versions(tags) if v[3] is not None]


def shipped(version: Version, tags: Sequence[str]) -> bool:
    return stable_of(version) in stables(tags)


# --- lines ------------------------------------------------------------------

Line = Optional[Tuple[int, int]]


def line_of(branch: str) -> Line:
    if branch == "main":
        return None
    found = LINE_BRANCH.match(branch)
    if not found:
        raise Refusal(f"{branch} is not main or a release/X.Y.x branch; nothing else releases")
    return int(found.group(1)), int(found.group(2))


def line_name(line: Tuple[int, int]) -> str:
    return f"{line[0]}.{line[1]}.x"


def branch_of(line: Tuple[int, int]) -> str:
    return f"release/{line_name(line)}"


def in_line(version: Version, line: Tuple[int, int]) -> bool:
    return (version[0], version[1]) == line


def lines_with_branches(branches: Sequence[str]) -> List[Tuple[int, int]]:
    found = []
    for branch in branches:
        match = LINE_BRANCH.match(branch)
        if match:
            found.append((int(match.group(1)), int(match.group(2))))
    return found


def next_patch(line: Tuple[int, int], tags: Sequence[str]) -> Version:
    """The line's next patch: one above its newest shipped version."""
    on_line = [v for v in stables(tags) if in_line(v, line)]
    newest = on_line[-1] if on_line else (line[0], line[1], 0, None)
    return line[0], line[1], newest[2] + 1, None


def release_as(version: Version) -> str:
    return f"'Release-As: {spell(version)}-rc.1'"


REWRITTEN = ", and the release PR is rewritten to it."


# --- guard-pr ----------------------------------------------------------------


def guard_pr(branch: str, text: str, tags: Sequence[str], branches: Sequence[str]) -> Dict[str, str]:
    line = line_of(branch)
    version = parse(text)
    if version is None:
        raise Refusal(f"The release PR proposes '{text}', which is not a version X.Y.Z-rc.N")
    major, minor, patch, rc = version

    if line is not None:
        patch_hint = release_as(next_patch(line, tags))
        if rc is None:
            raise Refusal(
                f"The release PR proposes {text}, which is not a release candidate; a stable version comes only "
                f"from Promote. Merge a commit on {branch} whose message ends with {patch_hint}{REWRITTEN}"
            )
        if not in_line(version, line):
            raise Refusal(
                f"The release PR on {branch} proposes {text}, outside the {line_name(line)} line: a feat or a "
                f"breaking change was merged there. A feature does not belong on a maintenance line. Revert that "
                f"merge, or if its 'Release-As:' line is wrong merge a commit ending with {patch_hint}{REWRITTEN}"
            )
        if shipped(version, tags):
            raise Refusal(
                f"The release PR proposes {text}, but {spell(stable_of(version))} has shipped: Promote tagged "
                f"v{spell(stable_of(version))} from its candidate, so from {branch} the candidate still looks like "
                f"the last release. Merge a commit whose message ends with {patch_hint}{REWRITTEN}"
            )
        before = (major, minor, patch - 1, None)
        if patch == 0 or before not in stables(tags):
            raise Refusal(
                f"The release PR proposes {text}, but v{spell(before)} has not shipped; a line's patches go out "
                f"in order. Merge a commit whose message ends with {patch_hint}{REWRITTEN}"
            )
        return {"version": text, "line": line_name(line)}

    if rc is None:
        raise Refusal(
            f"The release PR proposes {text}, which is not a release candidate; a stable version comes only from "
            f"Promote. Merge a commit whose message ends with a 'Release-As: X.Y.Z-rc.1' line naming the "
            f"candidate meant{REWRITTEN}"
        )
    minor_hint = release_as((major, minor + 1, 0, None))
    branched = lines_with_branches(branches)
    if (major, minor) in branched:
        raise Refusal(
            f"The release PR proposes {text}, inside the {line_name((major, minor))} line, whose patches are cut on "
            f"{branch_of((major, minor))}, not on main. Merge a commit whose message ends with {minor_hint}"
            f"{REWRITTEN}"
        )
    if shipped(version, tags):
        stable = spell(stable_of(version))
        patch_hint = release_as((major, minor, patch + 1, None))
        raise Refusal(
            f"The release PR proposes {text}, but {stable} has shipped: Promote tagged v{stable} from its "
            f"candidate, so from main the candidate still looks like the last release. Merge a commit whose "
            f"message ends with {minor_hint} (a feat or a breaking change since {stable}) or {patch_hint} "
            f"(fixes only; or cut release/{major}.{minor}.x from v{stable} and backport them there){REWRITTEN}"
        )
    return {"version": text, "line": ""}


# --- guard-tag ---------------------------------------------------------------


def guard_tag(branch: str, text: str, branches: Sequence[str]) -> Dict[str, str]:
    line = line_of(branch)
    version = parse(text)
    if version is None:
        raise Refusal(f"'{text}' is not a version X.Y.Z or X.Y.Z-rc.N")
    major, minor = version[0], version[1]
    if line is not None:
        if not in_line(version, line):
            raise Refusal(
                f"{text} is not a version of the {line_name(line)} line, and {branch} tags nothing else. "
                f"Revert the merge that set it; nothing was tagged."
            )
        return {"version": text, "line": line_name(line)}
    if (major, minor) in lines_with_branches(branches):
        raise Refusal(
            f"{text} is inside the {line_name((major, minor))} line, whose versions are cut on "
            f"{branch_of((major, minor))}, not on main. Revert the merge that set it; nothing was tagged."
        )
    return {"version": text, "line": ""}


# --- candidate ---------------------------------------------------------------


def candidate(text: str, tags: Sequence[str]) -> Dict[str, str]:
    known = versions(tags)
    if text:
        chosen = parse(text)
        if not text.startswith("v") or chosen is None or chosen[3] is None:
            raise Refusal(f"{text} is not a candidate's tag, vX.Y.Z-rc.N")
        if chosen not in known:
            raise Refusal(
                f"{text} is not tagged. CI tags a candidate once it passes on its branch; wait for that, or "
                f"re-run CI's release job if it failed."
            )
    else:
        waiting: Dict[Version, Version] = {}
        for rc in candidates(tags):
            if stable_of(rc) not in known:
                waiting[stable_of(rc)] = rc  # sorted, so the last of a version wins
        if not candidates(tags):
            raise Refusal("no candidate is tagged. Merge the release PR to cut one, let it release, then run Promote.")
        if not waiting:
            raise Refusal(
                "no candidate is waiting: every candidate's version has shipped. Merge a release PR to cut one, "
                "let it release, then run Promote."
            )
        if len(waiting) > 1:
            names = ", ".join(f"v{spell(rc)}" for rc in waiting.values())
            raise Refusal(
                f"candidates of {len(waiting)} versions are out: {names}. Name the one to promote; blank takes "
                f"the newest only when one version has candidates."
            )
        chosen = next(iter(waiting.values()))
    stable = stable_of(chosen)
    newest = [v for v in candidates(tags) if stable_of(v) == stable][-1]
    if newest != chosen:
        raise Refusal(
            f"v{spell(chosen)} is not the newest candidate of {spell(stable)}; v{spell(newest)} is. A stable "
            f"release ships the last candidate: promote v{spell(newest)}, or leave the candidate blank for the "
            f"newest."
        )
    if stable in known:
        below = [v for v in stables(tags) if sort_key(v) < sort_key(stable)]
        before = f"v{spell(below[-1])}" if below else "<the stable version before it>"
        raise Refusal(
            f"{spell(stable)} has shipped already: v{spell(stable)} is tagged. If an earlier run tagged it and "
            f"then failed, finish by hand: gh release create v{spell(stable)} --title 'Release v{spell(stable)}' "
            f"--generate-notes --notes-start-tag {before} --verify-tag --latest, then gh workflow run "
            f"release.yml --ref main -f tag=v{spell(stable)}."
        )
    return {"candidate": f"v{spell(chosen)}", "stable": spell(stable)}


# --- previous ----------------------------------------------------------------


def previous(text: str, tags: Sequence[str]) -> Dict[str, str]:
    stable = parse(text)
    if stable is None or stable[3] is not None or text.startswith("v"):
        raise Refusal(f"'{text}' is not a stable version X.Y.Z")
    shipped_versions = stables(tags)
    newer = [v for v in shipped_versions if sort_key(v) > sort_key(stable)]
    if newer:
        raise Refusal(
            f"v{spell(newer[-1])} is the newest stable version; promoting {text} after it would downgrade "
            f"{CHANNELS}, which each carry one version. A line older than the newest stable is not maintained: "
            f"its fix ships in the newest line."
        )
    below = [v for v in shipped_versions if sort_key(v) < sort_key(stable)]
    if not below:
        raise Refusal(f"no stable version is tagged before {text}, so there is nothing for its notes to start from")
    return {"previous": f"v{spell(below[-1])}"}


# --- cut ---------------------------------------------------------------------


def cut(text: str, tags: Sequence[str], branches: Sequence[str]) -> Dict[str, str]:
    stable = parse(text)
    if not text.startswith("v") or stable is None or stable[3] is not None:
        raise Refusal(f"{text} is not a stable version's tag, vX.Y.Z; a line starts at a stable version")
    if stable not in versions(tags):
        raise Refusal(f"{text} is not tagged. Promote the version first; a line starts at its tag.")
    line = (stable[0], stable[1])
    if line in lines_with_branches(branches):
        raise Refusal(
            f"{branch_of(line)} exists already: the {line_name(line)} line is cut. Backport to it with the "
            f"'backport: {line_name(line)}' label on a merged pull request."
        )
    shipped_versions = stables(tags)
    newer = [v for v in shipped_versions if sort_key(v) > sort_key(stable)]
    on_line = [v for v in newer if in_line(v, line)]
    if on_line:
        raise Refusal(
            f"v{spell(on_line[-1])} is the newest {line_name(line)} version; a line starts at its newest, so "
            f"cut it from v{spell(on_line[-1])}."
        )
    if newer:
        raise Refusal(
            f"v{spell(newer[-1])} is the newest stable version; a line for {text} would ship below it, and a "
            f"line older than the newest stable is not maintained: {CHANNELS} each carry one version."
        )
    return {"branch": branch_of(line), "line": line_name(line), "label": f"backport: {line_name(line)}"}


# --- self-test ---------------------------------------------------------------

TODAY = ["v0.28.0", "v0.28.0-rc.5", "v0.29.0-rc.1", "v0.29.0-rc.7", "v0.29.0", "v0.30.0-rc.1", "v0.30.0-rc.2"]
# 0.30.0 shipped, a line cut for it, and main gone on to 0.31.0's candidates.
LINE = TODAY + ["v0.30.0-rc.3", "v0.30.0", "v0.31.0-rc.1"]
LINE_BRANCHES = ["release/0.30.x"]
# The line's first patch out.
PATCHED = LINE + ["v0.30.1-rc.1", "v0.30.1"]
# Interleaved: a patch on the line while main's next minor was out.
INTERLEAVED = ["v0.29.0", "v0.30.0-rc.1", "v0.29.1-rc.1", "v0.29.1", "v0.30.0"]
LS_REMOTE_TAGS = [
    "0123456789abcdef0123456789abcdef01234567\trefs/tags/v0.29.0",
    "89abcdef0123456789abcdef0123456789abcdef\trefs/tags/v0.29.0^{}",
    "",
    "fedcba9876543210fedcba9876543210fedcba98\trefs/tags/v0.30.0-rc.1",
]
LS_REMOTE_HEADS = ["fedcba9876543210fedcba9876543210fedcba98\trefs/heads/release/0.29.x"]


def self_test() -> bool:
    failures: List[str] = []

    def passes(name: str, got: Dict[str, str], want: Dict[str, str]) -> None:
        if got != want:
            failures.append(f"{name}: got {got}, want {want}")

    def refuses(name: str, run, *needles: str) -> None:
        try:
            got = run()
        except Refusal as refusal:
            missing = [needle for needle in needles if needle not in str(refusal)]
            if missing:
                failures.append(f"{name}: refused without {missing!r}: {refusal}")
            return
        failures.append(f"{name}: passed with {got}, and must refuse")

    # refs: `git ls-remote` output and bare names both read.
    if refs(LS_REMOTE_TAGS, "refs/tags/") != ["v0.29.0", "v0.30.0-rc.1"]:
        failures.append(f"refs: ls-remote tags read as {refs(LS_REMOTE_TAGS, 'refs/tags/')}")
    if refs(LS_REMOTE_HEADS, "refs/heads/") != ["release/0.29.x"]:
        failures.append(f"refs: ls-remote heads read as {refs(LS_REMOTE_HEADS, 'refs/heads/')}")
    if refs(["v0.29.0", "", "v0.30.0-rc.1"], "refs/tags/") != ["v0.29.0", "v0.30.0-rc.1"]:
        failures.append("refs: bare tags")

    # line
    passes("line: main", line({"branch": "main"}), {"line": ""})
    passes("line: a line", line({"branch": "release/0.30.x"}), {"line": "0.30.x"})
    refuses("line: a feature branch", lambda: line({"branch": "feat/thing"}), "nothing else releases")
    refuses("line: release/0.30 without .x", lambda: line({"branch": "release/0.30"}), "nothing else releases")

    # guard-pr on main, as today
    passes("pr main: today's cycle", guard_pr("main", "0.30.0-rc.3", TODAY, []), {"version": "0.30.0-rc.3", "line": ""})
    refuses("pr main: stable", lambda: guard_pr("main", "0.30.0", TODAY, []), "not a release candidate", "Release-As: X.Y.Z-rc.1")
    refuses(
        "pr main: rc of a shipped version",
        lambda: guard_pr("main", "0.29.0-rc.8", TODAY, []),
        "0.29.0 has shipped", "'Release-As: 0.30.0-rc.1'", "'Release-As: 0.29.1-rc.1'", "cut release/0.29.x",
    )
    refuses("pr main: not a version", lambda: guard_pr("main", "banana", TODAY, []), "not a version")
    # guard-pr on main beside a line
    passes("pr main: next minor beside a line", guard_pr("main", "0.31.0-rc.2", LINE, LINE_BRANCHES), {"version": "0.31.0-rc.2", "line": ""})
    refuses(
        "pr main: a patch of a branched line",
        lambda: guard_pr("main", "0.30.1-rc.1", LINE, LINE_BRANCHES),
        "inside the 0.30.x line", "release/0.30.x", "'Release-As: 0.31.0-rc.1'",
    )
    passes("pr main: a patch of an unbranched line", guard_pr("main", "0.30.1-rc.1", LINE, []), {"version": "0.30.1-rc.1", "line": ""})
    # guard-pr on a line
    passes("pr line: first patch", guard_pr("release/0.30.x", "0.30.1-rc.1", LINE, LINE_BRANCHES), {"version": "0.30.1-rc.1", "line": "0.30.x"})
    passes("pr line: first patch's rc.2", guard_pr("release/0.30.x", "0.30.1-rc.2", LINE + ["v0.30.1-rc.1"], LINE_BRANCHES), {"version": "0.30.1-rc.2", "line": "0.30.x"})
    refuses(
        "pr line: rc of the shipped patch",
        lambda: guard_pr("release/0.30.x", "0.30.1-rc.2", PATCHED, LINE_BRANCHES),
        "0.30.1 has shipped", "'Release-As: 0.30.2-rc.1'",
    )
    passes("pr line: second patch", guard_pr("release/0.30.x", "0.30.2-rc.1", PATCHED, LINE_BRANCHES), {"version": "0.30.2-rc.1", "line": "0.30.x"})
    refuses(
        "pr line: a feat cherry-picked",
        lambda: guard_pr("release/0.30.x", "0.31.0-rc.1", LINE, LINE_BRANCHES),
        "outside the 0.30.x line", "does not belong on a maintenance line", "'Release-As: 0.30.1-rc.1'",
    )
    refuses(
        "pr line: a patch skipped",
        lambda: guard_pr("release/0.30.x", "0.30.3-rc.1", PATCHED, LINE_BRANCHES),
        "v0.30.2 has not shipped", "'Release-As: 0.30.2-rc.1'",
    )
    refuses(
        "pr line: stable",
        lambda: guard_pr("release/0.30.x", "0.30.1", LINE, LINE_BRANCHES),
        "not a release candidate", "'Release-As: 0.30.1-rc.1'",
    )
    refuses("pr line: a .0 candidate", lambda: guard_pr("release/0.30.x", "0.30.0-rc.4", LINE, LINE_BRANCHES), "0.30.0 has shipped")

    # guard-tag
    passes("tag main: today", guard_tag("main", "0.30.0-rc.3", []), {"version": "0.30.0-rc.3", "line": ""})
    passes("tag main: stable by hand", guard_tag("main", "0.30.0", []), {"version": "0.30.0", "line": ""})
    refuses("tag main: inside a branched line", lambda: guard_tag("main", "0.30.1-rc.1", LINE_BRANCHES), "inside the 0.30.x line", "nothing was tagged")
    passes("tag main: next minor beside a line", guard_tag("main", "0.31.0-rc.1", LINE_BRANCHES), {"version": "0.31.0-rc.1", "line": ""})
    passes("tag line: a patch", guard_tag("release/0.30.x", "0.30.1-rc.1", LINE_BRANCHES), {"version": "0.30.1-rc.1", "line": "0.30.x"})
    refuses("tag line: another line's version", lambda: guard_tag("release/0.30.x", "0.31.0-rc.1", LINE_BRANCHES), "not a version of the 0.30.x line", "nothing was tagged")
    refuses("tag: a feature branch", lambda: guard_tag("feat/thing", "0.30.1-rc.1", []), "nothing else releases")

    # candidate
    passes("candidate: blank, one version out", candidate("", TODAY), {"candidate": "v0.30.0-rc.2", "stable": "0.30.0"})
    passes("candidate: named newest", candidate("v0.30.0-rc.2", TODAY), {"candidate": "v0.30.0-rc.2", "stable": "0.30.0"})
    refuses("candidate: named older", lambda: candidate("v0.30.0-rc.1", TODAY), "not the newest candidate of 0.30.0", "v0.30.0-rc.2 is")
    refuses("candidate: named shipped", lambda: candidate("v0.29.0-rc.7", TODAY), "0.29.0 has shipped already", "--notes-start-tag v0.28.0", "--latest")
    refuses("candidate: not a tag", lambda: candidate("0.30.0-rc.2", TODAY), "not a candidate's tag")
    refuses("candidate: a stable tag", lambda: candidate("v0.30.0", TODAY), "not a candidate's tag")
    refuses("candidate: untagged", lambda: candidate("v0.30.0-rc.9", TODAY), "not tagged")
    refuses("candidate: blank, none tagged", lambda: candidate("", ["v0.1.0"]), "no candidate is tagged")
    refuses("candidate: blank, all shipped", lambda: candidate("", ["v0.29.0-rc.1", "v0.29.0"]), "every candidate's version has shipped")
    two_out = LINE + ["v0.30.1-rc.1"]
    refuses("candidate: blank, two lines out", lambda: candidate("", two_out), "candidates of 2 versions are out", "v0.30.1-rc.1", "v0.31.0-rc.1", "Name the one")
    passes("candidate: named, two lines out", candidate("v0.30.1-rc.1", two_out), {"candidate": "v0.30.1-rc.1", "stable": "0.30.1"})
    passes("candidate: blank, line only", candidate("", LINE[:-1] + ["v0.30.1-rc.1"]), {"candidate": "v0.30.1-rc.1", "stable": "0.30.1"})

    # previous
    passes("previous: today", previous("0.30.0", TODAY), {"previous": "v0.29.0"})
    passes("previous: a line's patch while main's minor is out", previous("0.29.1", ["v0.29.0", "v0.30.0-rc.1", "v0.29.1-rc.1"]), {"previous": "v0.29.0"})
    passes("previous: interleaved, the minor after the patch", previous("0.30.0", INTERLEAVED), {"previous": "v0.29.1"})
    refuses("previous: a downgrade", lambda: previous("0.29.2", INTERLEAVED), "v0.30.0 is the newest stable", "downgrade", "not maintained")
    refuses("previous: nothing before", lambda: previous("0.1.0", ["v0.1.0-rc.1"]), "no stable version is tagged before")
    refuses("previous: with a v", lambda: previous("v0.30.0", TODAY), "not a stable version")
    refuses("previous: a candidate", lambda: previous("0.30.0-rc.1", TODAY), "not a stable version")

    # cut
    passes("cut: today's stable", cut("v0.29.0", TODAY, []), {"branch": "release/0.29.x", "line": "0.29.x", "label": "backport: 0.29.x"})
    passes("cut: the newest patch of a line", cut("v0.29.1", INTERLEAVED[:-1], []), {"branch": "release/0.29.x", "line": "0.29.x", "label": "backport: 0.29.x"})
    refuses("cut: a candidate", lambda: cut("v0.30.0-rc.2", TODAY, []), "not a stable version's tag")
    refuses("cut: without a v", lambda: cut("0.29.0", TODAY, []), "not a stable version's tag")
    refuses("cut: untagged", lambda: cut("v0.30.0", TODAY, []), "not tagged", "Promote the version first")
    refuses("cut: the line exists", lambda: cut("v0.30.0", LINE, LINE_BRANCHES), "release/0.30.x exists already", "'backport: 0.30.x'")
    refuses("cut: an older patch of the line", lambda: cut("v0.30.0", PATCHED, []), "v0.30.1 is the newest 0.30.x version", "cut it from v0.30.1")
    refuses("cut: below the newest stable", lambda: cut("v0.29.1", INTERLEAVED, []), "v0.30.0 is the newest stable", "not maintained")

    if not failures:
        print("✓ release_lines.py self-test: the line rules hold on main, on a line and between them", file=sys.stderr)
        return True
    print("✗ release_lines.py self-test failed:", file=sys.stderr)
    for failure in failures:
        print(f"    {failure}", file=sys.stderr)
    return False


# --- command line ------------------------------------------------------------


def line(args) -> Dict[str, str]:
    found = line_of(args["branch"])
    return {"line": line_name(found) if found else ""}


def read_refs(path: Optional[str], prefix: str) -> List[str]:
    if path is None:
        return []
    if path == "-":
        return refs(sys.stdin.read().splitlines(), prefix)
    with open(path, encoding="utf-8") as handle:
        return refs(handle.read().splitlines(), prefix)


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--self-test", action="store_true", help="run the fixtures only")
    commands = parser.add_subparsers(dest="command")
    sub = commands.add_parser("line", help="which line a branch releases")
    sub.add_argument("--branch", required=True)
    sub = commands.add_parser("guard-pr", help="the release PR's version")
    sub.add_argument("--branch", required=True, help="the release PR's base branch")
    sub.add_argument("--version", required=True)
    sub.add_argument("--tags", required=True, help="a file of tags, or - for stdin")
    sub.add_argument("--branches", help="a file of branches")
    sub = commands.add_parser("guard-tag", help="the version about to be tagged")
    sub.add_argument("--branch", required=True, help="the branch being tagged")
    sub.add_argument("--version", required=True)
    sub.add_argument("--branches", required=True, help="a file of branches, or - for stdin")
    sub = commands.add_parser("candidate", help="which candidate Promote promotes")
    sub.add_argument("--input", default="", help="the candidate named, vX.Y.Z-rc.N, or blank")
    sub.add_argument("--tags", required=True, help="a file of tags, or - for stdin")
    sub = commands.add_parser("previous", help="the stable version a promotion's notes start from")
    sub.add_argument("--stable", required=True, help="the version being promoted, X.Y.Z")
    sub.add_argument("--tags", required=True, help="a file of tags, or - for stdin")
    sub = commands.add_parser("cut", help="the line a stable tag starts")
    sub.add_argument("--stable", required=True, help="the stable version's tag, vX.Y.Z")
    sub.add_argument("--tags", required=True, help="a file of tags, or - for stdin")
    sub.add_argument("--branches", required=True, help="a file of branches")
    args = parser.parse_args(argv)

    if args.self_test:
        return 0 if self_test() else 1
    if args.command is None:
        parser.print_usage(sys.stderr)
        return 2
    try:
        if args.command == "line":
            out = line({"branch": args.branch})
        elif args.command == "guard-pr":
            out = guard_pr(args.branch, args.version, read_refs(args.tags, "refs/tags/"), read_refs(args.branches, "refs/heads/"))
        elif args.command == "guard-tag":
            out = guard_tag(args.branch, args.version, read_refs(args.branches, "refs/heads/"))
        elif args.command == "candidate":
            out = candidate(args.input, read_refs(args.tags, "refs/tags/"))
        elif args.command == "cut":
            out = cut(args.stable, read_refs(args.tags, "refs/tags/"), read_refs(args.branches, "refs/heads/"))
        else:
            out = previous(args.stable, read_refs(args.tags, "refs/tags/"))
    except Refusal as refusal:
        print(f"::error::{refusal}", file=sys.stderr)
        return 1
    for key, value in out.items():
        print(f"{key}={value}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
