#!/usr/bin/env python3
"""Try the engine a pin pull request proposes, on this machine, and approve it.

Usage:
    ./scripts/engine_trial.py start PR            # install the candidate over your engine
    ./scripts/engine_trial.py approve PR          # say on the pull request that it works
    ./scripts/engine_trial.py approve PR --print  # print that comment and post nothing
    ./scripts/engine_trial.py revert PR           # put back what was installed before
    ./scripts/engine_trial.py --self-test

A pin pull request moves the release of llama.cpp or stable-diffusion.cpp
that gglib installs. CI proves the candidate starts and takes gglib's flags.
It cannot use it: load a real model on a real GPU, answer, draw. This is that
step, done by the owner on their own machine with the gglib they use.

`start` reads which engine the pull request moves and to which release, from
its head, and installs that release over the one in use, with the `gglib` on
PATH and the override CONTRIBUTING gives for trying a release
(`GGLIB_SD_RELEASE=<tag> gglib config sd install --force`, or
`GGLIB_LLAMA_RELEASE` and `config llama`). The variable is set on that one
command and nowhere else: a daemon started with it would compare everything
against the candidate. What was installed before is kept under
target/engine-trial/, so `revert` knows whether to put back a download or a
source build. Nothing is built, no second daemon runs, and the database and
the models are the owner's own.

Then the owner uses gglib as they like.

`approve` posts a `/tried <engine> <release>` comment from the owner's
account, with what this script saw. The Engine trial workflow hears only the
owner, records the trial on the pull request's branch, and the record is what
lets the bump merge. Before it posts, `approve` refuses when:

* the pull request no longer proposes the release that was installed;
* the installed binary does not say it is that release, asked now;
* the engine was not used since it was installed. For llama.cpp that is a
  reply saved after the install: the app, `gglib chat` and `gglib q` save
  one. For stable-diffusion.cpp it is a picture drawn in a chat after the
  install. `gglib draw` leaves nothing gglib keeps, so it does not count.

It then asks for the release to be typed on a terminal, and refuses where
there is none: approving is a person's act, not a script's. `--print` stops
before that and prints the comment.

`--self-test` runs every reader and every refusal on planted inputs, and
holds the comment to scripts/fixtures/engine-trial-comment.txt, the sample
scripts/record_trial.sh's own self-test reads, so the two cannot drift. It
asks the question on a terminal of its own, with a line typed there, and
with no terminal at all.

Exit codes:
    0  done, or the self-test passed
    1  refused (the reason is printed), or the self-test failed
    2  the arguments were refused
"""

from __future__ import annotations

import argparse
import datetime
import json
import os
import platform
import re
import select
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.parse

REPO = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
STATE_DIR = os.path.join(REPO, "target", "engine-trial")
FIXTURE = os.path.join(REPO, "scripts", "fixtures", "engine-trial-comment.txt")

# Each engine: its product name, where its pin is and under which constant,
# the form of its tags, the override that names a release, and its part of the
# command line and of the data folder.
ENGINES = {
    "llama": {
        "product": "llama.cpp",
        "pin_file": "crates/gglib-core/src/domain/engine_pins.rs",
        "constant": "PINNED_LLAMA_RELEASE",
        "tag": re.compile(r"^b([0-9]+)$"),
        "env": "GGLIB_LLAMA_RELEASE",
        "cli": "llama",
        "server_key": "llama_server_path",
        "record": os.path.join(".llama", "llama-config.json"),
        "used": "replies",
    },
    "sd": {
        "product": "stable-diffusion.cpp",
        "pin_file": "crates/gglib-runtime/src/sd/install/release.rs",
        "constant": "PINNED_SD_RELEASE",
        "tag": re.compile(r"^master-([0-9]+)-([0-9a-f]+)$"),
        "env": "GGLIB_SD_RELEASE",
        "cli": "sd",
        "server_key": "sd_server_path",
        "record": os.path.join(".sd", "sd-config.json"),
        "used": "pictures",
    },
}

# Each command's time limit, in seconds. An install is a download, or a
# source build when this machine has no download.
LIMITS = {"short": 60, "install": 3600}

# What a line shown on a pull request may hold: printable ASCII, no backtick.
PLAIN = re.compile(r"^[ -_a-~]{1,200}$")


class Refusal(Exception):
    """Why the script stops, in words for the owner."""


# --- Readers, each of one thing ---------------------------------------------


def pin_in(text: str, constant: str) -> str | None:
    """The release `constant` names in a Rust source file."""
    found = re.search(rf'{re.escape(constant)}: &str = "([^"]+)"', text)
    return found.group(1) if found else None


def moved(base: dict[str, str], head: dict[str, str]) -> tuple[str, str, str]:
    """The engine a pull request moves, with its pin before and after.

    `base` and `head` are each engine's pin file, by engine, at the pull
    request's base and at its head.
    """
    found = []
    for engine, spec in ENGINES.items():
        before = pin_in(base.get(engine, ""), spec["constant"])
        after = pin_in(head.get(engine, ""), spec["constant"])
        if before is None or after is None:
            raise Refusal(f"{spec['constant']} is not in {spec['pin_file']}")
        if before != after:
            if not spec["tag"].match(after):
                raise Refusal(f"{after!r} is not a {spec['product']} release tag")
            found.append((engine, before, after))
    if not found:
        raise Refusal("the pull request moves no engine's pin")
    if len(found) > 1:
        raise Refusal("the pull request moves both engines' pins; try one bump at a time")
    return found[0]


def read_settings(said: str) -> dict[str, str]:
    """`key = value` lines, as `gglib config paths` prints them."""
    found: dict[str, str] = {}
    for line in said.splitlines():
        key, sep, value = line.partition(" = ")
        if sep and key.strip() and " " not in key.strip():
            found[key.strip()] = value.strip()
    return found


def version_line(engine: str, said: str) -> str | None:
    """What the server says it is: the line the recorder keeps of `--version`."""
    lines = [line.strip() for line in said.splitlines() if line.strip()]
    if engine == "llama":
        lines = [line for line in lines if line.startswith("version:")]
    return lines[0] if lines else None


def names_release(engine: str, line: str, release: str) -> bool:
    """Whether a server's version line is of `release`.

    llama-server prints its build number, the `n` of `b<n>`; sd-server prints
    its commit, the last part of `master-<n>-<commit>`.
    """
    tag = ENGINES[engine]["tag"].match(release)
    if not tag:
        return False
    if engine == "llama":
        build = re.search(r"\(build ([0-9]+),", line)
        return bool(build) and build.group(1) == tag.group(1)
    commit = re.search(r"commit ([0-9a-f]+)\b", line)
    return bool(commit) and commit.group(1) == tag.group(2)


def record_time(record: str) -> datetime.datetime:
    """When an install's record says it was installed or built, in UTC."""
    try:
        fields = json.loads(record)
    except json.JSONDecodeError as err:
        raise Refusal(f"the install's record is not JSON: {err}") from err
    stamp = fields.get("installed_at") or fields.get("build_date")
    if not isinstance(stamp, str):
        raise Refusal("the install's record has no time in it")
    try:
        when = datetime.datetime.fromisoformat(stamp.replace("Z", "+00:00"))
    except ValueError as err:
        raise Refusal(f"the install's record has a time that is not one: {stamp!r}") from err
    if when.tzinfo is None:
        when = when.replace(tzinfo=datetime.timezone.utc)
    return when.astimezone(datetime.timezone.utc)


def was_built(record: str | None) -> bool:
    """Whether an install's record is a source build's."""
    if not record:
        return False
    try:
        return "build_date" in json.loads(record)
    except json.JSONDecodeError:
        return False


def used_since(database: str, since: datetime.datetime) -> dict[str, int]:
    """Replies saved and pictures drawn in a chat since `since`.

    gglib stamps a row with SQLite's `datetime('now')`: UTC, to the second,
    with no zone. A picture drawn in a chat is its tool's row, whose text
    starts `Drew`. The database is opened read-only; it is in WAL mode, which
    a reader beside a running daemon is safe in.
    """
    at = since.strftime("%Y-%m-%d %H:%M:%S")
    address = "file:" + urllib.parse.quote(database) + "?mode=ro"
    try:
        with sqlite3.connect(address, uri=True, timeout=10) as db:
            replies = db.execute(
                "SELECT COUNT(*) FROM chat_messages WHERE role = 'assistant' AND created_at >= ?", (at,)
            ).fetchone()[0]
            pictures = db.execute(
                "SELECT COUNT(*) FROM chat_messages WHERE role = 'tool' AND content LIKE 'Drew %' AND created_at >= ?",
                (at,),
            ).fetchone()[0]
    except sqlite3.Error as err:
        raise Refusal(f"gglib's database could not be read ({database}): {err}") from err
    return {"replies": int(replies), "pictures": int(pictures)}


def not_used(engine: str, used: dict[str, int], since: datetime.datetime) -> str | None:
    """Why `used` is not a trial of `engine`, or None when it is one."""
    if used[ENGINES[engine]["used"]] > 0:
        return None
    at = since.strftime("%Y-%m-%d %H:%M UTC")
    if engine == "llama":
        return (
            f"llama.cpp has not answered since it was installed ({at}): gglib has saved no reply since then. "
            "Chat with a model in the app, with `gglib chat` or with `gglib q`, then approve."
        )
    return (
        f"stable-diffusion.cpp has not drawn in a chat since it was installed ({at}). "
        "Draw a picture in a chat, in the app or with /draw in `gglib chat`, then approve. "
        "A picture from `gglib draw` does not count: gglib keeps nothing of it."
    )


def plain(text: str) -> str:
    """`text` as one line a pull request can show: no backtick, printable, cut."""
    kept = "".join(ch for ch in text if " " <= ch <= "~" and ch != "`")
    return kept.strip()[:200]


def comment(facts: dict) -> str:
    """The comment `approve` posts: the request, then what was seen, as JSON."""
    body = json.dumps(facts, indent=2, sort_keys=True)
    return f"/tried {facts['engine']} {facts['release']}\n\n```json\n{body}\n```\n"


# --- The machine --------------------------------------------------------------


def run(argv: list[str], limit: int, env: dict[str, str] | None = None, shown: bool = False) -> tuple[int, str]:
    """Run `argv`; its exit code and what it said. `shown` leaves its output on the terminal."""
    try:
        if shown:
            done = subprocess.run(argv, env=env, timeout=limit, check=False)
            return done.returncode, ""
        done = subprocess.run(
            argv, env=env, timeout=limit, check=False, capture_output=True, text=True, stdin=subprocess.DEVNULL
        )
    except FileNotFoundError as err:
        raise Refusal(f"{argv[0]} is not installed") from err
    except subprocess.TimeoutExpired as err:
        raise Refusal(f"{' '.join(argv)} did not finish in {limit} s") from err
    return done.returncode, done.stdout + done.stderr


def said(argv: list[str], limit: int = LIMITS["short"]) -> str:
    """What `argv` printed, or a refusal with it when it failed."""
    code, out = run(argv, limit)
    if code != 0:
        raise Refusal(f"{' '.join(argv)} failed:\n{out.strip()}")
    return out


def gglib() -> str:
    found = shutil.which("gglib")
    if not found:
        raise Refusal("there is no `gglib` on PATH; the trial uses the gglib you use (`make setup` installs it)")
    return found


def pull_request(number: int) -> dict:
    """The pull request's head, base and state, and each engine's pin file at both."""
    fields = json.loads(
        said(["gh", "pr", "view", str(number), "--json", "state,isCrossRepository,headRefOid,baseRefOid,url"])
    )
    if fields["state"] != "OPEN":
        raise Refusal(f"pull request {number} is {fields['state'].lower()}")
    if fields["isCrossRepository"]:
        raise Refusal(f"pull request {number} is from another repository; a trial is recorded on a branch here")
    repo = said(["gh", "repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"]).strip()
    texts: dict[str, dict[str, str]] = {"base": {}, "head": {}}
    for side, ref in (("base", fields["baseRefOid"]), ("head", fields["headRefOid"])):
        for engine, spec in ENGINES.items():
            texts[side][engine] = said(
                [
                    "gh",
                    "api",
                    "-H",
                    "Accept: application/vnd.github.raw",
                    f"repos/{repo}/contents/{spec['pin_file']}?ref={ref}",
                ]
            )
    engine, before, after = moved(texts["base"], texts["head"])
    return {"engine": engine, "before": before, "candidate": after, "url": fields["url"]}


def state_path(number: int) -> str:
    return os.path.join(STATE_DIR, f"{number}.json")


def load_state(number: int) -> dict:
    try:
        with open(state_path(number), encoding="utf-8") as handle:
            return json.load(handle)
    except FileNotFoundError as err:
        raise Refusal(
            f"no trial of pull request {number} was started here; run `make engine-trial PR={number}` first"
        ) from err


def installed(engine: str, settings: dict[str, str]) -> tuple[str | None, str | None]:
    """The install's record as text, and what the server says it is; None where there is none."""
    spec = ENGINES[engine]
    record_path = os.path.join(settings["resource_root"], spec["record"])
    try:
        with open(record_path, encoding="utf-8") as handle:
            record = handle.read()
    except FileNotFoundError:
        record = None
    server = settings.get(spec["server_key"], "")
    line = None
    if server and os.path.isfile(server):
        _, out = run([server, "--version"], LIMITS["short"])
        line = version_line(engine, out)
    return record, line


def this_machine() -> str:
    system = platform.system()
    if system == "Darwin":
        return plain(f"macOS {platform.mac_ver()[0]} {platform.machine()}")
    return plain(f"{system} {platform.release()} {platform.machine()}")


def backend(program: str) -> str:
    """The acceleration gglib detects here, in its own word."""
    code, out = run([program, "config", "llama", "detect", "--json"], LIMITS["short"])
    try:
        found = json.loads(out[out.index("{") :]).get("acceleration") if code == 0 else None
    except (ValueError, json.JSONDecodeError):
        found = None
    return plain(str(found)) if found else "unknown"


# --- The three commands -------------------------------------------------------


def start(number: int) -> int:
    program = gglib()
    pr = pull_request(number)
    engine, candidate = pr["engine"], pr["candidate"]
    spec = ENGINES[engine]
    settings = read_settings(said([program, "config", "paths"]))
    record, _ = installed(engine, settings)

    # The record of what was there is saved once: a second `start` must not
    # take the candidate's own record for it.
    os.makedirs(STATE_DIR, exist_ok=True)
    try:
        before = load_state(number).get("record_before")
    except Refusal:
        before = record
    state = {"pr": number, "engine": engine, "before": pr["before"], "candidate": candidate, "record_before": before}
    with open(state_path(number), "w", encoding="utf-8") as handle:
        json.dump(state, handle, indent=2)

    print(f"Pull request {number} moves {spec['product']} from {pr['before']} to {candidate}.")
    print(f"Installing {candidate} over your {spec['product']}, with {program}.\n")
    env = dict(os.environ, **{spec["env"]: candidate})
    code, _ = run([program, "config", spec["cli"], "install", "--force"], LIMITS["install"], env=env, shown=True)
    if code != 0:
        raise Refusal(f"the install did not finish (exit {code}); nothing was approved. See what it said above.")

    _, line = installed(engine, settings)
    if not line or not names_release(engine, line, candidate):
        raise Refusal(f"what is installed does not say it is {candidate}: {line or 'it did not answer --version'}")
    print(f"\n✓ {spec['product']} {candidate} is installed: {line}")
    print("\nUse gglib as you like. Then:")
    print(f"  make engine-trial-approve PR={number}   # it works: let the bump merge")
    print(f"  make engine-trial-revert PR={number}    # put back what you had")
    return 0


def approve(number: int, print_only: bool) -> int:
    program = gglib()
    state = load_state(number)
    engine, candidate = state["engine"], state["candidate"]
    spec = ENGINES[engine]
    pr = pull_request(number)
    if (pr["engine"], pr["candidate"]) != (engine, candidate):
        raise Refusal(
            f"pull request {number} now proposes {ENGINES[pr['engine']]['product']} {pr['candidate']}, "
            f"and what was installed for it is {spec['product']} {candidate}; start the trial again"
        )

    settings = read_settings(said([program, "config", "paths"]))
    record, line = installed(engine, settings)
    if not record:
        raise Refusal(f"{spec['product']} has no install record; start the trial again")
    if not line or not names_release(engine, line, candidate):
        raise Refusal(
            f"what is installed now does not say it is {candidate}: {line or 'it did not answer --version'}. "
            "Start the trial again."
        )
    since = record_time(record)
    used = used_since(settings["database_path"], since)
    refusal = not_used(engine, used, since)
    if refusal:
        raise Refusal(refusal)

    fields = json.loads(record)
    facts = {
        "engine": engine,
        "release": candidate,
        "binary": plain(line),
        "platform": plain(str(fields.get("platform") or fields.get("acceleration") or "unknown")),
        "installed_at": since.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "machine": this_machine(),
        "backend": backend(program),
        "gglib": plain(settings.get("version", "unknown")),
        "used": used,
    }
    text = comment(facts)
    if print_only:
        sys.stdout.write(text)
        return 0

    print(f"This says on pull request {number} that {spec['product']} {candidate} works on this machine:\n")
    print(text)
    typed = ask(f"Type the release ({candidate}) to approve it, or anything else to stop: ")
    if typed != candidate:
        raise Refusal("not approved: the release typed was not the one tried")
    with tempfile.NamedTemporaryFile("w", suffix=".md", encoding="utf-8", delete=False) as handle:
        handle.write(text)
    try:
        said(["gh", "pr", "comment", str(number), "--body-file", handle.name])
    finally:
        os.unlink(handle.name)
    print(f"\n✓ Said so on {pr['url']}. The Engine trial workflow records it on the branch.")
    return 0


def ask(question: str) -> str:
    """One line typed by a person on this terminal; a refusal where there is none.

    The terminal is opened once to read and once to write. Python opens no
    text file both ways on what cannot seek, and a terminal cannot: one `r+`
    open fails on every terminal, with an error that reads here as there
    being none.
    """
    try:
        with open("/dev/tty", encoding="utf-8") as heard, open("/dev/tty", "w", encoding="utf-8") as shown:
            shown.write(question)
            shown.flush()
            return heard.readline().strip()
    except OSError as err:
        raise Refusal("there is no terminal to ask on: approving is a person's act; run this in your own terminal") from err


def revert(number: int) -> int:
    program = gglib()
    state = load_state(number)
    spec = ENGINES[state["engine"]]
    argv = [program, "config", spec["cli"], "install", "--force"]
    if was_built(state.get("record_before")):
        # A source build is put back as one; the download would replace it.
        argv.insert(-1, "--build")
    if state.get("record_before"):
        print(f"Putting back the pinned {spec['product']}, as it was installed before the trial.\n")
    else:
        print(f"{spec['product']} was not installed before the trial; installing the pinned release.\n")
    code, _ = run(argv, LIMITS["install"], shown=True)
    if code != 0:
        raise Refusal(f"the install did not finish (exit {code}). See what it said above.")
    os.unlink(state_path(number))
    return 0


# --- The self-test ------------------------------------------------------------

SAMPLE = {
    "engine": "sd",
    "release": "master-1094-3bad892",
    "binary": "stable-diffusion.cpp version unknown, commit 3bad892",
    "platform": "macOS universal (Metal)",
    "installed_at": "2026-11-17T09:30:00Z",
    "machine": "macOS 27.2 arm64",
    "backend": "Metal",
    "gglib": "0.38.0-rc.1",
    "used": {"replies": 3, "pictures": 1},
}

RUST = 'pub const {constant}: &str = "{release}";\n'


def planted_database(path: str, rows: list[tuple[str, str, str]]) -> None:
    """A database with gglib's `chat_messages` columns this script reads."""
    with sqlite3.connect(path) as db:
        db.execute("CREATE TABLE chat_messages (role TEXT, content TEXT, created_at TEXT)")
        db.executemany("INSERT INTO chat_messages VALUES (?, ?, ?)", rows)


def asked_in_a_child(typed: str | None) -> tuple[bool, str]:
    """What `ask` gives a child process, and whether its question was shown.

    The child has a terminal of its own with `typed` typed on it once the
    question shows, or, with `None`, no terminal at all. A refusal comes back
    as its words after `refused: `. A child still waiting after ten seconds is
    ended, and its answer is whatever it had said.
    """
    import pty  # Not on Windows, which has no /dev/tty for `ask` either.

    question = "release: "
    answer_out, answer_in = os.pipe()
    pid, terminal = (os.fork(), None) if typed is None else pty.fork()
    if pid == 0:
        try:
            os.close(answer_out)
            if typed is None:
                os.setsid()
            try:
                heard = ask(question)
            except Refusal as refusal:
                heard = f"refused: {refusal}"
            os.write(answer_in, heard.encode())
        finally:
            os._exit(0)
    os.close(answer_in)
    reading = {answer_out} if terminal is None else {answer_out, terminal}
    shown, answer, sent = b"", b"", False
    deadline = time.monotonic() + 10
    while answer_out in reading and time.monotonic() < deadline:
        ready, _, _ = select.select(list(reading), [], [], 0.2)
        for descriptor in ready:
            try:
                chunk = os.read(descriptor, 1024)
            except OSError:
                chunk = b""
            if not chunk:
                reading.discard(descriptor)
            elif descriptor == answer_out:
                answer += chunk
            else:
                shown += chunk
        if terminal is not None and not sent and question.encode() in shown:
            os.write(terminal, f"{typed}\n".encode())
            sent = True
    try:
        os.kill(pid, 9)
    except ProcessLookupError:
        pass
    os.waitpid(pid, 0)
    os.close(answer_out)
    if terminal is not None:
        os.close(terminal)
    return question.encode() in shown, answer.decode()


def self_test() -> bool:
    failures: list[str] = []

    def check(name: str, ok: bool, detail: object = "") -> None:
        if not ok:
            failures.append(f"{name}{': ' + str(detail) if detail != '' else ''}")

    def refused(name: str, call, wanted: str) -> None:
        try:
            call()
        except Refusal as refusal:
            check(name, wanted in str(refusal), refusal)
        else:
            check(name, False, "it was not refused")

    def files(llama: str, sd: str) -> dict[str, str]:
        return {
            "llama": RUST.format(constant="PINNED_LLAMA_RELEASE", release=llama),
            "sd": "//! notes\n" + RUST.format(constant="PINNED_SD_RELEASE", release=sd),
        }

    # 1. Which engine a pull request moves, and to what.
    base = files("b12359", "master-1025-8ee08e3")
    check("an sd bump is read", moved(base, files("b12359", "master-1094-3bad892")) == ("sd", "master-1025-8ee08e3", "master-1094-3bad892"))
    check("a llama bump is read", moved(base, files("b12400", "master-1025-8ee08e3")) == ("llama", "b12359", "b12400"))
    refused("no pin moved", lambda: moved(base, base), "moves no engine's pin")
    refused("both pins moved", lambda: moved(base, files("b12400", "master-1094-3bad892")), "both engines' pins")
    refused("a pin that is no tag", lambda: moved(base, files("b12359", "latest")), "is not a stable-diffusion.cpp release tag")
    refused("a file without its constant", lambda: moved(base, {"llama": "", "sd": base["sd"]}), "PINNED_LLAMA_RELEASE is not in")

    # 2. What a server says it is, and whether that is the release.
    llama_said = "load_backend: loaded Metal backend\nversion: 1.0.0-dev (build 12359, commit b1b770d90)\nbuilt with clang\n"
    line = version_line("llama", llama_said)
    check("llama's version line is the `version:` one", line == "version: 1.0.0-dev (build 12359, commit b1b770d90)", line)
    check("llama's build is its release's number", names_release("llama", line or "", "b12359"))
    check("another build is not the release", not names_release("llama", line or "", "b1235"))
    sd_line = version_line("sd", "\nstable-diffusion.cpp version unknown, commit 3bad892\n")
    check("sd's version line is the first with anything on it", sd_line == SAMPLE["binary"], sd_line)
    check("sd's commit is its release's", names_release("sd", sd_line or "", "master-1094-3bad892"))
    check("another commit is not the release", not names_release("sd", sd_line or "", "master-1094-3bad89"))
    check("a server that says nothing has no line", version_line("sd", "\n\n") is None)

    # 3. When an install was made, from either shape of record.
    download = '{"version": "master-1094-3bad892", "platform": "macOS universal (Metal)", "install_type": "prebuilt", "installed_at": "2026-11-17T19:30:00.123456+10:00"}'
    built = '{"version": "b12359", "commit_sha": "b1b770d", "build_date": "2026-11-17T09:30:00Z", "acceleration": "Metal", "cmake_flags": []}'
    at = datetime.datetime(2026, 11, 17, 9, 30, tzinfo=datetime.timezone.utc)
    check("a download's time is read in UTC", record_time(download).replace(microsecond=0) == at, record_time(download))
    check("a source build's time is read", record_time(built) == at, record_time(built))
    check("only a source build was built", was_built(built) and not was_built(download) and not was_built(None))
    refused("a record that is not JSON", lambda: record_time("{"), "is not JSON")
    refused("a record with no time", lambda: record_time("{}"), "has no time in it")

    # 4. What was used since: rows before the install do not count, a picture
    #    is a tool's row that starts `Drew`, and `gglib draw` leaves no row.
    with tempfile.TemporaryDirectory() as folder:
        path = os.path.join(folder, "gglib.db")
        planted_database(
            path,
            [
                ("assistant", "an answer from before", "2026-11-17 09:29:59"),
                ("tool", "Drew 1 image, 1024x1024 PNG, with flux in 70 s; the user can see it, you cannot.", "2026-11-17 09:29:59"),
                ("user", "a question", "2026-11-17 09:31:00"),
                ("assistant", "an answer", "2026-11-17 09:31:05"),
                ("tool", "read 3 files", "2026-11-17 09:32:00"),
                ("tool", "Drew 2 images, 512x512 PNG, with flux in 40 s; the user can see it, you cannot.", "2026-11-17 09:33:00"),
            ],
        )
        used = used_since(path, at)
        check("replies and pictures since the install are counted", used == {"replies": 1, "pictures": 1}, used)
        later = used_since(path, at + datetime.timedelta(hours=1))
        check("nothing since a later install", later == {"replies": 0, "pictures": 0}, later)
        check("a used llama.cpp is a trial", not_used("llama", used, at) is None)
        check("a used stable-diffusion.cpp is a trial", not_used("sd", used, at) is None)
        check("an unused llama.cpp says to chat", "has saved no reply" in (not_used("llama", later, at) or ""))
        said_sd = not_used("sd", {"replies": 5, "pictures": 0}, at) or ""
        check("replies are not a trial of stable-diffusion.cpp", "A picture from `gglib draw` does not count" in said_sd, said_sd)
        refused("a database that is not there", lambda: used_since(os.path.join(folder, "none.db"), at), "could not be read")

    # 5. `gglib config paths`, and a line fit for a pull request.
    settings = read_settings("binary = /usr/local/bin/gglib\nversion = 0.38.0-rc.1\nnot a setting\nresource_root = /data\n")
    check("paths are read by key", settings == {"binary": "/usr/local/bin/gglib", "version": "0.38.0-rc.1", "resource_root": "/data"}, settings)
    check("a shown line has no backtick or control character", plain("a`b\x1b[0mc\n") == "ab[0mc", plain("a`b\x1b[0mc\n"))
    check("every sample value is plain", all(PLAIN.match(v) for v in SAMPLE.values() if isinstance(v, str)))

    # 6. The comment is the sample record_trial.sh's self-test reads.
    try:
        with open(FIXTURE, encoding="utf-8") as handle:
            sample = handle.read()
    except FileNotFoundError:
        sample = None
    check("the comment is scripts/fixtures/engine-trial-comment.txt", comment(SAMPLE) == sample, FIXTURE)

    # 7. The release is asked for on a terminal: a real one, since a terminal
    #    is what `ask` opens, and nowhere else. Windows has neither.
    if os.name != "nt":
        asked = asked_in_a_child("  master-1094-3bad892 ")
        check("the release is asked for on a terminal, and what is typed comes back", asked == (True, "master-1094-3bad892"), asked)
        _, nowhere = asked_in_a_child(None)
        check("with no terminal the question is refused", nowhere.startswith("refused: there is no terminal to ask on"), nowhere)

    if failures:
        print("❌ self-test failed:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return False
    print(
        "✅ self-test: a bump is read, a release is told from a version line, use is counted, the comment is the shared sample, "
        "the release is asked for on a terminal",
        file=sys.stderr,
    )
    return True


def main() -> int:
    # What this prints and what gglib prints share the terminal, in order.
    sys.stdout.reconfigure(line_buffering=True)
    if sys.argv[1:] == ["--self-test"]:
        return 0 if self_test() else 1
    parser = argparse.ArgumentParser(description=(__doc__ or "").split("\n\n", 1)[0])
    commands = parser.add_subparsers(dest="command", required=True)
    for name, about in (
        ("start", "install the candidate over your engine"),
        ("approve", "say on the pull request that it works"),
        ("revert", "put back what was installed before"),
    ):
        command = commands.add_parser(name, help=about)
        command.add_argument("pr", type=int, help="the pin pull request's number")
        if name == "approve":
            command.add_argument("--print", action="store_true", dest="print_only", help="print the comment and post nothing")
    args = parser.parse_args()
    try:
        if args.command == "start":
            return start(args.pr)
        if args.command == "approve":
            return approve(args.pr, args.print_only)
        return revert(args.pr)
    except Refusal as refusal:
        print(f"✗ {refusal}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
