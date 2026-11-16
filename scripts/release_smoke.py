#!/usr/bin/env python3
"""The built release, installed as a user installs it, answers a chat.

Usage:
    ./scripts/release_smoke.py run --artifacts DIR --version V --sha SHA --models DIR --work DIR
    ./scripts/release_smoke.py --self-test

`run` is release.yml's `smoke` job, once per OS. It takes one target's built
release from DIR, the files `build` and `build-gui` uploaded, and walks the
path from the installer to a first reply, the path #1959 found broken on
Windows by hand:

 1. no gglib program is running yet;
 2. the release is installed the way a user installs it (PLATFORMS says how
    on each OS), and the installed `gglib --version` names the version and the
    commit being released;
 3. the CLI archive unpacks and its `gglib --version` says the same;
 4. where the OS keeps a user's own PATH (Windows), the install put its
    folder on it exactly once and left every other entry as it was, read
    from the registry a new terminal's PATH is built from;
 5. `gglib config paths` reads a data folder of the smoke's own, under
    --work, through GGLIB_DATA_DIR: nothing of the runner's or a person's;
 6. `gglib config llama install` installs the pinned engine, and
    `gglib config llama status` says it is functional;
 7. `gglib model add` adds tinygemma3 from the pinned test models (--models,
    the `pinned-engines` action's folder), and `gglib model list` gives
    its ID, which `add` does not print;
 8. `gglib up -y --model <id> --port <port>` starts the daemon and the
    endpoint as a user's first command does;
 9. one chat request to the endpoint gets a reply with text in it;
10. `gglib daemon stop`, then no gglib, gglib-gui, llama-server or sd-server
    process is left, and no lock or pid file is left in the data folder;
11. uninstalling, where PLATFORMS says how, removes what was installed and
    leaves no gglib process, and the user's own PATH is again what it was
    before the install. Steps 10 and 11 run after an earlier failure too;
    an uninstall that then fails is reported, not counted as the cause.

A step PLATFORMS lists under `pending` is expected to fail on that OS until
the change it names merges: its failure is a warning, and its passing a
notice to take it off the list. None is listed now, and the summary says so.

Every command has a time limit and its stdout and stderr go to files under
--work/logs, which the job uploads whether or not the run passed, with the
daemon's own logs. The log says which step failed and why, each failure
also as a GitHub error annotation, and a summary goes to the job's page.

What differs between the three OSes is data, in PLATFORMS: the artifacts'
names, how a user installs and uninstalls them, where the installed `gglib`
lands, how processes are listed. The steps are one list for every OS.

The installs write outside --work (/Applications, /usr/bin, %LOCALAPPDATA%),
so `run` refuses to start outside GitHub Actions unless given --here.

`--self-test` checks the pure parts: that PLATFORMS has a complete entry for
each target release.yml builds and names no value the run does not give;
that the leftover-process check finds a planted gglib program and ignores
everything else; that the PATH checks refuse a folder added twice or not at
all, another entry changed, and a value not given back whole; that the reply check refuses an empty reply and an error
body; that the version check refuses another version or commit; and the
readers of a process list, of `gglib config paths`, of `gglib model list`
(on what `model add` printed on Windows, which has no ID, and on a list
with spaces in its paths), of what `gglib up` says, and of the data folder.

Exit codes:
    0  every step passed, or the self-test passed
    1  a step failed, or the self-test failed
    2  the arguments were refused
"""

from __future__ import annotations

import argparse
import csv
import io
import json
import os
import re
import shutil
import string
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
import zipfile
from collections.abc import Callable, Mapping
from dataclasses import dataclass, fields, replace

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# The programs a gglib install runs. None may outlive `gglib daemon stop`.
WATCHED = ("gglib", "gglib-gui", "llama-server", "sd-server")

# The pinned test model asked the question: tinygemma3 carries its own chat
# template, and the engine recorder holds its greedy reply to this question
# (contracts/upstream/llama/observed.json, greedy.tinygemma3.*).
MODEL = "tinygemma3-Q8_0.gguf"
QUESTION = {
    "messages": [{"role": "user", "content": "What is the weather in Paris?"}],
    "max_tokens": 16,
    "temperature": 0,
    "seed": 1,
}

# Ports of the smoke's own, away from gglib's defaults.
DAEMON_PORT = 19487
ENDPOINT_PORT = 19488

# Each command's time limit, in seconds. A command that hangs is stopped at
# its limit, and the run goes on to stop the daemon, check what was left and
# keep every output, so one hang fails the step with its logs. The workflow
# step's own limit is the backstop for more than one. A command not named
# here gets `short`.
LIMITS = {
    "short": 60,
    "install": 900,
    "engine": 900,
    "model": 120,
    # `up` also downloads the drawing engine, which it may skip.
    "up": 1200,
    "reply": 600,
    "stop": 180,
    "uninstall": 300,
    "gone": 180,
}

# The values a PLATFORMS template may name, and nothing else.
VALUES = ("version", "target", "artifacts", "work", "LOCALAPPDATA")
# Those of them that are folders.
FOLDERS = {"artifacts", "work", "LOCALAPPDATA"}


@dataclass(frozen=True)
class Platform:
    """One OS's facts. Templates take `{name}` for each name in VALUES."""

    # The Rust target release.yml builds for this OS.
    target: str
    # What is installed, and why that and not something else.
    how: str
    # The CLI archive a user unpacks, and the program in it.
    cli_archive: str
    cli_program: str
    # The commands that install the built release as a user would, in order.
    install: tuple[tuple[str, ...], ...]
    # A path that exists once installed and is gone once uninstalled.
    installed: str
    # The gglib the smoke runs: the one the install put there.
    installed_cli: str
    # The commands that uninstall it as a user would; () for none.
    uninstall: tuple[tuple[str, ...], ...]
    # A command listing every process, and which reader reads it.
    processes: tuple[str, ...]
    process_listing: str
    # A command that kills a watched program by name, `{name}` in it. Only
    # the clean-up after the checks runs it.
    kill: tuple[str, ...]
    # A command printing, as a JSON list, the user's own PATH as it is
    # stored, `%VAR%` references unexpanded, and gglib's record of the
    # folders it added to it, each null when absent; () where the OS keeps
    # no PATH of the user's own.
    user_path: tuple[str, ...]
    # Steps expected to fail on this OS, each with why: (step, reason). Its
    # failure is then a warning. Empty once every step is required.
    pending: tuple[tuple[str, str], ...]


PLATFORMS: dict[str, Platform] = {
    "windows": Platform(
        target="x86_64-pc-windows-msvc",
        how=(
            "the NSIS installer, silently (/S), into its per-user folder "
            "%LOCALAPPDATA%\\gglib-gui, and the gglib.exe it installs beside the app"
        ),
        cli_archive="gglib-{version}-{target}.zip",
        cli_program="gglib.exe",
        install=(("{artifacts}/gglib-gui-{version}-{target}-setup.exe", "/S"),),
        installed="{LOCALAPPDATA}/gglib-gui",
        installed_cli="{LOCALAPPDATA}/gglib-gui/gglib.exe",
        # An NSIS uninstaller copies itself to %TEMP% and returns at once, so
        # the run waits for the folder to go rather than for this command.
        uninstall=(("{LOCALAPPDATA}/gglib-gui/uninstall.exe", "/S"),),
        processes=("tasklist", "/FO", "CSV", "/NH"),
        process_listing="tasklist",
        kill=("taskkill", "/F", "/T", "/IM", "{name}.exe"),
        # The installer runs `gglib config path-entry add` after it installs
        # and `remove` before it uninstalls (src-tauri/windows/
        # installer-hooks.nsh), which edit the value `Path` of
        # HKCU\Environment and record the folder in HKCU\Software\gglib,
        # `PathEntriesAdded` (gglib_core::paths::user_path). A new terminal's
        # PATH is built from that value; a process the job starts is not, as
        # the installer tells only its own desktop session, so the value is
        # read where it is stored, unexpanded. The record is cast to text, so
        # an absent one reads as "". No braces: this is a template.
        user_path=(
            "powershell",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$p = (Get-Item HKCU:\\Environment).GetValue('Path', $null, 'DoNotExpandEnvironmentNames'); "
            "$r = [string](Get-ItemProperty -Path HKCU:\\Software\\gglib -Name PathEntriesAdded "
            "-ErrorAction SilentlyContinue).PathEntriesAdded; "
            "ConvertTo-Json -Compress -InputObject @($p, $r)",
        ),
        pending=(),
    ),
    "macos": Platform(
        target="aarch64-apple-darwin",
        how=(
            "the .dmg's gglib.app, copied to /Applications as a drag does, and the "
            "gglib inside the app"
        ),
        cli_archive="gglib-{version}-{target}.tar.gz",
        cli_program="gglib",
        install=(
            (
                "hdiutil", "attach", "-nobrowse", "-readonly", "-mountpoint", "{work}/dmg",
                "{artifacts}/gglib-gui-{version}-{target}.dmg",
            ),
            ("ditto", "{work}/dmg/gglib.app", "/Applications/gglib.app"),
            ("hdiutil", "detach", "{work}/dmg"),
        ),
        installed="/Applications/gglib.app",
        installed_cli="/Applications/gglib.app/Contents/MacOS/gglib",
        uninstall=(("rm", "-rf", "/Applications/gglib.app"),),
        processes=("ps", "-A", "-o", "comm="),
        process_listing="ps",
        kill=("pkill", "-KILL", "-x", "{name}"),
        user_path=(),
        pending=(),
    ),
    "linux": Platform(
        target="x86_64-unknown-linux-gnu",
        how=(
            "the CLI's .deb and the app's .deb together with apt, which also proves "
            "the app's Depends names the CLI's package; the AppImage needs FUSE and a "
            "display, and build-gui already checks it carries the CLI"
        ),
        cli_archive="gglib-{version}-{target}.tar.gz",
        cli_program="gglib",
        install=(
            ("sudo", "apt-get", "update"),
            (
                "sudo", "apt-get", "install", "-y", "--no-install-recommends",
                "{artifacts}/gglib-{version}-{target}.deb",
                "{artifacts}/gglib-gui-{version}-{target}.deb",
            ),
        ),
        installed="/usr/bin/gglib",
        installed_cli="/usr/bin/gglib",
        uninstall=(("sudo", "apt-get", "remove", "-y", "gglib-gui", "gglib"),),
        processes=("ps", "-A", "-o", "comm="),
        process_listing="ps",
        kill=("pkill", "-KILL", "-x", "{name}"),
        user_path=(),
        pending=(),
    ),
}


# --- the pure parts, which the self-test checks ------------------------------


def template_names(template: str) -> set[str]:
    """The `{name}`s a template names."""
    return {name for _, name, _, _ in string.Formatter().parse(template) if name is not None}


def expand(template: str, values: Mapping[str, str]) -> str:
    """The template with each `{name}` replaced; a name with no value is refused."""
    missing = template_names(template) - set(values)
    if missing:
        raise KeyError(f"{template!r} names {', '.join(sorted(missing))}, which this run does not give")
    return template.format_map(values)


def platform_templates(platform: Platform) -> list[str]:
    """Every string of a platform's entry that the run expands."""
    found: list[str] = []
    for field in fields(Platform):
        if field.name == "pending":
            continue
        value = getattr(platform, field.name)
        if isinstance(value, str):
            found.append(value)
        else:
            for item in value:
                found.extend(item if isinstance(item, tuple) else (item,))
    return found


def check_table(platforms: Mapping[str, Platform], targets: list[str]) -> list[str]:
    """What is wrong with `platforms`, given the targets release.yml builds."""
    problems: list[str] = []
    for name in ("windows", "macos", "linux"):
        if name not in platforms:
            problems.append(f"no entry for {name}")
    by_target = {p.target: name for name, p in platforms.items()}
    for target in targets:
        if target not in by_target:
            problems.append(f"release.yml builds {target}, which no entry is for")
    for name, platform in platforms.items():
        if platform.target not in targets:
            problems.append(f"{name}: release.yml builds no {platform.target}")
        for field in ("target", "how", "cli_archive", "cli_program", "installed", "installed_cli", "process_listing"):
            if not getattr(platform, field):
                problems.append(f"{name}: {field} is empty")
        for field in ("install", "processes", "kill"):
            if not getattr(platform, field):
                problems.append(f"{name}: {field} is empty")
        if platform.process_listing not in LISTINGS:
            problems.append(f"{name}: no reader for a {platform.process_listing!r} process list")
        if "{name}" not in " ".join(platform.kill):
            problems.append(f"{name}: kill does not name {{name}}")
        for step, reason in platform.pending:
            if step not in STEPS:
                problems.append(f"{name}: pending names {step!r}, which is no step")
            if not reason:
                problems.append(f"{name}: pending {step!r} gives no reason")
        for template in platform_templates(platform):
            # `kill` alone also takes the program's name.
            unknown = template_names(template) - set(VALUES) - ({"name"} if template in platform.kill else set())
            if unknown:
                problems.append(f"{name}: {template!r} names {', '.join(sorted(unknown))}, which the run does not give")
    return problems


def tasklist_names(text: str) -> list[str]:
    """Image names from `tasklist /FO CSV /NH`."""
    return [row[0] for row in csv.reader(io.StringIO(text)) if row and row[0].strip()]


def ps_names(text: str) -> list[str]:
    """Program names from `ps -A -o comm=`, which macOS prints as paths."""
    return [line.strip().rsplit("/", 1)[-1] for line in text.splitlines() if line.strip()]


LISTINGS: dict[str, Callable[[str], list[str]]] = {"tasklist": tasklist_names, "ps": ps_names}


def leftovers(names: list[str], watched: tuple[str, ...] = WATCHED) -> list[str]:
    """The watched programs among `names`, by program name alone.

    `gglib.exe` and `GGLIB.EXE` are gglib; `gglib-test` and a command line
    that merely mentions gglib are not.
    """
    found = set()
    for name in names:
        stem = name.strip()
        if stem.lower().endswith(".exe"):
            stem = stem[:-4]
        if stem.lower() in watched:
            found.add(stem.lower())
    return sorted(found)


def check_version(said: str, version: str, sha: str) -> str | None:
    """Why `gglib --version`'s output is not this release's; None when it is.

    It must read `gglib <version> (<the commit's first 12 characters>)`.
    """
    lines = [line.strip() for line in said.splitlines() if line.strip()]
    if not lines:
        return "it printed nothing"
    want = f"gglib {version} ({sha[:12]})"
    if lines[0] == want:
        return None
    match = re.fullmatch(r"gglib (\S+)(?: \((\S+)\))?", lines[0])
    if not match:
        return f"it printed {lines[0]!r}, not {want!r}"
    if match.group(1) != version:
        return f"it names version {match.group(1)}, not {version}"
    if match.group(2) is None:
        return f"it names no commit; the release is {sha[:12]}"
    return f"it names commit {match.group(2)}, not {sha[:12]}"


def check_reply(status: int, body: str) -> tuple[str | None, str]:
    """Why a chat completion is not a reply, or None; and the reply's text."""
    if status != 200:
        return f"the endpoint answered HTTP {status}: {body[:400]}", ""
    try:
        answer = json.loads(body)
    except json.JSONDecodeError:
        return f"the body is not JSON: {body[:400]!r}", ""
    if not isinstance(answer, dict):
        return f"the body is not a JSON object: {body[:400]}", ""
    if "error" in answer:
        return f"the body is an error: {json.dumps(answer['error'])[:400]}", ""
    try:
        content = answer["choices"][0]["message"]["content"]
    except (KeyError, IndexError, TypeError):
        return f"the body has no choices[0].message.content: {body[:400]}", ""
    if not isinstance(content, str) or not content.strip():
        return f"the reply is empty: {body[:400]}", ""
    return None, content


def read_settings(said: str) -> dict[str, str]:
    """`key = value` lines, as `gglib config paths` prints them."""
    found: dict[str, str] = {}
    for line in said.splitlines():
        key, sep, value = line.partition(" = ")
        if sep and key.strip() and " " not in key.strip():
            found[key.strip()] = value.strip()
    return found


def listed_model_id(said: str, file_name: str) -> str | None:
    """The ID `gglib model list` gives the model whose file is `file_name`.

    `model list` prints a table: a row starts with the model's ID and ends
    with its file's path, which may hold spaces. `model add` prints no ID, so
    the list is where it is read. None unless exactly one row is that file's.
    """
    ids = []
    for line in said.splitlines():
        match = re.match(r"^\s*(\d+)\s+\S", line)
        if match and line.rstrip().replace("\\", "/").endswith("/" + file_name):
            ids.append(match.group(1))
    return ids[0] if len(ids) == 1 else None


def read_up(said: str) -> dict[str, str]:
    """What `gglib up` says it did: the endpoint's `base` URL from its client
    block, the `model` id a client sends ("Model ID <id> \u00b7 gglib id <n>"),
    and its warm-up's `proof`, the line saying whether the first request was
    answered. Each key is present only when `up` said it."""
    found: dict[str, str] = {}
    base = re.search(r"\bhttps?://(?:127\.0\.0\.1|localhost|\[::1\]):\d+/v1\b", said)
    if base:
        found["base"] = base.group(0)
    model = re.search(r"Model ID (\S+) \u00b7 gglib id \d+", said)
    if model:
        found["model"] = model.group(1)
    proof = re.search(r"^\s*(?:\u2713 answered in .*|! the first request did not complete: .*)$", said, re.MULTILINE)
    if proof:
        found["proof"] = proof.group(0).strip()
    return found


def shorten(text: str, limit: int = 240) -> str:
    """`text` on one line, cut to `limit` characters with a pointer to the log."""
    flat = " ".join(text.split())
    if len(flat) <= limit:
        return flat
    return flat[: limit - 40].rstrip() + " \u2026 (whole text in 00-summary.txt)"


def path_entries(text: str | None) -> list[str]:
    """A PATH value's entries, as gglib_core::paths::user_path splits them:
    at each `;` outside double quotes, an unclosed quote running to the end."""
    if not text:
        return []
    entries, entry, quoted = [], "", False
    for char in text:
        if char == '"':
            quoted = not quoted
        if char == ";" and not quoted:
            entries.append(entry)
            entry = ""
        else:
            entry += char
    entries.append(entry)
    return entries


def names_folder(entry: str, folder: str) -> bool:
    """Whether a PATH entry names `folder`, by user_path's rule: the same text
    once surrounding quotes and trailing separators are dropped, `/` read as
    `\\` and case ignored. `%LOCALAPPDATA%\\gglib-gui` is another entry."""

    def plain(text: str) -> str:
        # user_path.rs's `compared`, step for step.
        if len(text) >= 2 and text.startswith('"') and text.endswith('"'):
            text = text[1:-1]
        return text.rstrip("\\/").replace("/", "\\").lower()

    return bool(plain(entry)) and plain(entry) == plain(folder)


def check_path_added(before: str | None, after: str | None, folder: str) -> str | None:
    """Why the install did not put `folder` on the user's PATH as it should:
    once, every other entry kept as written. None when it did."""
    named = [entry for entry in path_entries(after) if names_folder(entry, folder)]
    if not named:
        return f"the user's PATH has no entry for {folder}; it is {after!r}"
    if len(named) > 1:
        return f"the user's PATH names {folder} {len(named)} times; it is {after!r}"
    kept = [entry for entry in path_entries(after) if not names_folder(entry, folder) and entry]
    was = [entry for entry in path_entries(before) if entry]
    if kept != was:
        return f"the install changed the user's PATH's other entries: it was {before!r}, it is {after!r}"
    return None


def check_path_removed(before: str | None, after: str | None, folder: str) -> str | None:
    """Why the uninstall did not give the user's PATH back as it was before
    the install. An absent value and an empty one are the same: a value left
    with nothing in it is stored empty. None when it did."""
    if any(names_folder(entry, folder) for entry in path_entries(after)):
        return f"the user's PATH still names {folder}; it is {after!r}"
    if (after or "") != (before or ""):
        return f"the user's PATH is not what it was before the install: it was {before!r}, it is {after!r}"
    return None


def record_names(record: str | None, folder: str) -> bool:
    """Whether gglib's record of the folders it added (`;`-separated) names `folder`."""
    return any(names_folder(entry, folder) for entry in (record or "").split(";"))


def leftover_locks(paths: list[str]) -> list[str]:
    """The files among `paths`, relative to the data folder, that a running
    daemon or engine holds and a stopped one removes: the daemon's lock and
    the record beside it, and the engines' pid files."""
    held = re.compile(r"^(?:daemon\.lock|daemon\.json(?:\.part)?|pids/[^/]+\.pid(?:\.tmp)?)$")
    return sorted(path for path in (p.replace("\\", "/") for p in paths) if held.match(path))


def release_targets(workflow: str) -> list[str]:
    """The targets release.yml's `meta` job lists for the desktop builds."""
    match = re.search(r"^\s*gui='(\[.*\])'\s*$", workflow, re.MULTILINE)
    if not match:
        return []
    return [leg["target"] for leg in json.loads(match.group(1))]


# --- the run -----------------------------------------------------------------


class Failed(Exception):
    """A step's check did not hold; the message says what was seen."""


@dataclass
class Step:
    name: str
    run: Callable[[], str | None]
    # Runs after an earlier step failed, to say what that left behind.
    always: bool = False
    # Why this step is expected to fail today; its failure then fails nothing.
    pending: str = ""
    # After an earlier step failed, its failure is reported and not counted:
    # it may be that failure's consequence, and the run has failed already.
    secondary: bool = False


# How a step that had nothing to check says so.
SKIPPED = "skipped: "

# Every step's name, in order; PLATFORMS' `pending` names steps from here.
STEPS = (
    "no gglib program is running yet",
    "the user's PATH is read before the install",
    "install the built release",
    "the installed gglib --version",
    "the CLI archive's gglib --version",
    "the install put its folder on the user's PATH once",
    "a data folder of the smoke's own",
    "the pinned engine installs and works",
    "the pinned test model is added",
    "gglib up starts the endpoint",
    "one chat request gets a reply",
    "gglib daemon stop",
    "no gglib process is left",
    "no lock is left in the data folder",
    "uninstall",
    "the uninstall gave the user's PATH back as it was",
)


def slug(text: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")[:48]


def annotate(kind: str, title: str, message: str) -> None:
    """A GitHub annotation; a property's value escapes `:` and `,` too."""
    flat = message.replace("%", "%25").replace("\r", "").replace("\n", "%0A")
    title = title.replace("%", "%25").replace(":", "%3A").replace(",", "%2C")
    print(f"::{kind} title={title}::{flat}", flush=True)


class Smoke:
    def __init__(self, platform: Platform, args: argparse.Namespace) -> None:
        self.platform = platform
        self.version = args.version
        self.sha = args.sha
        self.artifacts = os.path.abspath(args.artifacts)
        self.models = os.path.abspath(args.models)
        self.work = os.path.abspath(args.work)
        self.logs = os.path.join(self.work, "logs")
        self.data = os.path.join(self.work, "data")
        self.models_dir = os.path.join(self.work, "models")
        self.values = {
            "version": self.version,
            "target": platform.target,
            "artifacts": self.artifacts,
            "work": self.work,
            "LOCALAPPDATA": os.environ.get("LOCALAPPDATA", "<no LOCALAPPDATA>"),
        }
        self.env = dict(os.environ)
        self.env.update(
            {
                "GGLIB_DATA_DIR": self.data,
                "GGLIB_MODELS_DIR": self.models_dir,
                "GGLIB_DAEMON_PORT": str(DAEMON_PORT),
                "NO_COLOR": "1",
            }
        )
        self.env.pop("GGLIB_RESOURCE_DIR", None)
        self.count = 0
        self.before: list[str] = []
        self.installed = False
        self.daemon_may_run = False
        self.stopped = False
        self.model = ""
        self.path_before: str | None = None
        self.path_read = False
        self.up_said: dict[str, str] = {}
        self.results: list[tuple[str, str, str]] = []

    # -- helpers --

    def path(self, template: str) -> str:
        return os.path.normpath(expand(template, self.values))

    def command(self, template: tuple[str, ...]) -> list[str]:
        """The command, each part that names a folder written with this OS's separators."""
        return [
            os.path.normpath(expand(part, self.values)) if template_names(part) & FOLDERS else expand(part, self.values)
            for part in template
        ]

    def cli(self) -> str:
        return self.path(self.platform.installed_cli)

    def run(
        self,
        label: str,
        argv: list[str],
        limit: int,
        answers: bytes | None = None,
        check: bool = True,
    ) -> tuple[int, str, str]:
        """Run `argv` with a time limit, its output to files under logs/."""
        self.count += 1
        stem = os.path.join(self.logs, f"{self.count:02d}-{slug(label)}")
        print(f"$ {subprocess.list2cmdline(argv)}    [limit {limit}s, output in {os.path.basename(stem)}.*]", flush=True)
        stdin: dict = {"input": answers} if answers is not None else {"stdin": subprocess.DEVNULL}
        code: int | None
        started = time.monotonic()
        with open(stem + ".out.txt", "wb") as out, open(stem + ".err.txt", "wb") as err:
            try:
                code = subprocess.run(argv, stdout=out, stderr=err, env=self.env, timeout=limit, **stdin).returncode
            except subprocess.TimeoutExpired:
                code = None
            except OSError as error:
                err.write(f"could not start: {error}\n".encode())
                code = -1
        took = time.monotonic() - started
        said = read_text(stem + ".out.txt")
        complained = read_text(stem + ".err.txt")
        print(f"::group::{label}: exit {code if code is not None else 'none (stopped at the limit)'} after {took:.0f}s")
        print(tail(said, "stdout"))
        print(tail(complained, "stderr"))
        print("::endgroup::", flush=True)
        if code is None:
            raise Failed(f"`{label}` did not finish within {limit}s and was stopped. {tail(complained, 'stderr', 6)}")
        if check and code != 0:
            raise Failed(f"`{label}` exited {code}. {tail(complained, 'stderr', 6)} {tail(said, 'stdout', 6)}")
        return code, said, complained

    def processes(self) -> list[str]:
        _, said, _ = self.run("list processes", self.command(self.platform.processes), LIMITS["short"])
        return LISTINGS[self.platform.process_listing](said)

    def wait_for_no_leftovers(self, limit: int) -> list[str]:
        deadline = time.monotonic() + limit
        while True:
            found = [name for name in leftovers(self.processes()) if name not in self.before]
            if not found or time.monotonic() > deadline:
                return found
            time.sleep(5)

    def wait_for(self, what: str, holds: Callable[[], bool], limit: int) -> None:
        deadline = time.monotonic() + limit
        while not holds():
            if time.monotonic() > deadline:
                raise Failed(f"{what} did not happen within {limit}s")
            time.sleep(3)

    def artifact(self, template: str) -> str:
        found = self.path("{artifacts}/" + template)
        if not os.path.isfile(found):
            raise Failed(f"no {os.path.basename(found)} among the built files; there are: {listing(self.artifacts)}")
        return found

    # -- the steps --

    def nothing_running(self) -> str | None:
        self.before = leftovers(self.processes())
        if self.before:
            raise Failed(f"{', '.join(self.before)} already running before anything was installed")
        return None

    def install(self) -> str | None:
        print(f"Installing {self.platform.how}.")
        for template in self.platform.install:
            for part in template:
                if "{artifacts}" in part:
                    self.artifact(part.replace("{artifacts}/", ""))
            argv = self.command(template)
            self.installed = True
            self.run(f"install: {os.path.basename(argv[0])}", argv, LIMITS["install"])
        self.wait_for(f"{self.cli()} appearing", lambda: os.path.isfile(self.cli()), 120)
        return self.cli()

    def installed_version(self) -> str | None:
        _, said, _ = self.run("installed gglib --version", [self.cli(), "--version"], LIMITS["short"])
        reason = check_version(said, self.version, self.sha)
        if reason:
            raise Failed(f"the installed gglib is not this release: {reason}")
        return said.strip()

    def archive_version(self) -> str | None:
        archive = self.artifact(self.platform.cli_archive)
        into = os.path.join(self.work, "cli")
        shutil.rmtree(into, ignore_errors=True)
        os.makedirs(into)
        if archive.endswith(".zip"):
            with zipfile.ZipFile(archive) as unpacked:
                unpacked.extractall(into)
        else:
            with tarfile.open(archive) as unpacked:
                unpacked.extractall(into, filter="data")
        program = os.path.join(into, self.platform.cli_program)
        if not os.path.isfile(program):
            raise Failed(f"{os.path.basename(archive)} holds no {self.platform.cli_program}: {listing(into)}")
        _, said, _ = self.run("unpacked gglib --version", [program, "--version"], LIMITS["short"])
        reason = check_version(said, self.version, self.sha)
        if reason:
            raise Failed(f"{os.path.basename(archive)}'s gglib is not this release: {reason}")
        return said.strip()

    def user_path(self) -> tuple[str | None, str | None]:
        """The user's own PATH as stored, and gglib's record, from the registry."""
        _, said, _ = self.run("the user's PATH and gglib's record", self.command(self.platform.user_path), LIMITS["short"])
        try:
            path, record = json.loads(said)
        except (ValueError, TypeError):
            raise Failed(f"could not read the user's PATH: {said.strip()[:400]!r}") from None
        if not all(value is None or isinstance(value, str) for value in (path, record)):
            raise Failed(f"the user's PATH or gglib's record read as something other than text: {said.strip()[:400]!r}")
        return path, record

    def path_before_install(self) -> str | None:
        self.path_before, record = self.user_path()
        self.path_read = True
        folder = self.path(self.platform.installed)
        if any(names_folder(entry, folder) for entry in path_entries(self.path_before)):
            raise Failed(f"the user's PATH names {folder} before anything was installed: {self.path_before!r}")
        if record_names(record, folder):
            raise Failed(f"gglib's record names {folder} before anything was installed: {record!r}")
        return f"{len([e for e in path_entries(self.path_before) if e])} entries"

    def path_added(self) -> str | None:
        folder = self.path(self.platform.installed)
        after, record = self.user_path()
        reason = check_path_added(self.path_before, after, folder)
        if reason:
            raise Failed(reason)
        if not record_names(record, folder):
            raise Failed(f"gglib's record (HKCU\\Software\\gglib, PathEntriesAdded) does not name {folder}: {record!r}")
        return f"{folder}, once, the other entries as they were"

    def path_removed(self) -> str | None:
        if not self.path_read or not self.installed:
            return SKIPPED + "the user's PATH was not read before an install"
        folder = self.path(self.platform.installed)
        after, record = self.user_path()
        reason = check_path_removed(self.path_before, after, folder)
        if reason:
            raise Failed(reason)
        if record_names(record, folder):
            raise Failed(f"gglib's record still names {folder}: {record!r}")
        return "as it was before the install"

    def clean_data(self) -> str | None:
        os.makedirs(self.data, exist_ok=True)
        os.makedirs(self.models_dir, exist_ok=True)
        _, said, _ = self.run("gglib config paths", [self.cli(), "config", "paths"], LIMITS["short"])
        paths = read_settings(said)
        problems = []
        if paths.get("data_root_source") != "GGLIB_DATA_DIR":
            problems.append(f"data_root_source is {paths.get('data_root_source')!r}, not GGLIB_DATA_DIR")
        for key, want in (("data_root", self.data), ("resource_root", self.data), ("models_dir", self.models_dir)):
            if not same_path(paths.get(key, ""), want):
                problems.append(f"{key} is {paths.get(key)!r}, not {want}")
        if problems:
            raise Failed("; ".join(problems))
        return paths["data_root"]

    def engine(self) -> str | None:
        self.run("gglib config llama install", [self.cli(), "config", "llama", "install"], LIMITS["engine"])
        _, said, _ = self.run("gglib config llama status", [self.cli(), "config", "llama", "status"], LIMITS["short"])
        if "Functional" not in said:
            raise Failed(f"`gglib config llama status` does not say it is functional: {tail(said, 'stdout', 12)}")
        build = [line.strip() for line in said.splitlines() if re.match(r"\s*(Platform|Acceleration|Build):", line)]
        return "; ".join(build) or "functional"

    def add_model(self) -> str | None:
        model = os.path.join(self.models, MODEL)
        if not os.path.isfile(model):
            raise Failed(f"the pinned test models have no {MODEL}: {listing(self.models)}")
        # With no terminal on its input `model add` asks nothing and takes the
        # parameter count the file gives (mmogr/gglib#2020).
        self.run("gglib model add", [self.cli(), "model", "add", model], LIMITS["model"])
        # `model add` prints no ID; the library's own list says it.
        _, said, _ = self.run("gglib model list", [self.cli(), "model", "list"], LIMITS["short"])
        self.model = listed_model_id(said, MODEL) or ""
        if not self.model:
            raise Failed(f"`gglib model list` has no single row for {MODEL}: {tail(said, 'stdout', 12)}")
        return f"id {self.model}"

    def up(self) -> str | None:
        # What main does today: `up` starts the daemon and the endpoint and
        # sends one warm-up request, whose outcome never fails it, so the
        # request below is the check. Once the endpoint is on at launch (6.2
        # in #1959's plan), `up` still starts it the same way.
        #
        # With -y and --model, `up` asks nothing: the engines are installed
        # with -y (the drawing engine's failure only prints), the model is
        # the one named, and it opens no browser and attaches no dashboard.
        # It exits non-zero only when the chat engine does not work or the
        # endpoint does not start; its warm-up's outcome is a line it prints.
        self.daemon_may_run = True
        _, said, _ = self.run(
            "gglib up",
            [self.cli(), "up", "-y", "--model", self.model, "--port", str(ENDPOINT_PORT)],
            LIMITS["up"],
        )
        self.up_said = read_up(said)
        base = self.up_said.get("base", f"http://127.0.0.1:{ENDPOINT_PORT}/v1 (assumed; up named no URL)")
        return f"{base}; warm-up: {self.up_said.get('proof', 'up printed no warm-up line')}"

    def chat(self) -> str | None:
        # The URL `up` printed when it printed one, else the port it was given.
        base = self.up_said.get("base", f"http://127.0.0.1:{ENDPOINT_PORT}/v1")
        status, body = http("GET", f"{base}/models", None, LIMITS["short"])
        self.save("models.json", body)
        try:
            listed = [entry["id"] for entry in json.loads(body)["data"]]
        except (ValueError, KeyError, TypeError):
            raise Failed(f"GET /v1/models answered HTTP {status}: {body[:400]}") from None
        if not listed:
            raise Failed("GET /v1/models lists no model")
        # The id `up` told a client to send, when the endpoint lists it.
        named = self.up_said.get("model")
        request = dict(QUESTION, model=named if named in listed else listed[0])
        self.save("chat-request.json", json.dumps(request, indent=2))
        started = time.monotonic()
        status, body = http("POST", f"{base}/chat/completions", request, LIMITS["reply"])
        self.save("chat-response.json", body)
        reason, content = check_reply(status, body)
        if reason:
            raise Failed(reason)
        return f"{request['model']} said {content!r} in {time.monotonic() - started:.0f}s"

    def stop(self) -> str | None:
        if not self.daemon_may_run:
            return SKIPPED + "nothing started the daemon"
        self.stopped = True
        # It says so on stderr: "Stopping daemon... — stopped." once the
        # daemon has let go of its lock, or "Daemon is not running.", exit 0
        # both; a daemon still stopping past its watchdog exits 1.
        _, _, complained = self.run("gglib daemon stop", [self.cli(), "daemon", "stop"], LIMITS["stop"])
        return " ".join(complained.split()) or None

    def no_leftovers(self) -> str | None:
        if not self.installed:
            return SKIPPED + "nothing was installed"
        found = self.wait_for_no_leftovers(60)
        if found:
            raise Failed(f"still running: {', '.join(found)}")
        return None

    def no_locks(self) -> str | None:
        if not os.path.isdir(self.data):
            return SKIPPED + "no data folder was made"
        held = leftover_locks(files_under(self.data))
        if held:
            raise Failed(f"left in {self.data}: {', '.join(held)}")
        return None

    def uninstall(self) -> str | None:
        if not self.platform.uninstall:
            return SKIPPED + "no uninstall for this OS"
        if not self.installed:
            return SKIPPED + "nothing was installed"
        for template in self.platform.uninstall:
            argv = self.command(template)
            self.run(f"uninstall: {os.path.basename(argv[0])}", argv, LIMITS["uninstall"])
        installed = self.path(self.platform.installed)
        self.wait_for(f"{installed} going", lambda: not os.path.exists(installed), LIMITS["gone"])
        found = self.wait_for_no_leftovers(30)
        if found:
            raise Failed(f"still running after the uninstall: {', '.join(found)}")
        return None

    def steps(self) -> list[Step]:
        pending = dict(self.platform.pending)
        path = bool(self.platform.user_path)
        # (name, what runs it, runs after a failure, its failure then not the cause, kept on this OS)
        listed = [
            (STEPS[0], self.nothing_running, False, False, True),
            (STEPS[1], self.path_before_install, False, False, path),
            (STEPS[2], self.install, False, False, True),
            (STEPS[3], self.installed_version, False, False, True),
            (STEPS[4], self.archive_version, False, False, True),
            (STEPS[5], self.path_added, False, False, path),
            (STEPS[6], self.clean_data, False, False, True),
            (STEPS[7], self.engine, False, False, True),
            (STEPS[8], self.add_model, False, False, True),
            (STEPS[9], self.up, False, False, True),
            (STEPS[10], self.chat, False, False, True),
            (STEPS[11], self.stop, True, False, True),
            (STEPS[12], self.no_leftovers, True, False, True),
            (STEPS[13], self.no_locks, True, False, True),
            # After a failure too, so one run says as much as it can; its
            # failure then is reported, not counted as the cause.
            (STEPS[14], self.uninstall, True, True, True),
            (STEPS[15], self.path_removed, True, True, path),
        ]
        return [
            Step(name, run, always=always, secondary=secondary, pending=pending.get(name, ""))
            for name, run, always, secondary, kept in listed
            if kept
        ]

    def save(self, name: str, text: str) -> None:
        with open(os.path.join(self.logs, name), "w", encoding="utf-8") as handle:
            handle.write(text)

    def walk(self) -> int:
        os.makedirs(self.logs, exist_ok=True)
        print(f"Smoke test of gglib {self.version} ({self.sha[:12]}) for {self.platform.target}")
        print(f"Data folder {self.data}, daemon port {DAEMON_PORT}, endpoint port {ENDPOINT_PORT}", flush=True)
        failed = False
        try:
            for step in self.steps():
                if failed and not step.always:
                    self.results.append((step.name, "not run", "an earlier step failed"))
                    continue
                print(f"\n=== {step.name}", flush=True)
                try:
                    said = step.run() or ""
                except Failed as error:
                    if step.pending:
                        self.results.append((step.name, "expected failure", f"{error} ({step.pending})"))
                        annotate("warning", f"Smoke test: {step.name}, expected to fail", f"{error}. {step.pending}")
                    elif failed and step.secondary:
                        self.results.append((step.name, "failed too, after an earlier failure", str(error)))
                        annotate("warning", f"Smoke test: {step.name}, after an earlier failure", str(error))
                    else:
                        failed = True
                        self.results.append((step.name, "FAILED", str(error)))
                        annotate("error", f"Smoke test: {step.name}", str(error))
                    continue
                if said.startswith(SKIPPED):
                    self.results.append((step.name, "skipped", said.removeprefix(SKIPPED)))
                elif step.pending:
                    self.results.append((step.name, "passed, though expected to fail", said))
                    annotate("notice", f"Smoke test: {step.name} now passes", f"Switch it on: {step.pending}")
                else:
                    self.results.append((step.name, "passed", said))
        finally:
            self.clean_up()
        self.report()
        return 1 if failed else 0

    def clean_up(self) -> None:
        """After the checks have spoken: stop what a failed run left, so the
        job ends and its logs upload. Nothing that was running before the
        smoke started, nor anything at all when nothing was installed."""
        if not self.installed:
            return
        if self.daemon_may_run and not self.stopped:
            try:
                self.run("clean-up: gglib daemon stop", [self.cli(), "daemon", "stop"], 120, check=False)
            except Failed:
                pass
        try:
            left = leftovers(self.processes())
        except Failed:
            left = list(WATCHED)
        for name in (name for name in left if name not in self.before):
            try:
                self.run(f"clean-up: kill {name}", self.command(tuple(p.replace("{name}", name) for p in self.platform.kill)), 60, check=False)
            except Failed:
                pass

    def report(self) -> None:
        width = max(len(name) for name, _, _ in self.results)
        whole = [f"{name:<{width}}  {verdict}{': ' + said if said else ''}" for name, verdict, said in self.results]
        expected = "; ".join(f"{step} ({why})" for step, why in self.platform.pending) or "none"
        self.save("00-summary.txt", f"Steps expected to fail on this OS: {expected}\n\n" + "\n".join(whole) + "\n")
        print(f"\nSteps expected to fail on this OS: {expected}")
        print("\n=== Summary")
        for name, verdict, said in self.results:
            print(f"{name:<{width}}  {verdict}{': ' + shorten(said) if said else ''}")
        summary = os.environ.get("GITHUB_STEP_SUMMARY")
        if summary:
            with open(summary, "a", encoding="utf-8") as handle:
                handle.write(f"### Smoke test of gglib {self.version} on {self.platform.target}\n\n")
                handle.write(f"Steps expected to fail on this OS: {shorten(expected)}\n\n")
                handle.write("| Step | Result | What it saw |\n|---|---|---|\n")
                for name, verdict, said in self.results:
                    cell = shorten(said).replace("|", "\\|")
                    handle.write(f"| {name} | {verdict} | {cell} |\n")
                handle.write("\n")


def read_text(path: str, limit: int = 2_000_000) -> str:
    with open(path, "rb") as handle:
        return handle.read(limit).decode("utf-8", errors="replace")


def tail(text: str, label: str, lines: int = 60) -> str:
    kept = text.strip().splitlines()
    if not kept:
        return f"({label} empty)"
    cut = kept[-lines:]
    more = f" (last {lines} of {len(kept)} lines)" if len(kept) > lines else ""
    return f"{label}{more}: " + "\n".join(cut)


def listing(folder: str) -> str:
    try:
        return ", ".join(sorted(os.listdir(folder))) or "(empty)"
    except OSError as error:
        return f"(cannot list: {error})"


def files_under(folder: str) -> list[str]:
    found = []
    for root, _, names in os.walk(folder):
        for name in names:
            found.append(os.path.relpath(os.path.join(root, name), folder))
    return found


def same_path(said: str, want: str) -> bool:
    if not said:
        return False
    return os.path.normcase(os.path.realpath(said)) == os.path.normcase(os.path.realpath(want))


def http(method: str, url: str, payload: dict | None, limit: int) -> tuple[int, str]:
    data = json.dumps(payload).encode() if payload is not None else None
    request = urllib.request.Request(url, data=data, method=method, headers={"Content-Type": "application/json"})
    print(f"{method} {url}    [limit {limit}s]", flush=True)
    try:
        with urllib.request.urlopen(request, timeout=limit) as answer:
            return answer.status, answer.read().decode("utf-8", errors="replace")
    except urllib.error.HTTPError as error:
        return error.code, error.read().decode("utf-8", errors="replace")
    except (urllib.error.URLError, TimeoutError, OSError) as error:
        raise Failed(f"{method} {url} got no answer: {error}") from None


def this_os() -> str:
    return {"win32": "windows", "darwin": "macos"}.get(sys.platform, "linux")


# --- self-test ---------------------------------------------------------------

# What `gglib model add` printed on windows-2022 in run 43833047240: no ID.
ADD_SAID = """File validation and metadata extraction successful.

Extracted metadata:
  Name: tinygemma3-Q8_0
  Architecture: gemma3
  Parameters: 0.0B
  Quantization: Q8_0
  Context Length: 131072
Parameter count (in billions) [0.0]:

Model successfully created:

  Name: tinygemma3-Q8_0
  File: D:\\a\\_temp\\test-models\\tinygemma3-Q8_0.gguf
  Parameters: 0.0B
  Architecture: gemma3
  Quantization: Q8_0 \u00b7 8.51 bpw
  Context Length: 131072 tokens
Model successfully added to database!
"""

# `gglib model list` after that add, laid out as list.rs's render_table lays
# out a row: ID first, the file's path last.
LIST_SAID_WINDOWS = """Found 1 model(s):

ID  Name                      Model ID        Params   Arch         Quant    Size       Context    Images  Draws Added                File Path
---------------------------------------------------------------------------------------------------------------------------------------------------
1   tinygemma3-Q8_0           tinygemma3-Q8_0 0.0B     gemma3       Q8_0     51.2 MiB   131072     --      --    2026-11-16 09:12:44  D:\\a\\_temp\\test-models\\tinygemma3-Q8_0.gguf
"""

# docs/cli/snippets/model-list.txt, whose paths hold spaces.
LIST_SAID_DOCS = """Found 3 model(s):

ID  Name                      Model ID               Params   Arch         Quant    Size       Context    Images  Draws Added                File Path
-------------------------------------------------------------------------------------------------------------------------------------------------------------------
3   Qwen3.8-27B               Qwen3.8-27B            27.0B    qwen35       Q8_0     27.05 GiB  262144     --      --    2026-10-31 06:25:37  /Users/mmogr/Library/Application Support/gglib/models/q38/Qwen3.8-27B-Q8_0.gguf
2   Qwen3-4B-Instruct-2507    Qwen3-4B-Instruct-2507 4.0B     qwen3        Q8_0     3.99 GiB   262144     --      --    2026-10-31 06:25:37  /Users/mmogr/Library/Application Support/gglib/models/Qwen3-4B-Instruct-2507-Q8_0.gguf
1   Qwen3-30B-A3B             Qwen3-30B-A3B          30.0B    qwen3moe     Q4_K_M   17.28 GiB  40960      --      --    2026-10-31 06:24:05  /Users/mmogr/Library/Application Support/gglib/models/Qwen3-30B-A3B-Q4_K_M.gguf
"""

LIST_SAID_EMPTY = "No models found.\nUse 'gglib model add <file_path>' to add your first model.\n"

# The end of `gglib up`'s output with colour off. The proof line and the
# "Connect your client" line are as warm.rs and client_block.rs print them;
# the recipe lines under it are the daemon's, and only their URL is read.
UP_SAID = """
  \u2713 answered in 2.4s by tinygemma3-Q8_0

  Connect your client  Model ID tinygemma3-Q8_0 \u00b7 gglib id 1

    OpenAI-compatible        any client that takes a base URL
      Base URL http://127.0.0.1:19488/v1
      API key  (none)

      curl http://127.0.0.1:19488/v1/models
  Ready. The endpoint keeps running in the background.
"""


def self_test() -> int:
    wrong: list[str] = []

    def expect(what: str, got: object, want: object) -> None:
        if got != want:
            wrong.append(f"{what}: got {got!r}, want {want!r}")

    def attempt(what: str, check: Callable[[], None]) -> None:
        try:
            check()
        except Exception as error:  # a stub, or a reader that throws
            wrong.append(f"{what}: raised {type(error).__name__}: {error}")

    with open(os.path.join(REPO, ".github", "workflows", "release.yml"), encoding="utf-8") as handle:
        targets = release_targets(handle.read())

    def table() -> None:
        expect("release.yml's targets", sorted(targets), sorted(p.target for p in PLATFORMS.values()) or ["<none>"])
        expect("the table's problems", check_table(PLATFORMS, targets), [])
        for name, platform in PLATFORMS.items():
            values = dict.fromkeys(VALUES, "x")
            for template in platform_templates(platform):
                if template in platform.kill:
                    continue
                expand(template, values)
        broken = {k: v for k, v in PLATFORMS.items() if k != "macos"}
        if not check_table(broken, targets):
            wrong.append("a table with no macOS entry passed")
        for name, platform in PLATFORMS.items():
            misspelt = tuple(tuple(part.replace("{version}", "{versoin}") for part in command) for command in platform.install)
            for planted_field, value in (
                ("cli_archive", platform.cli_archive.replace("{version}", "{versoin}")),
                ("install", misspelt),
            ):
                planted = {**PLATFORMS, name: replace(platform, **{planted_field: value})}
                if not any("versoin" in problem for problem in check_table(planted, targets)):
                    wrong.append(f"{name}: a {planted_field} naming {{versoin}} passed")
            planted = {**PLATFORMS, name: replace(platform, target="x86_64-unknown-freebsd")}
            if not check_table(planted, targets):
                wrong.append(f"{name}: a target release.yml does not build passed")
            planted = {**PLATFORMS, name: replace(platform, pending=(("a step nobody runs", "#0"),))}
            if not any("no step" in problem for problem in check_table(planted, targets)):
                wrong.append(f"{name}: a pending step that is no step passed")
            planted = {**PLATFORMS, name: replace(platform, pending=((STEPS[-1], "#0 merges"),))}
            expect(f"{name}: a pending step that is one", check_table(planted, targets), [])

    attempt("the per-OS table", table)

    def processes() -> None:
        tasklist = (
            '"System Idle Process","0","Services","0","8 K"\r\n'
            '"Runner.Worker.exe","4120","Console","1","90,000 K"\r\n'
            '"GGLIB.EXE","5120","Console","1","20,000 K"\r\n'
            '"llama-server.exe","5200","Console","1","120,000 K"\r\n'
            '"gglib-test.exe","5300","Console","1","1,000 K"\r\n'
        )
        expect("tasklist names", tasklist_names(tasklist)[:3], ["System Idle Process", "Runner.Worker.exe", "GGLIB.EXE"])
        expect("leftovers in a tasklist", leftovers(tasklist_names(tasklist)), ["gglib", "llama-server"])
        ps = (
            "/sbin/launchd\n"
            "/Applications/gglib.app/Contents/MacOS/gglib-gui\n"
            "python3\n"
            "/Users/runner/work/_temp/smoke/data/.sd/bin/sd-server\n"
            "gglibd\n"
            "llama-server-x\n"
        )
        expect("ps names", ps_names(ps)[:3], ["launchd", "gglib-gui", "python3"])
        expect("leftovers in a ps list", leftovers(ps_names(ps)), ["gglib-gui", "sd-server"])
        expect("leftovers among unrelated programs", leftovers(["bash", "Runner.Listener", "python", "release_smoke.py", "gglib.app"]), [])
        expect("leftovers in an empty list", leftovers([]), [])

    attempt("the leftover-process check", processes)

    def replies() -> None:
        good = json.dumps({"choices": [{"message": {"role": "assistant", "content": "What is this:\n\n"}}]})
        expect("a reply with text", check_reply(200, good), (None, "What is this:\n\n"))
        for what, status, body in (
            ("an empty reply", 200, json.dumps({"choices": [{"message": {"content": ""}}]})),
            ("a reply of whitespace", 200, json.dumps({"choices": [{"message": {"content": " \n "}}]})),
            ("a null reply", 200, json.dumps({"choices": [{"message": {"content": None}}]})),
            ("no choices", 200, json.dumps({"choices": []})),
            ("an error body with 200", 200, json.dumps({"error": {"message": "model failed to load"}})),
            ("an error body", 503, json.dumps({"error": {"message": "loading"}})),
            ("an HTML page", 200, "<html>gateway</html>"),
            ("an empty body", 200, ""),
            ("a JSON list", 200, "[]"),
        ):
            reason, content = check_reply(status, body)
            if reason is None:
                wrong.append(f"the reply check took {what} for a reply")
            expect(f"the text of {what}", content, "")

    attempt("the reply check", replies)

    def versions() -> None:
        sha = "0123456789abcdef0123456789abcdef01234567"
        expect("this release", check_version("gglib 0.37.0-rc.1 (0123456789ab)\n", "0.37.0-rc.1", sha), None)
        expect("this release, Windows line ends", check_version("gglib 0.37.0-rc.1 (0123456789ab)\r\n", "0.37.0-rc.1", sha), None)
        for what, said in (
            ("another version", "gglib 0.36.0 (0123456789ab)"),
            ("the stable version of the candidate", "gglib 0.37.0 (0123456789ab)"),
            ("a longer prerelease", "gglib 0.37.0-rc.10 (0123456789ab)"),
            ("another commit", "gglib 0.37.0-rc.1 (fedcba987654)"),
            ("no commit", "gglib 0.37.0-rc.1"),
            ("nothing", ""),
            ("another program", "llama-server 0.37.0-rc.1 (0123456789ab)"),
        ):
            if check_version(said, "0.37.0-rc.1", sha) is None:
                wrong.append(f"the version check took {what} ({said!r}) for 0.37.0-rc.1 at {sha[:12]}")

    attempt("the version check", versions)

    def readers() -> None:
        paths = read_settings(
            "binary = /usr/bin/gglib\nversion = 0.37.0 (0123456789ab)\ndata_root_source = GGLIB_DATA_DIR\n"
            "data_root = C:\\Users\\runner\\smoke data\\x\nmodels_source = Env\nnot a setting\n"
        )
        expect("config paths' data_root_source", paths.get("data_root_source"), "GGLIB_DATA_DIR")
        expect("config paths' data_root", paths.get("data_root"), "C:\\Users\\runner\\smoke data\\x")
        expect("config paths' other lines", "not a setting" in " ".join(paths), False)
        expect("an ID in model add's own output, which has none", listed_model_id(ADD_SAID, MODEL), None)
        expect("the ID in a model list on Windows", listed_model_id(LIST_SAID_WINDOWS, MODEL), "1")
        expect(
            "the ID in docs/cli/snippets/model-list.txt, a path with spaces",
            listed_model_id(LIST_SAID_DOCS, "Qwen3-4B-Instruct-2507-Q8_0.gguf"),
            "2",
        )
        expect("an ID for a file the list does not hold", listed_model_id(LIST_SAID_DOCS, MODEL), None)
        expect("an ID in an empty library", listed_model_id(LIST_SAID_EMPTY, MODEL), None)
        expect("an ID for a file listed twice", listed_model_id(LIST_SAID_WINDOWS + LIST_SAID_WINDOWS.splitlines()[-1].replace("1   ", "2   ", 1), MODEL), None)
        expect(
            "what up said",
            read_up(UP_SAID),
            {
                "base": "http://127.0.0.1:19488/v1",
                "model": "tinygemma3-Q8_0",
                "proof": "\u2713 answered in 2.4s by tinygemma3-Q8_0",
            },
        )
        expect(
            "what up said when its warm-up failed",
            read_up("  ! the first request did not complete: timed out\n  the endpoint is still running\n"),
            {"proof": "! the first request did not complete: timed out"},
        )
        expect("a short detail", shorten("passed:  id 1\n"), "passed: id 1")
        long = "no folder on a new shell's PATH holds gglib.exe; it is: " + ";".join(["C:\\Program Files\\x"] * 60)
        cut = shorten(long)
        if len(cut) > 240 or not cut.startswith("no folder on a new shell's PATH") or "00-summary.txt" not in cut:
            wrong.append(f"a long detail was cut to {cut!r}")
        expect(
            "locks left",
            leftover_locks(["daemon.lock", "daemon.json", "pids\\1.pid", "pids/2.pid.tmp", "logs/daemon.log", "data/gglib.db", "pids/README", "daemon.lock.bak"]),
            ["daemon.json", "daemon.lock", "pids/1.pid", "pids/2.pid.tmp"],
        )
        expect("locks left, none", leftover_locks(["logs/daemon.log", ".llama/bin/llama-server", ".env"]), [])

    attempt("the readers", readers)

    def user_paths() -> None:
        folder = "C:\\Users\\runneradmin\\AppData\\Local\\gglib-gui"
        before = '%USERPROFILE%\\.dotnet\\tools;"C:\\odd;dir";C:\\Tools\\'
        added = before + ";" + folder
        expect("PATH entries, a ; inside quotes kept", path_entries(before), ["%USERPROFILE%\\.dotnet\\tools", '"C:\\odd;dir"', "C:\\Tools\\"])
        expect("PATH entries, an unclosed quote to the end", path_entries('a;"b;c'), ["a", '"b;c'])
        expect("PATH entries of no value", path_entries(None), [])
        for spelt in (folder, folder.lower() + "\\", '"' + folder + '"', folder.replace("\\", "/") + "/"):
            if not names_folder(spelt, folder):
                wrong.append(f"{spelt!r} was not read as naming {folder}")
        for other in ("%LOCALAPPDATA%\\gglib-gui", folder + "-old", "", '"'):
            if names_folder(other, folder):
                wrong.append(f"{other!r} was read as naming {folder}")
        expect("the folder added once", check_path_added(before, added, folder), None)
        expect("the folder added to no value", check_path_added(None, folder, folder), None)
        expect("the folder added after a trailing ;", check_path_added("C:\\a;", "C:\\a;" + folder + ";", folder), None)
        for what, after in (
            ("not added", before),
            ("added twice", added + ";" + folder.lower()),
            ("an entry expanded", added.replace("%USERPROFILE%", "C:\\Users\\runneradmin")),
            ("an entry dropped", "C:\\Tools\\;" + folder),
            ("the quoted entry split", before.replace('"', "") + ";" + folder),
        ):
            if check_path_added(before, after, folder) is None:
                wrong.append(f"the PATH check took a folder {what} for one added once")
        expect("the value given back", check_path_removed(before, before, folder), None)
        expect("no value given back as empty", check_path_removed(None, "", folder), None)
        for what, after in (
            ("still there", added),
            ("there in another spelling", before + ";" + folder.upper() + "\\"),
            ("not given back whole", before.rstrip("\\")),
            ("emptied", ""),
        ):
            if check_path_removed(before, after, folder) is None:
                wrong.append(f"the PATH check took a value {what} for the one before the install")
        expect("a record naming the folder", record_names("C:\\other;" + folder, folder), True)
        expect("no record", record_names(None, folder), False)

    attempt("the user's PATH checks", user_paths)

    if wrong:
        print("❌ self-test: a planted case was not judged as planted:")
        for line in wrong:
            print(f"     {line}")
        return 1
    print(f"✅ self-test: the table covers {', '.join(sorted(PLATFORMS))} as release.yml builds them; every planted process, reply, version, PATH and file judged as planted")
    return 0


def main() -> int:
    # Windows' Python writes a pipe in the ANSI code page, which has no ✅.
    sys.stdout.reconfigure(encoding="utf-8", line_buffering=True)
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n", 1)[0])
    sub = parser.add_subparsers(dest="command", required=True)
    run = sub.add_parser("run", help="install the built release and ask it one question")
    run.add_argument("--artifacts", required=True, help="the folder holding this target's built files")
    run.add_argument("--version", required=True, help="the version being released")
    run.add_argument("--sha", required=True, help="the commit being released")
    run.add_argument("--models", required=True, help="the pinned test models' folder")
    run.add_argument("--work", required=True, help="a folder of the smoke's own: data, logs, unpacked CLI")
    run.add_argument("--os", choices=sorted(PLATFORMS), default=this_os(), help="the OS whose facts to use (default: this one)")
    run.add_argument("--here", action="store_true", help="run outside GitHub Actions; it installs into this machine's real locations")
    args = parser.parse_args()
    if os.environ.get("GITHUB_ACTIONS") != "true" and not args.here:
        print("refusing: this installs gglib into this machine's real locations; pass --here to do it anyway", file=sys.stderr)
        return 2
    return Smoke(PLATFORMS[args.os], args).walk()


if __name__ == "__main__":
    sys.exit(main())
