#!/usr/bin/env python3
"""Install a CI job's system packages: from a cache when it can, and from a
mirror that keeps to a time budget when it cannot.

The `apt-packages` action runs this, and it is the one place a workflow
installs a system package (`check_system_packages.sh` holds that). A job that
downloads every package from the runner's first Ubuntu mirror on every run is
one slow mirror away from its timeout: a mirror can serve tens of kB/s for
hours (#2102).

`key` names the cache entry. The key is the release (its id, version and
architecture), the packages asked for, and a hash of every package installed
on the system, each read here when the job runs. Two runs with one key start
from the same system and ask for the same thing, so what the first downloaded
installs on the second.

`install` keeps apt's package lists and downloaded packages in a directory of
its own, which the action restores and saves, and leaves apt's system
directories alone:

1. With lists in the directory, it installs with `--no-download`. When that
   works, no mirror was asked for anything.
2. Otherwise it downloads. A runner that reads its mirrors from a list (a
   `mirror+file:` source) fails over only on an error, never on a slow
   transfer, so each mirror gets `--budget-seconds` for `apt-get update` and
   the download together. Past that, the mirror the list ranks first is taken
   off the list and the download carries on from the next, keeping what has
   arrived. No mirror is named here: the list is the runner's own, found in
   apt's sources.
3. A budget is for finding a faster mirror, never a reason to fail. After
   every mirror has had its turn, the download runs once more from the
   runner's list as it was, with no budget, and that run's failure is the
   job's. A runner with no mirror list, or one mirror, gets only that run.
4. It installs from the directory, removes from it the packages no index
   offers any more, puts the mirror list back as it was and hands the
   directory to the user the job runs as.

It prints `source` (`cache` or `mirror`) and `mirrors-dropped` to
`$GITHUB_OUTPUT`.

Usage:
    ./scripts/apt_packages.py key --packages "<names>"
    ./scripts/apt_packages.py install --packages "<names>" --dir <dir> [--budget-seconds N]
    ./scripts/apt_packages.py mirrors
    ./scripts/apt_packages.py --self-test

Exit status:
    0  done, or the self-test passed
    1  the self-test failed, or the arguments were refused
    apt's own status when a download or an install failed
"""

import argparse
import glob
import hashlib
import os
import re
import subprocess
import sys
import tempfile
import time
from typing import List, NamedTuple, Optional, Sequence, Tuple

KEY_VERSION = "v1"
SOURCES = ("/etc/apt/sources.list", "/etc/apt/sources.list.d/*.list", "/etc/apt/sources.list.d/*.sources")
PACKAGE = re.compile(r"[a-z0-9][a-z0-9+.:-]*")
MIRROR_FILE = re.compile(r"mirror\+file:(/[^\s\]]+)")
PRIORITY = re.compile(r"priority:(\d+)")


class Attempt(NamedTuple):
    """One download: how many mirrors are off the list, and its budget."""

    dropped: int
    budget: Optional[int]


def package_names(text: str) -> List[str]:
    """The packages asked for, sorted, each once. Refuses what is not a name,
    so nothing in the list can reach apt as an option."""
    names = sorted(set(text.split()))
    for name in names:
        if not PACKAGE.fullmatch(name):
            raise ValueError(f"'{name}' is not a package name")
    if not names:
        raise ValueError("no package was named")
    return names


def cache_key(os_id: str, version: str, arch: str, installed: str, names: Sequence[str]) -> Tuple[str, str, str]:
    """The cache key, and the two prefixes a run restores from when no entry
    has its key: first the newest entry for these packages on another state
    of the same release, then the newest for the release whatever it holds.
    Either's lists are stale, and most of its packages are still the ones
    wanted.

    The key ends in two hashes of eight digits, the packages and then the
    system, as a Rust cache's key ends in two. `ci.yml`'s `cache-prune`
    reads that shape and keeps the newest two entries per key less its last
    hash, so main holds two per package list: one for each runner image
    while GitHub moves from one to the next."""
    release = f"apt-packages-{KEY_VERSION}-{os_id}{version}-{arch}-"
    packages = release + hashlib.sha256("\n".join(names).encode()).hexdigest()[:8] + "-"
    return packages + hashlib.sha256(installed.encode()).hexdigest()[:8], packages, release


def mirror_files(sources: Sequence[str]) -> List[str]:
    """The mirror lists apt's sources name, in order, each once. A commented
    line names none."""
    found: List[str] = []
    for text in sources:
        for line in text.splitlines():
            if line.lstrip().startswith("#"):
                continue
            for path in MIRROR_FILE.findall(line):
                if path not in found:
                    found.append(path)
    return found


def ranked(text: str) -> List[int]:
    """The line numbers of the list's mirrors, the one apt asks first at the
    front. apt takes a lower `priority:` first, and a mirror with none after
    every mirror that has one; mirrors that tie keep the file's order here."""
    mirrors = []
    for number, line in enumerate(text.splitlines()):
        entry = line.strip()
        if not entry or entry.startswith("#"):
            continue
        priority = PRIORITY.search(entry)
        mirrors.append((0 if priority else 1, int(priority.group(1)) if priority else 0, number))
    return [number for _, _, number in sorted(mirrors)]


def without_first(text: str, dropped: int) -> str:
    """The list with its `dropped` first-ranked mirrors taken off. The last
    mirror is never taken off: a list with none would be an error, not a
    slower download."""
    order = ranked(text)
    gone = set(order[: min(dropped, max(len(order) - 1, 0))])
    lines = [line for number, line in enumerate(text.splitlines()) if number not in gone]
    return "\n".join(lines) + "\n"


def plan(mirrors: int, budget: int) -> List[Attempt]:
    """Each mirror in turn with a budget, where there is another to turn to,
    then the list as it was with none."""
    attempts = [Attempt(dropped, budget) for dropped in range(mirrors)] if mirrors > 1 else []
    return attempts + [Attempt(0, None)]


# ---------------------------------------------------------------------------
# What touches the system
# ---------------------------------------------------------------------------


def say(message: str) -> None:
    print(message, flush=True)


def root() -> List[str]:
    return [] if os.geteuid() == 0 else ["sudo"]


def apt(directory: str, arguments: Sequence[str], budget: Optional[float] = None) -> int:
    """Run apt-get against the directory's lists and packages. Past `budget`
    seconds it is stopped, and the status is `timeout`'s 124."""
    command = root()
    if budget is not None:
        command += ["timeout", "--kill-after=15", str(max(int(budget), 1))]
    command += [
        "env",
        "DEBIAN_FRONTEND=noninteractive",
        "apt-get",
        "-o", f"Dir::State::lists={directory}/lists",
        "-o", f"Dir::Cache::archives={directory}/archives",
        # The two binary caches are built from the system's lists; left on,
        # apt would read them for these.
        "-o", "Dir::Cache::pkgcache=",
        "-o", "Dir::Cache::srcpkgcache=",
        # Lists kept compressed and without translations: less to download,
        # and less in the cache entry.
        "-o", "Acquire::GzipIndexes=true",
        "-o", "Acquire::Languages=none",
        "-o", "Dpkg::Use-Pty=0",
        *arguments,
    ]
    say(f"$ apt-get {' '.join(arguments)}" + (f"   (budget {max(int(budget), 1)}s)" if budget is not None else ""))
    return subprocess.run(command, check=False).returncode


def read(path: str) -> Optional[str]:
    try:
        with open(path, encoding="utf-8", errors="replace") as handle:
            return handle.read()
    except OSError:
        return None


def mirror_list() -> Tuple[Optional[str], Optional[str]]:
    """The runner's mirror list and its text, or neither."""
    texts = [text for pattern in SOURCES for path in sorted(glob.glob(pattern)) if (text := read(path)) is not None]
    for path in mirror_files(texts):
        text = read(path)
        if text is not None:
            return path, text
    return None, None


def write_as_root(path: str, text: str) -> None:
    with tempfile.NamedTemporaryFile("w", encoding="utf-8", delete=False) as handle:
        handle.write(text)
    try:
        subprocess.run(root() + ["cp", handle.name, path], check=True)
        subprocess.run(root() + ["chmod", "644", path], check=True)
    finally:
        os.unlink(handle.name)


def has_lists(directory: str) -> bool:
    lists = os.path.join(directory, "lists")
    return os.path.isdir(lists) and any(
        name != "lock" and os.path.isfile(os.path.join(lists, name)) for name in os.listdir(lists)
    )


def download(directory: str, names: Sequence[str], budget_seconds: int) -> Tuple[int, int]:
    """Fetch the lists and the packages into the directory. Returns apt's
    status and how many mirrors were given up on."""
    path, original = mirror_list()
    if path is None or original is None:
        say("The runner reads its mirrors from no list, so there is none to turn to.")
        attempts = plan(0, budget_seconds)
    else:
        say(f"The runner's mirror list, {path}:")
        say("".join(f"    {line}\n" for line in original.splitlines()).rstrip("\n"))
        attempts = plan(len(ranked(original)), budget_seconds)

    status = 1
    gave_up = 0
    try:
        for attempt in attempts:
            if path is not None and original is not None:
                write_as_root(path, without_first(original, attempt.dropped) if attempt.dropped else original)
            if attempt.dropped:
                say(f"Carrying on without the first {attempt.dropped} of the list's mirrors.")
            # One budget for the lists and the packages together.
            started = time.monotonic()
            status = apt(directory, ["update"], attempt.budget)
            if status == 0:
                left = None if attempt.budget is None else attempt.budget - (time.monotonic() - started)
                status = apt(directory, ["install", "-y", "--download-only", *names], left)
            if status == 0:
                return 0, gave_up
            if attempt.budget is None:
                break
            gave_up += 1
            why = f"did not finish in {attempt.budget} seconds" if status == 124 else f"failed with status {status}"
            say(f"::warning title=slow apt mirror::The package download {why} from the mirror the runner's list ranks first; trying the next.")
    finally:
        if path is not None and original is not None:
            write_as_root(path, original)
    return status, gave_up


def install(directory: str, names: Sequence[str], budget_seconds: int) -> int:
    for part in ("lists/partial", "archives/partial"):
        os.makedirs(os.path.join(directory, part), exist_ok=True)

    source = "cache"
    gave_up = 0
    try:
        offline = 1
        if has_lists(directory):
            say("::group::Install from the cache, with no download")
            offline = apt(directory, ["install", "-y", "--no-download", *names])
            say("::endgroup::")
            if offline != 0:
                say("The cache does not hold everything this system needs; downloading the rest.")
        if offline != 0:
            source = "mirror"
            say("::group::Download")
            status, gave_up = download(directory, names, budget_seconds)
            say("::endgroup::")
            if status != 0:
                say(f"::error title=system packages::apt could not download {' '.join(names)} (status {status}).")
                return status
            say("::group::Install what was downloaded")
            status = apt(directory, ["install", "-y", "--no-download", *names])
            say("::endgroup::")
            if status != 0:
                say(f"::error title=system packages::apt could not install {' '.join(names)} (status {status}).")
                return status
            # Drop the packages no index offers any more, so an entry that
            # starts from an older one does not carry every version it ever held.
            apt(directory, ["autoclean"])
    finally:
        # apt wrote as root; the cache action reads as the job's user.
        subprocess.run(root() + ["chown", "-R", f"{os.getuid()}:{os.getgid()}", directory], check=False)

    say(f"System packages installed from the {source}" + (f", after giving up on {gave_up} mirror(s)." if gave_up else "."))
    output = os.environ.get("GITHUB_OUTPUT")
    if output:
        with open(output, "a", encoding="utf-8") as handle:
            handle.write(f"source={source}\nmirrors-dropped={gave_up}\n")
    return 0


def key(names: Sequence[str]) -> int:
    release = dict(
        line.rstrip("\n").split("=", 1) for line in (read("/etc/os-release") or "").splitlines() if "=" in line
    )
    os_id = release.get("ID", "linux").strip('"')
    version = release.get("VERSION_ID", "").strip('"')
    arch = subprocess.run(["dpkg", "--print-architecture"], check=True, capture_output=True, text=True).stdout.strip()
    installed = subprocess.run(
        ["dpkg-query", "-W", "-f", "${binary:Package} ${Version}\n"], check=True, capture_output=True, text=True
    ).stdout
    cache, packages, release_prefix = cache_key(os_id, version, arch, installed, names)
    directory = os.path.join(os.environ.get("RUNNER_TEMP") or tempfile.gettempdir(), "apt-packages")
    lines = f"key={cache}\nrestore-packages={packages}\nrestore-release={release_prefix}\ndir={directory}\n"
    say(lines.rstrip("\n"))
    output = os.environ.get("GITHUB_OUTPUT")
    if output:
        with open(output, "a", encoding="utf-8") as handle:
            handle.write(lines)
    return 0


# ---------------------------------------------------------------------------
# Self-test
# ---------------------------------------------------------------------------

# A runner's list as apt reads it: tab-separated, the regional mirror first.
THREE = "http://near.example/ubuntu/\tpriority:1\nhttp://main.example/ubuntu/\tpriority:2\nhttp://security.example/ubuntu/\tpriority:3\n"
# The file's order is not the rank: priority is.
SHUFFLED = "http://b.example/ubuntu/ priority:2\n# a comment\n\nhttp://c.example/ubuntu/\nhttp://a.example/ubuntu/ priority:1\n"
DEB822 = "Types: deb\nURIs: mirror+file:/etc/apt/apt-mirrors.txt\nSuites: noble noble-updates\nComponents: main\n"
ONE_LINE = "deb [arch=amd64] mirror+file:/etc/apt/mirrors.txt noble main\n# deb mirror+file:/etc/apt/old.txt noble main\n"
PLAIN = "Types: deb\nURIs: http://archive.example/ubuntu/\nSuites: noble\n"


def self_test() -> bool:
    failures: List[str] = []
    total = 0

    def check(name: str, got: object, want: object) -> None:
        nonlocal total
        total += 1
        if got != want:
            failures.append(f"{name}: got {got!r}, want {want!r}")

    check("names are sorted, each once", package_names("libb-dev\n  liba-dev libb-dev"), ["liba-dev", "libb-dev"])
    check("a name with an architecture is a name", package_names("libstdc++6:amd64"), ["libstdc++6:amd64"])
    for bad in ("-o", "--allow-unauthenticated", "Lib", "a;b", ""):
        total += 1
        try:
            package_names(bad)
            failures.append(f"'{bad}' was taken as a package list")
        except ValueError:
            pass

    system = "a 1\nb 2\n"
    one, packages, release = cache_key("ubuntu", "24.04", "amd64", system, ["x", "y"])
    check("the release prefix is the system's release and architecture", release, "apt-packages-v1-ubuntu24.04-amd64-")
    check("the packages prefix is the release prefix and a hash", bool(re.fullmatch(re.escape(release) + "[0-9a-f]{8}-", packages)), True)
    check("the key ends in two hashes, as cache-prune reads it", bool(re.fullmatch(re.escape(packages) + "[0-9a-f]{8}", one)), True)
    check("the same system and packages give the same key", cache_key("ubuntu", "24.04", "amd64", system, ["x", "y"])[0], one)
    check("other packages give another packages prefix", cache_key("ubuntu", "24.04", "amd64", system, ["x", "z"])[1] != packages, True)
    check("another installed version keeps the packages prefix", cache_key("ubuntu", "24.04", "amd64", "a 1\nb 3\n", ["x", "y"])[1], packages)
    check("another installed version gives another key", cache_key("ubuntu", "24.04", "amd64", "a 1\nb 3\n", ["x", "y"])[0] != one, True)
    check("another release shares no prefix", cache_key("ubuntu", "26.04", "amd64", system, ["x", "y"])[2] != release, True)

    check("a deb822 source names its list", mirror_files([DEB822]), ["/etc/apt/apt-mirrors.txt"])
    check("a one-line source names its list, a commented one none", mirror_files([ONE_LINE]), ["/etc/apt/mirrors.txt"])
    check("a source with a plain URI names none", mirror_files([PLAIN]), [])
    check("each list once, in the sources' order", mirror_files([ONE_LINE, DEB822, ONE_LINE]), ["/etc/apt/mirrors.txt", "/etc/apt/apt-mirrors.txt"])

    check("rank follows priority", ranked(THREE), [0, 1, 2])
    check("rank follows priority, not the file's order, and none comes last", ranked(SHUFFLED), [4, 0, 3])
    check("nothing dropped is the list", without_first(THREE, 0), THREE)
    check("the first-ranked mirror is dropped", without_first(THREE, 1), "http://main.example/ubuntu/\tpriority:2\nhttp://security.example/ubuntu/\tpriority:3\n")
    check("the last mirror is never dropped", without_first(THREE, 5), "http://security.example/ubuntu/\tpriority:3\n")
    check("dropping keeps comments and the other lines' order", without_first(SHUFFLED, 1), "http://b.example/ubuntu/ priority:2\n# a comment\n\nhttp://c.example/ubuntu/\n")
    check("an empty list stays empty", without_first("", 1), "\n")

    check("three mirrors: each with a budget, then the list with none", plan(3, 240), [Attempt(0, 240), Attempt(1, 240), Attempt(2, 240), Attempt(0, None)])
    check("one mirror has none to turn to", plan(1, 240), [Attempt(0, None)])
    check("no list has none to turn to", plan(0, 240), [Attempt(0, None)])

    if not failures:
        print(f"✅ self-test: {total} planted cases, each as planted", file=sys.stderr)
        return True
    print("❌ self-test failed: the rules no longer give what the fixtures plant", file=sys.stderr)
    for failure in failures:
        print(f"     {failure}", file=sys.stderr)
    return False


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--self-test", action="store_true", help="run the fixtures only")
    commands = parser.add_subparsers(dest="command")
    for name, text in (("key", "print the cache key for these packages on this system"), ("install", "install the packages")):
        command = commands.add_parser(name, help=text)
        command.add_argument("--packages", required=True, help="package names, separated by spaces or newlines")
        if name == "install":
            command.add_argument("--dir", required=True, help="the directory the action restores and saves")
            command.add_argument("--budget-seconds", type=int, default=240, help="what each mirror gets")
    commands.add_parser("mirrors", help="print the runner's mirror list and where it is")
    args = parser.parse_args()

    if args.self_test:
        return 0 if self_test() else 1
    if args.command == "mirrors":
        path, text = mirror_list()
        say(f"{path}\n{text}" if path else "no mirror list")
        return 0
    if args.command not in ("key", "install"):
        parser.error("give --self-test, key, install or mirrors")
    try:
        names = package_names(args.packages)
    except ValueError as error:
        print(f"❌ {error}", file=sys.stderr)
        return 1
    if args.command == "key":
        return key(names)
    if args.budget_seconds < 1:
        parser.error("--budget-seconds is at least 1")
    return install(os.path.abspath(args.dir), names, args.budget_seconds)


if __name__ == "__main__":
    sys.exit(main())
