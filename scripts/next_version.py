#!/usr/bin/env python3
"""
Compute the version a Bump Version run moves to.

    scripts/next_version.py <bump> <current-version> [tag ...]

Prints the new version, or an error on stderr with exit status 1. Tags are the
repository's tags (`git tag -l`); only `vX.Y.Z-rc.N` ones are read.

The release flow this encodes:

    0.25.0 --rc-minor--> 0.26.0-rc.1 --next-rc--> 0.26.0-rc.2 --promote--> 0.26.0

  rc-patch, rc-minor, rc-major  start a cycle from a stable version
  next-rc                       the next candidate in the current cycle
  promote                       release the current candidate as stable

next-rc also accepts a stable version that was never tagged. That is main after
a promotion the release refused (check_rc_promotion.sh): the way on is another
candidate after the newest RC tag of that version, then promote again.
"""

import re
import sys

STABLE = re.compile(r"^(\d+)\.(\d+)\.(\d+)$")
RC = re.compile(r"^(\d+)\.(\d+)\.(\d+)-rc\.(\d+)$")


def latest_rc(base, tags):
    numbers = [
        int(m.group(4))
        for m in (RC.match(t[1:]) for t in tags if t.startswith("v"))
        if m and ".".join(m.groups()[:3]) == base
    ]
    return max(numbers, default=0)


def next_version(bump, current, tags):
    stable = STABLE.match(current)
    rc = RC.match(current)

    if bump in ("rc-patch", "rc-minor", "rc-major"):
        if not stable:
            raise ValueError(f"{current} is already a release candidate: use next-rc or promote")
        if f"v{current}" not in tags:
            raise ValueError(f"{current} was never released: use next-rc, then promote")
        major, minor, patch = map(int, stable.groups())
        if bump == "rc-major":
            major, minor, patch = major + 1, 0, 0
        elif bump == "rc-minor":
            minor, patch = minor + 1, 0
        else:
            patch += 1
        return f"{major}.{minor}.{patch}-rc.1"

    if bump == "next-rc":
        if rc:
            base = ".".join(rc.groups()[:3])
            n = max(int(rc.group(4)), latest_rc(base, tags))
            return f"{base}-rc.{n + 1}"
        if stable and f"v{current}" not in tags and latest_rc(current, tags):
            return f"{current}-rc.{latest_rc(current, tags) + 1}"
        raise ValueError(f"{current} is not in a release cycle: start one with rc-patch, rc-minor or rc-major")

    if bump == "promote":
        if not rc:
            raise ValueError(f"{current} is not a release candidate: nothing to promote")
        return ".".join(rc.groups()[:3])

    raise ValueError(f"unknown bump '{bump}'")


def main():
    if len(sys.argv) < 3:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    try:
        print(next_version(sys.argv[1], sys.argv[2], set(sys.argv[3:])))
    except ValueError as e:
        print(e, file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
