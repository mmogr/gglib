#!/usr/bin/env python3
"""Collect what a llama.cpp release changes for gglib.

Compares the pinned llama.cpp release (`PINNED_LLAMA_RELEASE` in
crates/gglib-runtime/src/llama/download/mod.rs) with a candidate release,
upstream's newest tag unless one is named, and writes the evidence a bump
needs to decide on, so the decision is read off a page rather than
re-derived from 1,000 commits:

  - the upstream commits between the two tags, filtered to the subsystems
    gglib's behaviour depends on;
  - `llama-server --help` at both tags, diffed, with the flags that came
    and went;
  - a launch of the candidate's `llama-server` with the flags gglib emits,
    on a 1 MB model, up to a 200 on `/health`, with `/props` kept;
  - the diff of each upstream file an ADR cites, with the ADR that cites it;
  - whether the release assets gglib downloads exist under the candidate;
  - `struct common_params_sampling`, diffed, the defaults ADR 0003 defers to.

Deterministic only. The verdict, whether gglib must change and how, is a
reading of this evidence, and llama-upstream.yml asks for it separately.

Needs a clone of ggml-org/llama.cpp with its tags; a partial one
(`git clone --filter=tree:0 --bare`) is enough, since every read is a
`git show` or `git diff` that fetches what it touches. Nothing here calls
the GitHub API: the newest tag is the largest `b<digits>` tag in the clone,
and an asset's existence is a HEAD request on its download URL.

Usage:
  scripts/llama_upstream.py --repo <llama.cpp clone> --out <dir>
                            [--candidate <tag>|latest] [--skip-binaries]

Writes to --out:
  facts.json    machine-readable summary (the workflow reads `up_to_date`)
  report.md     the issue body's evidence section, bounded in size
  evidence/     the full commit list, both --help texts, every diff, the
                launch logs and /props, for the artifact and the reader

Exit 0 when the evidence was collected, whether or not anything changed;
non-zero only when it could not be.
"""

from __future__ import annotations

import argparse
import difflib
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PIN_FILE = ROOT / "crates/gglib-runtime/src/llama/download/mod.rs"
COMMAND_FILE = ROOT / "crates/gglib-runtime/src/command.rs"
RELEASE_URL = "https://github.com/ggml-org/llama.cpp/releases/download/{tag}/llama-{tag}-{asset}"
LINUX_ASSET = "bin-ubuntu-x64.tar.gz"
# llama.cpp's own CI model: a 260K-parameter TinyStories checkpoint, about
# 1 MB, enough to load, serve and answer /health. Moves rarely; the URL is
# followed through redirects.
TINY_MODEL_URL = "https://huggingface.co/ggml-org/models/resolve/main/tinyllamas/stories260K.gguf"

# Upstream subsystems whose commits bear on gglib, by the prefix llama.cpp
# puts before the colon. Backends (metal, vulkan, cuda, ...), ci and the
# bundled web ui are left out: a change there reaches gglib only through
# the launch, which the smoke launch covers.
RELEVANT_PREFIX = re.compile(
    r"^(server|chat|chat-peg-parser|common|common/peg|common/json-schema|mtmd|jinja|grammar|"
    r"llama-grammar|sampling|llama|model|models|vocab|spec|kv-cache|kv-cells|memory|context|"
    r"arg|args|llama-mmap|tools|peg|json-schema)\s*[:(]",
    re.IGNORECASE,
)

# Upstream files whose contents an ADR or a doc reasons from. A change in one
# of them is what reopens the question that ADR settled, so each diff goes in
# the report under the documents that cite it.
CITED_FILES = {
    "common/arg.cpp": ["the launch flags command.rs emits"],
    "common/common.h": ["ADR 0003 (sampler defaults)"],
    "common/common.cpp": ["ADR 0003 (sampler defaults, common_init_sampler_from_model)"],
    "common/jinja/caps.h": ["ADR 0007 (chat_template_caps)"],
    "common/jinja/caps.cpp": ["ADR 0007 (chat_template_caps)"],
    "tools/server/server-common.cpp": [
        "ADR 0005 (grammar_lazy / grammar_triggers / preserved_tokens overwrite)",
        "ADR 0007 (reasoning_effort handling)",
    ],
    "tools/server/server-context.cpp": [
        "ADR 0003 and ADR 0004 (/props renders the sampler defaults)",
        "ADR 0007 (/props carries chat_template_caps)",
        "ADR 0015 (context shift and cache reuse with a projector)",
    ],
    "tools/server/README.md": ["the HTTP API gglib's proxy forwards to"],
}

# Flags command.rs emits, grouped by how the smoke launch proves them. A flag
# in neither group fails the collection loudly: a new launch flag must be
# placed here before the report can vouch for it.
LAUNCH_CHAT = [
    "--host", "127.0.0.1", "--port", "{port}", "--metrics",
    "--parallel", "1", "-c", "256",
    "--jinja", "--reasoning-format", "none",
    "--slot-save-path", "{slots}",
    "--cache-ram", "64", "--cache-reuse", "64",
    "--cache-type-k", "f16", "--cache-type-v", "f16",
    "--load-mode", "mmap+mlock",
]
LAUNCH_EMBEDDINGS = ["--host", "127.0.0.1", "--port", "{port}", "--embeddings", "--no-jinja"]
# Need a projector or a draft model to launch with; proven by their presence
# in --help instead.
HELP_ONLY = ["--mmproj", "--spec-type", "--spec-draft-n-max", "--spec-draft-p-min"]
ALWAYS = ["-m"]

# The issue body has to stay under GitHub's 65,536 characters with the verdict
# added, so each section of report.md is bounded; evidence/ holds the whole.
MAX_COMMITS_PER_GROUP = 25
MAX_DIFF_LINES = 120


def run(args: list[str], **kw) -> str:
    return subprocess.run(args, check=True, capture_output=True, text=True, **kw).stdout


def git(repo: Path, *args: str) -> str:
    return run(["git", "-C", str(repo), *args])


def read_pin() -> str:
    m = re.search(r'PINNED_LLAMA_RELEASE: &str = "(b\d+)"', PIN_FILE.read_text())
    if not m:
        sys.exit(f"no PINNED_LLAMA_RELEASE in {PIN_FILE}")
    return m.group(1)


def read_asset_patterns() -> list[str]:
    return sorted(set(re.findall(r'asset_pattern: "([^"]+)"', PIN_FILE.read_text())))


def read_emitted_flags() -> list[str]:
    return sorted(set(re.findall(r'\.arg\("(-{1,2}[a-z][a-z0-9-]*)"\)', COMMAND_FILE.read_text())))


def build_number(tag: str) -> int:
    return int(tag[1:])


def latest_tag(repo: Path) -> str:
    tags = [t for t in git(repo, "tag", "--list", "b*").split() if re.fullmatch(r"b\d+", t)]
    if not tags:
        sys.exit("the clone has no b<digits> tags")
    return max(tags, key=build_number)


def tag_date(repo: Path, tag: str) -> str:
    return git(repo, "log", "-1", "--format=%cs", tag).strip()


def download(url: str, dest: Path) -> bool:
    req = urllib.request.Request(url, headers={"User-Agent": "gglib-llama-upstream"})
    try:
        with urllib.request.urlopen(req, timeout=120) as resp, dest.open("wb") as out:
            shutil.copyfileobj(resp, out)
        return True
    except urllib.error.HTTPError as e:
        print(f"  {url}: HTTP {e.code}", file=sys.stderr)
        return False


def exists(url: str) -> bool:
    req = urllib.request.Request(url, method="HEAD", headers={"User-Agent": "gglib-llama-upstream"})
    try:
        with urllib.request.urlopen(req, timeout=60):
            return True
    except urllib.error.HTTPError as e:
        return e.code in (301, 302, 307, 308)


def fetch_server(tag: str, into: Path) -> Path | None:
    archive = into / f"{tag}.tar.gz"
    if not download(RELEASE_URL.format(tag=tag, asset=LINUX_ASSET), archive):
        return None
    with tarfile.open(archive) as tar:
        tar.extractall(into / tag, filter="data")
    for p in (into / tag).rglob("llama-server"):
        p.chmod(0o755)
        return p
    return None


def help_text(server: Path) -> str:
    r = subprocess.run([str(server), "--help"], capture_output=True, text=True,
                       env={**os.environ, "LD_LIBRARY_PATH": str(server.parent)})
    return r.stdout + r.stderr


def help_flags(text: str) -> set[str]:
    """Every long flag `--help` lists, aliases included: the dash-led tokens a
    line opens with, up to its first word that is not one."""
    flags: set[str] = set()
    for line in text.splitlines():
        for token in re.split(r"[,\s]+", line.strip()):
            if not token.startswith("-"):
                break
            if re.fullmatch(r"--[a-z][a-z0-9-]*", token):
                flags.add(token)
    return flags


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def launch(server: Path, model: Path, flags: list[str], workdir: Path, log: Path) -> tuple[bool, dict | None]:
    port = free_port()
    slots = workdir / "slots"
    slots.mkdir(exist_ok=True)
    argv = [str(server), *ALWAYS, str(model)] + [
        f.format(port=port, slots=slots) for f in flags
    ]
    env = {**os.environ, "LD_LIBRARY_PATH": str(server.parent)}
    with log.open("w") as out:
        out.write(" ".join(argv) + "\n\n")
        out.flush()
        proc = subprocess.Popen(argv, stdout=out, stderr=subprocess.STDOUT, env=env)
        try:
            deadline = time.time() + 90
            props = None
            while time.time() < deadline:
                if proc.poll() is not None:
                    return False, None
                try:
                    with urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=2) as r:
                        if r.status == 200:
                            with urllib.request.urlopen(f"http://127.0.0.1:{port}/props", timeout=5) as p:
                                props = json.load(p)
                            return True, props
                except (urllib.error.URLError, ConnectionError, TimeoutError):
                    time.sleep(0.5)
            return False, None
        finally:
            proc.terminate()
            try:
                proc.wait(10)
            except subprocess.TimeoutExpired:
                proc.kill()


def sampling_struct(repo: Path, tag: str) -> str:
    try:
        src = git(repo, "show", f"{tag}:common/common.h")
    except subprocess.CalledProcessError:
        return ""
    m = re.search(r"struct common_params_sampling \{.*?\n\};", src, re.DOTALL)
    # Column alignment moves whenever a longer member lands; a default does not.
    return re.sub(r"[ \t]+", " ", m.group(0)) if m else ""


def bounded(lines: list[str], limit: int, more: str) -> str:
    if len(lines) <= limit:
        return "\n".join(lines)
    return "\n".join(lines[:limit]) + f"\n… {len(lines) - limit} more; {more}"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--repo", required=True, type=Path, help="a clone of ggml-org/llama.cpp with its tags")
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--candidate", default="latest", help="a b<digits> tag, or latest (the default)")
    ap.add_argument("--skip-binaries", action="store_true",
                    help="no download, no --help diff, no launch; the git-derived evidence only")
    a = ap.parse_args()

    out: Path = a.out
    ev = out / "evidence"
    ev.mkdir(parents=True, exist_ok=True)
    repo: Path = a.repo

    pin = read_pin()
    candidate = latest_tag(repo) if a.candidate == "latest" else a.candidate
    if not re.fullmatch(r"b\d+", candidate):
        sys.exit(f"candidate must be b<digits>, got {candidate}")
    facts: dict = {"pin": pin, "candidate": candidate, "up_to_date": build_number(candidate) <= build_number(pin)}
    facts["pin_date"] = tag_date(repo, pin)
    facts["candidate_date"] = tag_date(repo, candidate)
    print(f"pin {pin} ({facts['pin_date']}), candidate {candidate} ({facts['candidate_date']})")
    if facts["up_to_date"]:
        (out / "facts.json").write_text(json.dumps(facts, indent=2) + "\n")
        (out / "report.md").write_text(f"The pin, `{pin}`, is upstream's newest release.\n")
        return 0

    # Commits between the tags.
    log = git(repo, "log", "--format=%h %s", f"{pin}..{candidate}").splitlines()
    (ev / "commits.txt").write_text("\n".join(log) + "\n")
    relevant = [l for l in log if RELEVANT_PREFIX.match(l.split(" ", 1)[1])]
    facts["commits_total"] = len(log)
    facts["commits_relevant"] = len(relevant)
    groups: dict[str, list[str]] = {}
    for line in relevant:
        subject = line.split(" ", 1)[1]
        key = re.split(r"\s*[:(]", subject, maxsplit=1)[0].lower()
        groups.setdefault(key, []).append(line)

    # Files the ADRs cite.
    changed: dict[str, dict] = {}
    for path, cites in CITED_FILES.items():
        diff = git(repo, "diff", pin, candidate, "--", path)
        if not diff:
            continue
        (ev / f"{path.replace('/', '__')}.diff").write_text(diff)
        stat = git(repo, "diff", "--numstat", pin, candidate, "--", path).split()
        changed[path] = {"insertions": int(stat[0]), "deletions": int(stat[1]), "cited_by": cites}
    facts["cited_files_changed"] = changed

    # Sampler defaults.
    before, after = sampling_struct(repo, pin), sampling_struct(repo, candidate)
    sampling_diff = "".join(difflib.unified_diff(
        before.splitlines(True), after.splitlines(True), f"{pin}:common/common.h", f"{candidate}:common/common.h"))
    facts["sampling_struct_changed"] = bool(sampling_diff)
    if sampling_diff:
        (ev / "common_params_sampling.diff").write_text(sampling_diff)

    # Release assets gglib downloads.
    patterns = read_asset_patterns()
    facts["assets_missing"] = [p for p in patterns if not exists(RELEASE_URL.format(tag=candidate, asset=p))]

    # The launch flags command.rs emits, each accounted for.
    emitted = read_emitted_flags()
    known = {f for f in LAUNCH_CHAT + LAUNCH_EMBEDDINGS + HELP_ONLY + ALWAYS if f.startswith("-")}
    facts["unlisted_flags"] = sorted(set(emitted) - known)

    # Binaries: --help at both tags, and the candidate launched.
    facts["help_flags_added"] = facts["help_flags_removed"] = None
    facts["launch"] = None
    if not a.skip_binaries:
        with tempfile.TemporaryDirectory(prefix="llama-upstream-") as tmp_s:
            tmp = Path(tmp_s)
            servers = {t: fetch_server(t, tmp) for t in (pin, candidate)}
            helps = {t: help_text(s) if s else "" for t, s in servers.items()}
            for t, h in helps.items():
                (ev / f"help-{t}.txt").write_text(h)
            if all(helps.values()):
                hd = "".join(difflib.unified_diff(
                    helps[pin].splitlines(True), helps[candidate].splitlines(True),
                    f"{pin} --help", f"{candidate} --help"))
                (ev / "help.diff").write_text(hd)
                facts["help_flags_added"] = sorted(help_flags(helps[candidate]) - help_flags(helps[pin]))
                facts["help_flags_removed"] = sorted(help_flags(helps[pin]) - help_flags(helps[candidate]))
            server = servers[candidate]
            model = tmp / "tiny.gguf"
            if server and download(TINY_MODEL_URL, model):
                results = {}
                for name, flags in (("chat", LAUNCH_CHAT), ("embeddings", LAUNCH_EMBEDDINGS)):
                    ok, props = launch(server, model, flags, tmp, ev / f"launch-{name}.log")
                    results[name] = ok
                    if props is not None:
                        (ev / f"props-{name}.json").write_text(json.dumps(props, indent=2) + "\n")
                present = help_flags(helps[candidate])
                results["help_only_missing"] = [f for f in HELP_ONLY if f not in present]
                facts["launch"] = results
            else:
                facts["launch"] = {"error": "no candidate binary or no model to launch it on"}

    (out / "facts.json").write_text(json.dumps(facts, indent=2) + "\n")

    # The report.
    r: list[str] = []
    r.append(f"Pin `{pin}` ({facts['pin_date']}) → candidate `{candidate}` ({facts['candidate_date']}): "
             f"{len(log)} upstream commits, {len(relevant)} in subsystems gglib depends on.")
    r.append("")
    r.append("### Launch")
    r.append("")
    if facts["launch"] is None:
        r.append("Not run (binaries skipped).")
    elif "error" in facts["launch"]:
        r.append(f"Could not run: {facts['launch']['error']}.")
    else:
        L = facts["launch"]
        r.append(f"- chat flags: {'**launched, /health 200**' if L['chat'] else '**failed** (see `launch-chat.log`)'}")
        r.append(f"- embeddings flags: {'**launched, /health 200**' if L['embeddings'] else '**failed** (see `launch-embeddings.log`)'}")
        miss = L["help_only_missing"]
        r.append(f"- in `--help` only: {', '.join(f'`{f}`' for f in HELP_ONLY)} — "
                 + ("all present" if not miss else "**missing: " + ", ".join(f"`{f}`" for f in miss) + "**"))
    if facts["unlisted_flags"]:
        r.append(f"- **command.rs emits flags this script does not cover: "
                 + ", ".join(f"`{f}`" for f in facts["unlisted_flags"]) + "** — add them to scripts/llama_upstream.py")
    r.append("")
    r.append("### Flags")
    r.append("")
    if facts["help_flags_added"] is None:
        r.append("No `--help` diff (binaries skipped or a download failed).")
    else:
        r.append("- removed: " + (", ".join(f"`{f}`" for f in facts["help_flags_removed"]) or "none"))
        r.append("- added: " + (", ".join(f"`{f}`" for f in facts["help_flags_added"]) or "none"))
        r.append("- the full `--help` diff is `help.diff` in the artifact")
    r.append("")
    r.append("### Release assets")
    r.append("")
    r.append("All present." if not facts["assets_missing"] else
             "**Missing: " + ", ".join(f"`{p}`" for p in facts["assets_missing"]) + "**")
    r.append("")
    r.append("### Sampler defaults (`common_params_sampling`)")
    r.append("")
    if sampling_diff:
        r.append("```diff")
        r.append(bounded(sampling_diff.splitlines(), MAX_DIFF_LINES, "see `common_params_sampling.diff`"))
        r.append("```")
    else:
        r.append("Unchanged.")
    r.append("")
    r.append("### Files the ADRs cite")
    r.append("")
    if not changed:
        r.append("None changed.")
    for path, c in changed.items():
        r.append(f"- `{path}` +{c['insertions']} −{c['deletions']} — cited by " + "; ".join(c["cited_by"]))
    r.append("")
    r.append("### Upstream commits in subsystems gglib depends on")
    r.append("")
    for key in sorted(groups, key=lambda k: -len(groups[k])):
        r.append(f"<details><summary><code>{key}</code> ({len(groups[key])})</summary>")
        r.append("")
        r.append(bounded([f"- {l}" for l in groups[key]], MAX_COMMITS_PER_GROUP, "see `commits.txt`"))
        r.append("")
        r.append("</details>")
    r.append("")
    (out / "report.md").write_text("\n".join(r) + "\n")
    print(f"wrote {out / 'report.md'} ({(out / 'report.md').stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
