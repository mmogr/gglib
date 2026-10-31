#!/usr/bin/env python3
"""Record a scripted terminal session as an asciicast v2 file.

Usage: python3 record.py <script.json> <out.cast>

The script is a list of steps run against a bash in a pty of the size it
names, so what is recorded is what a terminal of that size would have shown.
A step is one of:

  {"type": "gglib up"}            typed a key at a time, as a person would;
                                  "delay" sets the seconds per key (0.045)
  {"keys": "\\r"}                  sent as is (Enter, Escape, arrows)
  {"wait": "regex", "timeout": 600}
                                  until what was printed since the last
                                  keystroke or the last match (the typed echo
                                  included) matches; with "pane": "<name>",
                                  what that pane printed since its last match
  {"idle": 2.0, "timeout": 600}   until nothing has been printed for that long
  {"sleep": 1.5}
  {"mark": "chapter:Point opencode at gglib"}
                                  a title card and a title-bar label from here on
  {"mark": "title:Working"}       the title-bar label alone, with no card
  {"mark": "speed:20"}            played 20x faster from here, with a badge
                                  saying so; "speed:1" ends it

The top level is {"cols", "rows", "env", "cwd", "panes", "steps"}. `env` is
added to this process's environment, with `~` expanded; `cwd` likewise.
`panes` is an optional list of {"name", "rows", "cmd"}: each command runs in
a pty of its own beside the shell, nobody types into it, and render.mjs draws
it under the main terminal with its name as a label. Its output is recorded
as "o@<name>" events. Long waits are recorded as they happened; render.mjs is
what shortens them.
"""

import codecs
import fcntl
import json
import os
import pty
import re
import select
import signal
import struct
import sys
import termios
import threading
import time

ANSI = re.compile(r"\x1b(\[[0-?]*[ -/]*[@-~]|\][^\x07\x1b]*(\x07|\x1b\\)|[@-Z\\-_])")


def main():
    script_path, out_path = sys.argv[1], sys.argv[2]
    script = json.load(open(script_path))
    cols, rows = script.get("cols", 100), script.get("rows", 30)

    env = dict(os.environ)
    for k, v in script.get("env", {}).items():
        env[k] = os.path.expanduser(v)
    env.update(TERM="xterm-256color", COLUMNS=str(cols), LINES=str(rows))
    cwd = os.path.expanduser(script.get("cwd", "."))

    def spawn(argv, pane_rows):
        pid, fd = pty.fork()
        if pid == 0:
            os.chdir(cwd)
            os.execvpe(argv[0], argv, env)
        fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", pane_rows, cols, 0, 0))
        return pid, fd

    pid, fd = spawn(["bash", "--noprofile", "--norc", "-i"], rows)

    start = time.monotonic()
    events = []
    # What has been printed since the last keystroke, which is what a `wait`
    # matches against: a prompt can land while the step before is draining.
    since_input = [""]
    seen_lock = threading.Lock()
    decoder = codecs.getincrementaldecoder("utf-8")("replace")

    def pump(timeout):
        """Read what is waiting; return the text, or None once the shell is gone."""
        r, _, _ = select.select([fd], [], [], timeout)
        if not r:
            return ""
        try:
            data = os.read(fd, 65536)
        except OSError:
            return None
        if not data:
            return None
        text = decoder.decode(data)
        if text:
            events.append([round(time.monotonic() - start, 4), "o", text])
            since_input[0] += text
        return text

    def follow(name, pane_fd):
        """Record a pane's output until it closes."""
        pane_decoder = codecs.getincrementaldecoder("utf-8")("replace")
        while True:
            try:
                data = os.read(pane_fd, 65536)
            except OSError:
                return
            if not data:
                return
            text = pane_decoder.decode(data)
            if text:
                events.append([round(time.monotonic() - start, 4), f"o@{name}", text])
                with seen_lock:
                    panes[name]["seen"] += text

    def drain(seconds):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            if pump(max(0.0, end - time.monotonic())) is None:
                return

    # A bare prompt, so the recording shows the commands and not this machine.
    os.write(fd, b"PS1='\\[\\e[1;32m\\]$\\[\\e[0m\\] '; clear\r")
    drain(0.8)
    # The prompt `clear` drew is already gone with what came before it, so the
    # recording opens on a clean screen with that prompt written back.
    start = time.monotonic()
    events[:] = [[0.0, "o", "\x1b[H\x1b[2J\x1b[1;32m$\x1b[0m "]]

    panes = {}
    for pane in script.get("panes", []):
        pane_pid, pane_fd = spawn(["bash", "-c", pane["cmd"]], pane["rows"])
        panes[pane["name"]] = {"pid": pane_pid, "seen": ""}
        threading.Thread(target=follow, args=(pane["name"], pane_fd), daemon=True).start()
    drain(0.3)

    for step in script["steps"]:
        if "type" in step or "keys" in step:
            since_input[0] = ""
        if "type" in step:
            for ch in step["type"]:
                os.write(fd, ch.encode())
                drain(step.get("delay", 0.045))
        elif "keys" in step:
            os.write(fd, step["keys"].encode())
            drain(0.05)
        elif "wait" in step:
            pattern = re.compile(step["wait"])
            deadline = time.monotonic() + step.get("timeout", 600)
            pane = panes[step["pane"]] if "pane" in step else None
            seen = lambda: pane["seen"] if pane else since_input[0]  # noqa: E731
            while not pattern.search(ANSI.sub("", seen())):
                if time.monotonic() > deadline:
                    sys.exit(f"timed out waiting for {step['wait']!r}")
                if pump(0.2) is None:
                    sys.exit(f"the shell exited while waiting for {step['wait']!r}")
            # Matched once; the next wait looks only at what comes after.
            if pane:
                with seen_lock:
                    pane["seen"] = ""
            else:
                since_input[0] = ""
        elif "idle" in step:
            deadline = time.monotonic() + step.get("timeout", 600)
            last = time.monotonic()
            while time.monotonic() - last < step["idle"]:
                if time.monotonic() > deadline:
                    sys.exit(f"still printing after {step.get('timeout', 600)}s")
                text = pump(0.1)
                if text is None:
                    break
                if text:
                    last = time.monotonic()
        elif "sleep" in step:
            drain(step["sleep"])
        elif "mark" in step:
            events.append([round(time.monotonic() - start, 4), "m", step["mark"]])

    os.kill(pid, signal.SIGHUP)
    for pane in panes.values():
        os.kill(pane["pid"], signal.SIGHUP)
    header = {"version": 2, "width": cols, "height": rows, "timestamp": int(time.time())}
    if "panes" in script:
        header["panes"] = [{"name": p["name"], "rows": p["rows"]} for p in script["panes"]]
    with open(out_path, "w") as f:
        f.write(json.dumps(header) + "\n")
        for e in sorted(events, key=lambda e: e[0]):
            f.write(json.dumps(e) + "\n")
    print(f"wrote {out_path}: {len(events)} events, {events[-1][0] if events else 0:.1f}s")


if __name__ == "__main__":
    main()
