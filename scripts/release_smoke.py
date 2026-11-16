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
 4. where PLATFORMS names a check for it, a new shell finds `gglib` on the
    PATH it would be given (expected to fail until mmogr/gglib#2024 merges);
 5. `gglib config paths` reads a data folder of the smoke's own, under
    --work, through GGLIB_DATA_DIR: nothing of the runner's or a person's;
 6. `gglib config llama install` installs the pinned engine, and
    `gglib config llama status` says it is functional;
 7. `gglib model add` adds tinygemma3 from the pinned test models (--models,
    the `pinned-engines` action's folder);
 8. `gglib up -y --model <id> --port <port>` starts the daemon and the
    endpoint as a user's first command does;
 9. one chat request to the endpoint gets a reply with text in it;
10. `gglib daemon stop`, then no gglib, gglib-gui, llama-server or sd-server
    process is left, and no lock or pid file is left in the data folder;
11. uninstalling, where PLATFORMS says how, removes what was installed and
    leaves no gglib process.

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
everything else; that the reply check refuses an empty reply and an error
body; that the version check refuses another version or commit; and the
readers of a process list, of `gglib config paths`, of `gglib model add` and
of the data folder.

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
    # A command printing the PATH a new shell is given, read from where a
    # new shell reads it; () where no install puts gglib on PATH that way.
    new_shell_path: tuple[str, ...]
    # Why the new-shell check is expected to fail today; "" once it holds.
    new_shell_pending: str


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
        # The machine's PATH, then the user's, from the registry, as Explorer
        # gives them to a shell it starts. The job's own PATH proves nothing.
        new_shell_path=(
            "powershell",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Environment]::GetEnvironmentVariable('Path','Machine') + ';' + "
            "[Environment]::GetEnvironmentVariable('Path','User')",
        ),
        new_shell_pending=(
            "the installer does not add its folder to the user's PATH until "
            "https://github.com/mmogr/gglib/pull/2024 merges; empty "
            "`new_shell_pending` then, and this check fails the job when it fails"
        ),
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
        new_shell_path=(),
        new_shell_pending="",
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
        new_shell_path=(),
        new_shell_pending="",
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
        if platform.new_shell_pending and not platform.new_shell_path:
            problems.append(f"{name}: a new-shell check is pending with no command to run it")
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


def model_id(said: str) -> str | None:
    """The id `gglib model add` says it gave the model, from its `ID:` line."""
    match = re.search(r"^\s*ID:\s*(\d+)\s*$", said, re.MULTILINE)
    return match.group(1) if match else None


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


# How a step that had nothing to check says so.
SKIPPED = "skipped: "


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
        self.new_shell_found = False
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

    def new_shell_finds(self, want: bool) -> str | None:
        if not want and not self.new_shell_found:
            # Nothing to take away: the first check found it on no PATH.
            return SKIPPED + "no new shell found gglib before the uninstall either"
        _, said, _ = self.run("the PATH a new shell is given", self.command(self.platform.new_shell_path), LIMITS["short"])
        folders = [os.path.expandvars(entry.strip()) for entry in said.strip().split(os.pathsep) if entry.strip()]
        name = os.path.basename(self.cli())
        found = [folder for folder in folders if os.path.isfile(os.path.join(folder, name))]
        if want and not found:
            raise Failed(f"no folder on a new shell's PATH holds {name}; it is: {os.pathsep.join(folders)}")
        if not want and found:
            raise Failed(f"a new shell still finds {name} in {', '.join(found)}")
        if want:
            self.new_shell_found = True
        return ", ".join(found) or None

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
        # gglib model add asks for the parameter count and waits even with no
        # terminal on its input (mmogr/gglib#2020). An empty line takes the
        # count it read from the file; 0.1 answers when it read none. Remove
        # the answers once #2020 is fixed.
        _, said, _ = self.run("gglib model add", [self.cli(), "model", "add", model], LIMITS["model"], answers=b"\n0.1\n")
        self.model = model_id(said) or ""
        if not self.model:
            raise Failed(f"`gglib model add` named no ID: {tail(said, 'stdout', 12)}")
        return f"id {self.model}"

    def up(self) -> str | None:
        # What main does today: `up` starts the daemon and the endpoint and
        # sends one warm-up request, whose outcome never fails it, so the
        # request below is the check. Once the endpoint is on at launch (6.2
        # in #1959's plan), `up` still starts it the same way.
        self.daemon_may_run = True
        self.run(
            "gglib up",
            [self.cli(), "up", "-y", "--model", self.model, "--port", str(ENDPOINT_PORT)],
            LIMITS["up"],
        )
        return f"http://127.0.0.1:{ENDPOINT_PORT}/v1"

    def chat(self) -> str | None:
        base = f"http://127.0.0.1:{ENDPOINT_PORT}/v1"
        status, body = http("GET", f"{base}/models", None, LIMITS["short"])
        self.save("models.json", body)
        try:
            listed = [entry["id"] for entry in json.loads(body)["data"]]
        except (ValueError, KeyError, TypeError):
            raise Failed(f"GET /v1/models answered HTTP {status}: {body[:400]}") from None
        if not listed:
            raise Failed("GET /v1/models lists no model")
        request = dict(QUESTION, model=listed[0])
        self.save("chat-request.json", json.dumps(request, indent=2))
        started = time.monotonic()
        status, body = http("POST", f"{base}/chat/completions", request, LIMITS["reply"])
        self.save("chat-response.json", body)
        reason, content = check_reply(status, body)
        if reason:
            raise Failed(reason)
        return f"{listed[0]} said {content!r} in {time.monotonic() - started:.0f}s"

    def stop(self) -> str | None:
        if not self.daemon_may_run:
            return SKIPPED + "nothing started the daemon"
        self.stopped = True
        self.run("gglib daemon stop", [self.cli(), "daemon", "stop"], LIMITS["stop"])
        return None

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
        pending = self.platform.new_shell_pending
        steps = [
            Step("no gglib program is running yet", self.nothing_running),
            Step("install the built release", self.install),
            Step("the installed gglib --version", self.installed_version),
            Step("the CLI archive's gglib --version", self.archive_version),
        ]
        if self.platform.new_shell_path:
            steps.append(Step("a new shell finds gglib", lambda: self.new_shell_finds(True), pending=pending))
        steps += [
            Step("a data folder of the smoke's own", self.clean_data),
            Step("the pinned engine installs and works", self.engine),
            Step("the pinned test model is added", self.add_model),
            Step("gglib up starts the endpoint", self.up),
            Step("one chat request gets a reply", self.chat),
            Step("gglib daemon stop", self.stop, always=True),
            Step("no gglib process is left", self.no_leftovers, always=True),
            Step("no lock is left in the data folder", self.no_locks, always=True),
            Step("uninstall", self.uninstall),
        ]
        if self.platform.new_shell_path:
            steps.append(Step("a new shell no longer finds gglib", lambda: self.new_shell_finds(False), pending=pending))
        return steps

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
        lines = [f"{name:<{width}}  {verdict}{': ' + said if said else ''}" for name, verdict, said in self.results]
        print("\n=== Summary")
        print("\n".join(lines))
        self.save("00-summary.txt", "\n".join(lines) + "\n")
        summary = os.environ.get("GITHUB_STEP_SUMMARY")
        if summary:
            with open(summary, "a", encoding="utf-8") as handle:
                handle.write(f"### Smoke test of gglib {self.version} on {self.platform.target}\n\n")
                handle.write("| Step | Result | What it saw |\n|---|---|---|\n")
                for name, verdict, said in self.results:
                    cell = said.replace("|", "\\|").replace("\n", " ")[:300]
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
        added = "Model successfully created:\n\n  ID: 1\n  Name: tinygemma3\n  Parameters: 0.0B\n"
        expect("model add's ID", model_id(added), "1")
        expect("model add's ID, none said", model_id("Parameter count (in billions) [0.0]:\n"), None)
        expect(
            "locks left",
            leftover_locks(["daemon.lock", "daemon.json", "pids\\1.pid", "pids/2.pid.tmp", "logs/daemon.log", "data/gglib.db", "pids/README", "daemon.lock.bak"]),
            ["daemon.json", "daemon.lock", "pids/1.pid", "pids/2.pid.tmp"],
        )
        expect("locks left, none", leftover_locks(["logs/daemon.log", ".llama/bin/llama-server", ".env"]), [])

    attempt("the readers", readers)

    if wrong:
        print("❌ self-test: a planted case was not judged as planted:")
        for line in wrong:
            print(f"     {line}")
        return 1
    print(f"✅ self-test: the table covers {', '.join(sorted(PLATFORMS))} as release.yml builds them; every planted process, reply, version and file judged as planted")
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
