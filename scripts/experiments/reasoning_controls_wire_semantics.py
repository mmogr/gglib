#!/usr/bin/env python3
"""What do `reasoning_effort` and `reasoning_budget_tokens` actually do on the wire?

ADR 0007 defers effort detection to llama-server's `chat_template_caps`
self-report and pairs the template-dependent effort control with the
sampler-enforced `reasoning_budget_tokens`. Every claim in that document that
matters at the wire level is re-measured here rather than transcribed from
source: the exact shape of the caps object, its independence from `--jinja`,
what each effort level renders to, what junk values do, which templates get
the budget keys into slot params, and what `"none"` really renders (the
retraction's live check — ADR 0007 finding 4).

Unlike `sampler_wire_semantics.py`, which attaches to a server you launched,
this script launches its own servers: the measurement varies *launch* state
(`--chat-template-file`, `--jinja`/`--no-jinja`), so an attach-mode harness
could only ever see one configuration. Every server it starts is killed on
exit — including on crash and Ctrl-C — and the port is verified released.

Run it pointing at the pinned llama.cpp checkout:

    python3 scripts/experiments/reasoning_controls_wire_semantics.py \
        --server .llama/bin/llama-server \
        --model  ~/Library/Caches/llama.cpp/ggml-org_models_tinyllamas_stories15M-q4_0.gguf \
        --templates .llama/llama.cpp/models/templates

The model's output is gibberish (stories15M). Irrelevant: this measures the
wire — request parsing, prompt rendering, slot-param echo — not quality.

## The methodological traps

1. **The rendered prompt is captured via `/apply-template`, and that is only
   valid because it is the same parse.** `post_apply_template` calls the
   identical `oaicompat_chat_params_parse` as `/v1/chat/completions`
   (`tools/server/server-context.cpp:4862-4872`, pin `10bf611`) and returns
   the prompt without inference. A log-scraping capture would measure the
   same thing with more moving parts; a *different* endpoint would measure
   nothing. If upstream ever forks the parse paths, this capture silently
   stops being evidence — the citation is the tripwire.

2. **`params` appears only on an actively-processing slot** (trap 2 of
   `sampler_wire_semantics.py`), and a 15M model decodes so fast the window
   is tiny. The budget probes ask for a large `n_predict` and poll `/slots`
   in a tight loop with no sleep between the launch and the first poll. A
   missed window reports `unmeasurable`, never a fabricated absence.

3. **`chat_template_caps` is a report, not a conservative baseline.** Five
   of its nine bools default `true` upstream (`common/jinja/caps.h`), so
   "absent key" and "false" and "true-by-default" are three different facts.
   M1 records the object verbatim for exactly this reason.

4. **The budget is NOT wire-observable on this pin — neither the echo nor
   the gate.** Two separate absences, both measured rather than assumed:

   *No echo.* The schema stores `reasoning_budget_tokens` into
   `params.sampling` (`server-schema.cpp:383`), but `task_params::to_json`
   (`server-task.cpp:32-147`) serialises no `reasoning_budget_*` field in
   either branch, so neither `/slots` nor
   `/props → default_generation_settings.params` can ever echo it —
   debug mode included (servers here run `LLAMA_SERVER_SLOTS_DEBUG=1` to
   prove the absence in the *full* params branch, not just the metrics
   one).

   *No gate signal either.* The thinking-tags gate
   (`server-common.cpp:1345` — `!chat_params.thinking_end_tags.empty()`)
   only controls whether server-common *injects* the tag/message keys;
   the client's own `reasoning_budget_tokens` rides through the
   copy-remaining-properties loop (`server-common.cpp:1366-1370`)
   regardless, so schema validation — including the hard
   `-1 <= v <= INT32_MAX` 400 on `-2` — fires identically on every
   template. The first draft of this harness used that 400 as a gate
   side-channel; the identical 400 on the tagless config C is the
   refutation, kept in the probe as evidence that the gate has no wire
   signal at all. Its one observable consequence — the forced
   end-of-thinking tag — needs a model that actually opens a thinking
   block, which a gibberish 15M model never does.

## What this does NOT measure

Whether any effort level changes model *behaviour* — ADR 0007 already
concedes that is permanently unobservable at the sampling boundary and
per-template folklore besides (finding 3). This pins the wire contract only.

Exit status is 0 whatever the findings — this reports, it does not judge.
Exit 2 means the harness could not run, which is different and must stay so.
"""

from __future__ import annotations

import argparse
import atexit
import difflib
import json
import os
import signal
import socket
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
from pathlib import Path

# ── The nine caps keys upstream defines (common/jinja/caps.h) ────────────────

EXPECTED_CAPS_KEYS = (
    "supports_tools",
    "supports_tool_calls",
    "supports_system_role",
    "supports_parallel_tool_calls",
    "supports_preserve_reasoning",
    "supports_reasoning_effort",
    "supports_string_content",
    "supports_typed_content",
    "supports_object_arguments",
)

# Config A must be a template that *declares* reasoning_effort. gpt-oss is the
# primary; the fallbacks are the other declaring templates from ADR 0007's
# census, tried in order if a template fails to launch/render on this model —
# the measurement targets the server's behaviour, not gpt-oss specifically.
DECLARING_TEMPLATES = (
    "openai-gpt-oss-120b.jinja",
    "deepseek-ai-DeepSeek-V4.jinja",
    "upstage-Solar-Open-100B.jinja",
)
THINKING_TEMPLATE = "Qwen-Qwen3-0.6B.jinja"  # thinking tags, no reasoning_effort

EFFORT_LEVELS = ("low", "high", "xhigh")
JUNK_EFFORTS = (("banana", "banana"), ("int_42", 42), ("empty_string", ""))
BUDGET_VALUES = (0, 512, -1, "absent")

# Big enough that a stories15M decode is still running when the tight-loop
# poll lands; small ctx keeps the launch cheap.
BUDGET_MAX_TOKENS = 1500
CTX = 2048

STARTUP_TIMEOUT = 90.0


# ── Server lifecycle ─────────────────────────────────────────────────────────
#
# Every Popen is tracked in a module-level registry killed by atexit AND by
# signal handlers AND by the per-config finally. Redundant on purpose: the
# absolute rule this harness runs under is "leave no process behind", and the
# three paths cover crash, Ctrl-C, and normal exit respectively.

_LIVE_PROCS: list[subprocess.Popen] = []


def _kill(proc: subprocess.Popen) -> None:
    if proc.poll() is not None:
        return
    proc.terminate()
    try:
        proc.wait(timeout=8)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=8)


def _kill_all() -> None:
    for proc in _LIVE_PROCS:
        try:
            _kill(proc)
        except Exception:  # noqa: BLE001 — cleanup must reach every proc
            pass


atexit.register(_kill_all)


def _signal_exit(signum: int, _frame: object) -> None:
    _kill_all()
    sys.exit(2)


signal.signal(signal.SIGINT, _signal_exit)
signal.signal(signal.SIGTERM, _signal_exit)


def find_free_port(lo: int, hi: int) -> int:
    """A port in [lo, hi] that binds cleanly right now."""
    for port in range(lo, hi + 1):
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            try:
                s.bind(("127.0.0.1", port))
            except OSError:
                continue
            return port
    raise SystemExit(f"error: no free port in {lo}-{hi}")


def port_released(port: int, wait: float = 10.0) -> bool:
    """True once nothing is listening on the port any more."""
    deadline = time.monotonic() + wait
    while time.monotonic() < deadline:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            if s.connect_ex(("127.0.0.1", port)) != 0:
                return True
        time.sleep(0.25)
    return False


class Server:
    """One llama-server launch, guaranteed dead when the `with` block ends."""

    def __init__(self, argv: list[str], port: int, log_path: Path):
        self.argv = argv
        self.port = port
        self.base = f"http://127.0.0.1:{port}"
        self.log_path = log_path
        self.proc: subprocess.Popen | None = None

    def __enter__(self) -> "Server":
        log = self.log_path.open("wb")
        # Full (non-metrics) slot params, so "no budget key" is measured in
        # the branch with the most keys, not just the trimmed one — trap 4.
        env = dict(os.environ, LLAMA_SERVER_SLOTS_DEBUG="1")
        self.proc = subprocess.Popen(  # noqa: S603 — argv is built from our own flags
            self.argv, stdout=log, stderr=subprocess.STDOUT, env=env
        )
        _LIVE_PROCS.append(self.proc)
        deadline = time.monotonic() + STARTUP_TIMEOUT
        while time.monotonic() < deadline:
            if self.proc.poll() is not None:
                raise ServerDied(
                    f"exited rc={self.proc.returncode} before /health; "
                    f"log: {self.log_path}"
                )
            if get(self.base, "/health", 2) is not None:
                return self
            time.sleep(0.25)
        raise ServerDied(f"no /health within {STARTUP_TIMEOUT}s; log: {self.log_path}")

    def __exit__(self, *exc: object) -> None:
        if self.proc is not None:
            _kill(self.proc)
            if self.proc in _LIVE_PROCS:
                _LIVE_PROCS.remove(self.proc)
        if not port_released(self.port):
            print(
                f"warning: port {self.port} still held after kill — "
                "check `lsof -i :{self.port}` by hand",
                file=sys.stderr,
            )


class ServerDied(Exception):
    """The launch failed; the config (not the harness) may be at fault."""


# ── Transport ────────────────────────────────────────────────────────────────


def get(base: str, path: str, timeout: int) -> dict | list | None:
    try:
        with urllib.request.urlopen(f"{base}{path}", timeout=timeout) as r:
            return json.loads(r.read())
    except Exception:  # noqa: BLE001
        return None


def post(base: str, path: str, body: dict, timeout: int) -> tuple[int, dict | str]:
    """Return (status, parsed-or-raw). Never raises: a 400 is data, not a crash."""
    req = urllib.request.Request(
        f"{base}{path}",
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.status, json.loads(r.read())
    except urllib.error.HTTPError as e:
        raw = e.read().decode(errors="replace")
        try:
            return e.code, json.loads(raw)
        except json.JSONDecodeError:
            return e.code, raw
    except Exception as e:  # noqa: BLE001
        return 0, str(e)


def render_prompt(base: str, extra: dict, timeout: int) -> tuple[int, str]:
    """The rendered prompt for a one-message conversation, via /apply-template.

    Same parse path as /v1/chat/completions — see trap 1 in the module doc.
    """
    body = {"messages": [{"role": "user", "content": "hi"}], **extra}
    status, resp = post(base, "/apply-template", body, timeout)
    if status == 200 and isinstance(resp, dict) and "prompt" in resp:
        return status, resp["prompt"]
    return status, resp if isinstance(resp, str) else json.dumps(resp)


def chat(base: str, extra: dict, timeout: int, max_tokens: int = 8) -> tuple[int, dict | str]:
    body = {
        "messages": [{"role": "user", "content": "hi"}],
        "max_tokens": max_tokens,
        "stream": False,
        **extra,
    }
    return post(base, "/v1/chat/completions", body, timeout)


def prompt_diff(baseline: str, changed: str) -> list[str]:
    """The changed lines only — the evidence without two page-long prompts."""
    return [
        line
        for line in difflib.unified_diff(
            baseline.splitlines(), changed.splitlines(), lineterm="", n=0
        )
        if line[:1] in "+-" and line[:3] not in ("+++", "---")
    ]


def budget_slot_capture(
    base: str, extra: dict, timeout: int
) -> tuple[dict | None, int, dict | str]:
    """Slot `params` mid-generation, plus the completed response.

    Trap 2: tight-loop polling, because a 15M model's decode window is small.
    """
    result: dict = {}

    def run() -> None:
        result["resp"] = chat(base, extra, timeout, max_tokens=BUDGET_MAX_TOKENS)

    t = threading.Thread(target=run, daemon=True)
    t.start()
    captured: dict | None = None
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline and captured is None:
        slots = get(base, "/slots", 5)
        if isinstance(slots, list):
            for s in slots:
                if s.get("is_processing") and isinstance(s.get("params"), dict):
                    captured = s["params"]
                    break
        if captured is None and not t.is_alive():
            break
    t.join(timeout=timeout)
    status, resp = result.get("resp", (0, "request thread never finished"))
    return captured, status, resp


def budget_keys(params: dict | None) -> dict:
    if params is None:
        return {}
    return {
        k: v
        for k, v in params.items()
        if k.startswith("reasoning_budget") or k == "reasoning_control"
    }


# ── Probes ───────────────────────────────────────────────────────────────────


def probe_caps(base: str, timeout: int) -> dict:
    """M1/M2 — the exact chat_template_caps object off /props."""
    props = get(base, "/props", timeout)
    if not isinstance(props, dict):
        return {"status": "unmeasurable", "why": "GET /props returned nothing usable"}
    caps = props.get("chat_template_caps")
    gen_params = (props.get("default_generation_settings") or {}).get("params") or {}
    return {
        "status": "ok",
        "caps_key_present": "chat_template_caps" in props,
        "caps": caps,
        "keys_verbatim": sorted(caps) if isinstance(caps, dict) else None,
        "missing_expected_keys": (
            sorted(set(EXPECTED_CAPS_KEYS) - set(caps)) if isinstance(caps, dict) else None
        ),
        "unexpected_keys": (
            sorted(set(caps) - set(EXPECTED_CAPS_KEYS)) if isinstance(caps, dict) else None
        ),
        "props_reasoningish_default_params": {
            k: v
            for k, v in gen_params.items()
            if "reasoning" in k or "think" in k
        },
        "build_info": props.get("build_info"),
    }


def probe_effort_rendering(base: str, timeout: int) -> dict:
    """M3 — what each level renders to, against the absent-effort baseline."""
    b_status, baseline = render_prompt(base, {}, timeout)
    if b_status != 200:
        return {"status": "unmeasurable", "why": f"baseline render HTTP {b_status}"}
    out: dict = {"status": "ok", "baseline_prompt": baseline, "levels": {}}
    for level in EFFORT_LEVELS:
        r_status, prompt = render_prompt(base, {"reasoning_effort": level}, timeout)
        c_status, _ = chat(base, {"reasoning_effort": level}, timeout)
        out["levels"][level] = {
            "apply_template_http": r_status,
            "chat_completions_http": c_status,
            "prompt_changed": prompt != baseline,
            "diff_vs_baseline": prompt_diff(baseline, prompt) if r_status == 200 else None,
        }
    return out


def probe_effort_junk(base: str, timeout: int) -> dict:
    """M4 — unknown level, wrong type, empty string."""
    b_status, baseline = render_prompt(base, {}, timeout)
    out: dict = {"status": "ok", "cases": {}}
    for name, value in JUNK_EFFORTS:
        r_status, prompt = render_prompt(base, {"reasoning_effort": value}, timeout)
        c_status, c_resp = chat(base, {"reasoning_effort": value}, timeout)
        err = ""
        if isinstance(c_resp, dict) and "error" in c_resp:
            err = str(c_resp["error"])[:160]
        out["cases"][name] = {
            "sent": value,
            "apply_template_http": r_status,
            "chat_completions_http": c_status,
            "chat_error": err,
            "prompt_changed_vs_baseline": (
                prompt != baseline if r_status == 200 and b_status == 200 else None
            ),
            "diff_vs_baseline": (
                prompt_diff(baseline, prompt)
                if r_status == 200 and b_status == 200
                else None
            ),
        }
    return out


def probe_effort_none(base: str, timeout: int) -> dict:
    """M7 — the retraction's live check: does "none" fall back to medium?"""
    b_status, baseline = render_prompt(base, {}, timeout)
    r_status, prompt = render_prompt(base, {"reasoning_effort": "none"}, timeout)
    if r_status != 200:
        return {"status": "unmeasurable", "why": f"render HTTP {r_status}: {prompt[:200]}"}
    return {
        "status": "ok",
        "http": r_status,
        "prompt_equals_absent_baseline": b_status == 200 and prompt == baseline,
        "diff_vs_baseline": prompt_diff(baseline, prompt) if b_status == 200 else None,
        "rendered_reasoning_lines": [
            line for line in prompt.splitlines() if "easoning" in line
        ],
    }


def probe_budget_wire(base: str, timeout: int) -> dict:
    """M5/M6 — which reasoning_budget_* keys hit slot params, per sent value."""
    out: dict = {"status": "ok", "sent": {}}
    for value in BUDGET_VALUES + (-2,):
        extra = {} if value == "absent" else {"reasoning_budget_tokens": value}
        params, http, resp = budget_slot_capture(base, extra, timeout)
        row: dict = {
            "chat_completions_http": http,
            "slot_captured": params is not None,
            "budget_keys_in_slot_params": budget_keys(params),
        }
        if params is not None and "all_slot_param_keys" not in out:
            out["all_slot_param_keys"] = sorted(params)
        if isinstance(resp, dict) and http == 200:
            msg = resp.get("choices", [{}])[0].get("message", {})
            row["finish_reason"] = resp.get("choices", [{}])[0].get("finish_reason")
            row["has_reasoning_content"] = bool(msg.get("reasoning_content"))
            row["reasoning_content_len"] = len(msg.get("reasoning_content") or "")
            row["content_len"] = len(msg.get("content") or "")
        elif http != 200:
            row["error"] = str(resp)[:200]
        out["sent"][str(value)] = row
    # The alias upstream reads as the inner default of the same json_value
    # chain (server-common.cpp:1338-1339).
    params, http, resp = budget_slot_capture(
        base, {"thinking_budget_tokens": 256}, timeout
    )
    out["alias_thinking_budget_tokens_256"] = {
        "chat_completions_http": http,
        "slot_captured": params is not None,
        "budget_keys_in_slot_params": budget_keys(params),
    }
    return out


def probe_budget_gate_only(base: str, timeout: int) -> dict:
    """M5 per template: what wire signal does the thinking-tags gate leave?

    Answer measured here: none (trap 4). The -2 validation 400 fires on
    every template because the client field bypasses the gate via the
    copy-remaining-properties loop, and the slot echo never carries the
    budget keys. Both sends are kept so each config's identical result is
    recorded evidence, not an argument from source.
    """
    minus2_http, minus2_resp = chat(base, {"reasoning_budget_tokens": -2}, timeout)
    err = ""
    if isinstance(minus2_resp, dict) and "error" in minus2_resp:
        err = str(minus2_resp["error"])[:200]
    params, http, _resp = budget_slot_capture(
        base, {"reasoning_budget_tokens": 512}, timeout
    )
    return {
        "status": "ok",
        "minus2_http": minus2_http,
        "minus2_error": err,
        "send_512_http": http,
        "slot_captured": params is not None,
        "budget_keys_in_slot_params": budget_keys(params),
        "all_slot_param_keys": sorted(params) if params else None,
    }


# ── Report ───────────────────────────────────────────────────────────────────


def hr(title: str) -> None:
    print(f"── {title} " + "─" * max(4, 74 - len(title)))
    print()


def show_caps(tag: str, r: dict) -> None:
    if r["status"] != "ok":
        print(f"  [{tag}] UNMEASURABLE: {r['why']}")
        print()
        return
    print(f"  [{tag}] chat_template_caps present: {r['caps_key_present']}")
    if isinstance(r["caps"], dict):
        for k in sorted(r["caps"]):
            print(f"      {k:34} {r['caps'][k]}")
        if r["missing_expected_keys"]:
            print(f"      missing expected keys: {r['missing_expected_keys']}")
        if r["unexpected_keys"]:
            print(f"      unexpected keys:       {r['unexpected_keys']}")
    else:
        print(f"      value: {r['caps']!r}")
    if r["props_reasoningish_default_params"]:
        print(f"      reasoning-ish default_generation_settings.params: "
              f"{r['props_reasoningish_default_params']}")
    print()


def report(results: dict, note: str) -> None:
    print()
    print("═" * 78)
    print("  Reasoning controls wire semantics — effort + budget, per ADR 0007")
    for tag in ("A", "B", "C"):
        cfg = results["configs"].get(tag, {})
        print(f"  {tag}: {cfg.get('desc', '(did not launch)')}")
    if note:
        print(f"  note: {note}")
    print("═" * 78)
    print()

    hr("M1 — chat_template_caps shape, per config")
    for tag in ("A", "B", "C"):
        show_caps(tag, results["m1"].get(tag, {"status": "unmeasurable", "why": "not probed"}))

    hr("M2 — caps under --no-jinja / explicit --jinja (config A)")
    for tag in ("A_no_jinja", "A_jinja"):
        show_caps(tag, results["m2"].get(tag, {"status": "unmeasurable", "why": "not probed"}))
    a = results["m1"].get("A", {}).get("caps")
    for tag in ("A_no_jinja", "A_jinja"):
        b = results["m2"].get(tag, {}).get("caps")
        if isinstance(a, dict) and isinstance(b, dict):
            print(f"  identical to config A: {tag}: {a == b}")
    print()

    hr("M3 — effort rendering (A declares, B does not)")
    for tag in ("A", "B"):
        r = results["m3"].get(tag)
        if not r:
            continue
        if r["status"] != "ok":
            print(f"  [{tag}] UNMEASURABLE: {r['why']}")
            print()
            continue
        for level, row in r["levels"].items():
            print(f"  [{tag}] {level:7} apply-template HTTP {row['apply_template_http']}, "
                  f"chat HTTP {row['chat_completions_http']}, "
                  f"prompt changed: {row['prompt_changed']}")
            for line in row["diff_vs_baseline"] or []:
                print(f"        {line}")
        print()

    hr("M4 — junk effort values (config A)")
    r = results["m4"]
    if r["status"] == "ok":
        for name, c in r["cases"].items():
            print(f"  {name:14} sent={c['sent']!r:10} apply HTTP {c['apply_template_http']}, "
                  f"chat HTTP {c['chat_completions_http']}, "
                  f"prompt changed: {c['prompt_changed_vs_baseline']}")
            for line in c["diff_vs_baseline"] or []:
                print(f"        {line}")
            if c["chat_error"]:
                print(f"        error: {c['chat_error']}")
    print()

    hr("M5/M6 — budget wire (B = thinking tags, the positive control)")
    r = results["m5_b"]
    if r["status"] == "ok":
        for sent, row in r["sent"].items():
            print(f"  sent {sent:>7}: chat HTTP {row['chat_completions_http']}, "
                  f"slot captured: {row['slot_captured']}")
            if row["budget_keys_in_slot_params"]:
                for k, v in sorted(row["budget_keys_in_slot_params"].items()):
                    print(f"        {k} = {json.dumps(v)[:120]}")
            if "finish_reason" in row:
                print(f"        finish={row['finish_reason']} "
                      f"reasoning_len={row['reasoning_content_len']} "
                      f"content_len={row['content_len']}")
            if row.get("error"):
                print(f"        error: {row['error']}")
        al = r["alias_thinking_budget_tokens_256"]
        print(f"  alias thinking_budget_tokens=256: chat HTTP {al['chat_completions_http']}, "
              f"slot captured: {al['slot_captured']}")
        for k, v in sorted(al["budget_keys_in_slot_params"].items()):
            print(f"        {k} = {json.dumps(v)[:120]}")
        if r.get("all_slot_param_keys") is not None:
            n_budget = sum(
                1 for k in r["all_slot_param_keys"] if k.startswith("reasoning_budget")
            )
            print(f"  slot params carry {len(r['all_slot_param_keys'])} keys, "
                  f"{n_budget} of them reasoning_budget_* "
                  f"(LLAMA_SERVER_SLOTS_DEBUG=1, full branch)")
    print()

    hr("M5 — budget wire signal per template (expect: none — trap 4)")
    for tag in ("A", "B", "C"):
        row = results["m5_gate"].get(tag)
        if not row:
            continue
        print(f"  [{tag}] -2 → HTTP {row['minus2_http']}"
              + (f" — {row['minus2_error']}" if row["minus2_error"] else ""))
        print(f"        512 → HTTP {row['send_512_http']}, slot captured: "
              f"{row['slot_captured']}, budget keys in echo: "
              f"{sorted(row['budget_keys_in_slot_params']) or 'NONE'}")
    rows = [results["m5_gate"].get(t) for t in ("A", "B", "C")]
    rows = [r for r in rows if r]
    if len(rows) == 3 and all(r["minus2_http"] == rows[0]["minus2_http"] for r in rows):
        print()
        print("  identical across all three templates → the thinking-tags gate")
        print("  leaves no wire signal; validation is universal, echo is absent.")
    print()

    hr("M7 — reasoning_effort \"none\" (the retraction's live check, config A)")
    r = results["m7"]
    if r["status"] == "ok":
        print(f"  HTTP {r['http']}, prompt identical to absent-effort baseline: "
              f"{r['prompt_equals_absent_baseline']}")
        for line in r["rendered_reasoning_lines"]:
            print(f"        rendered: {line.strip()}")
    else:
        print(f"  UNMEASURABLE: {r['why']}")
    print()


# ── Entry point ──────────────────────────────────────────────────────────────


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--server", type=Path, required=True, help="llama-server binary")
    p.add_argument("--model", type=Path, required=True, help="GGUF (tiny is fine)")
    p.add_argument(
        "--templates", type=Path, required=True,
        help="directory holding upstream's models/templates/*.jinja",
    )
    p.add_argument("--port-lo", type=int, default=18300)
    p.add_argument("--port-hi", type=int, default=18999)
    p.add_argument("--timeout", type=int, default=120)
    p.add_argument("--log-dir", type=Path, default=Path("/tmp"),
                   help="where per-config server logs go")
    p.add_argument("--note", default="", help="free text recorded in the header")
    p.add_argument("--json", action="store_true", help="emit raw results as JSON too")
    args = p.parse_args()

    server = args.server.expanduser()
    model = args.model.expanduser()
    templates = args.templates.expanduser()
    for path, what in ((server, "server binary"), (model, "model"), (templates, "templates dir")):
        if not path.exists():
            print(f"error: {what} not found: {path}", file=sys.stderr)
            return 2

    port = find_free_port(args.port_lo, args.port_hi)
    print(f"using port {port}", file=sys.stderr)

    def launch(tag: str, template: Path | None, extra_flags: list[str]) -> Server:
        argv = [
            str(server), "-m", str(model),
            "--host", "127.0.0.1", "--port", str(port),
            "-c", str(CTX), "-ngl", "0",
        ]
        if template is not None:
            argv += ["--chat-template-file", str(template)]
        argv += extra_flags
        return Server(argv, port, args.log_dir / f"reasoning_probe_{tag}.log")

    results: dict = {"configs": {}, "m1": {}, "m2": {}, "m3": {}, "m5_gate": {}}
    to = args.timeout

    # Config A: first declaring template that launches. Substitution is data,
    # not failure — the measurement targets the server, not gpt-oss.
    a_template: Path | None = None
    a_note = ""
    for name in DECLARING_TEMPLATES:
        candidate = templates / name
        if not candidate.is_file():
            continue
        try:
            with launch("A", candidate, []) as srv:
                a_template = candidate
                results["configs"]["A"] = {"desc": f"--chat-template-file {name}"}
                if a_note:
                    results["configs"]["A"]["substituted"] = a_note
                print(f"config A up ({name}); probing ...", file=sys.stderr)
                results["m1"]["A"] = probe_caps(srv.base, to)
                results["m3"]["A"] = probe_effort_rendering(srv.base, to)
                results["m4"] = probe_effort_junk(srv.base, to)
                results["m7"] = probe_effort_none(srv.base, to)
                results["m5_gate"]["A"] = probe_budget_gate_only(srv.base, to)
            break
        except ServerDied as exc:
            a_note += f"{name} failed ({exc}); "
            print(f"config A: {name} failed, trying next: {exc}", file=sys.stderr)
    if a_template is None:
        print("error: no declaring template would launch — harness cannot run",
              file=sys.stderr)
        return 2

    # M2: config A's template under --no-jinja and explicit --jinja.
    for tag, flags in (("A_no_jinja", ["--no-jinja"]), ("A_jinja", ["--jinja"])):
        try:
            with launch(tag, a_template, flags) as srv:
                print(f"config {tag} up; probing caps ...", file=sys.stderr)
                results["m2"][tag] = probe_caps(srv.base, to)
        except ServerDied as exc:
            results["m2"][tag] = {"status": "unmeasurable", "why": str(exc)}

    # Config B: thinking tags, no reasoning_effort.
    b_template = templates / THINKING_TEMPLATE
    try:
        with launch("B", b_template, []) as srv:
            results["configs"]["B"] = {"desc": f"--chat-template-file {THINKING_TEMPLATE}"}
            print("config B up; probing ...", file=sys.stderr)
            results["m1"]["B"] = probe_caps(srv.base, to)
            results["m3"]["B"] = probe_effort_rendering(srv.base, to)
            results["m5_b"] = probe_budget_wire(srv.base, to)
            results["m5_gate"]["B"] = probe_budget_gate_only(srv.base, to)
    except ServerDied as exc:
        results["m1"]["B"] = {"status": "unmeasurable", "why": str(exc)}
        results["m5_b"] = {"status": "unmeasurable", "why": str(exc)}

    # Config C: no template flag at all — the model's own template (or the
    # server's fallback), which for stories15M means no thinking tags.
    try:
        with launch("C", None, []) as srv:
            results["configs"]["C"] = {"desc": "no --chat-template-file (built-in)"}
            print("config C up; probing ...", file=sys.stderr)
            results["m1"]["C"] = probe_caps(srv.base, to)
            results["m5_gate"]["C"] = probe_budget_gate_only(srv.base, to)
    except ServerDied as exc:
        results["m1"]["C"] = {"status": "unmeasurable", "why": str(exc)}

    results.setdefault("m4", {"status": "unmeasurable", "why": "config A never probed"})
    results.setdefault("m7", {"status": "unmeasurable", "why": "config A never probed"})
    results.setdefault("m5_b", {"status": "unmeasurable", "why": "config B never probed"})

    report(results, args.note)

    if args.json:
        print(json.dumps(results, indent=2, default=str))

    released = port_released(port)
    print(f"port {port} released: {released}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
