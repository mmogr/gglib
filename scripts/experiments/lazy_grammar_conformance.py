#!/usr/bin/env python3
"""Does llama.cpp already constrain tool-call arguments to the tool's JSON Schema?

The answer scopes `request_pipeline::constrain` (Tier A, see docs/adr/0001):
that stage originates a GBNF grammar for dialect models because upstream builds
none. But upstream ships `json_schema_to_grammar` and lazily-triggered
grammars, so the stage may already be redundant — or may be weaker than what is
available. This harness measures it instead of arguing about it.

Run against a RAW llama-server, not through gglib, or gglib's own
normalization and grammar origination confound every reading.

    llama-server -m Qwen3.5-4B.gguf --jinja --port 8081
    python3 scripts/experiments/lazy_grammar_conformance.py --port 8081

## The methodological trap

Sending ordinary prompts and observing conformant arguments proves nothing: a
capable model emits conformant arguments unaided. Distinguishing *enforcement*
from *luck* needs three things, all of which this harness does:

1. **Adversarial prompts.** Each one is written to tempt a specific violation
   ("read file number 42" invites `path: 42`).
2. **N samples at temperature 1.0.** A grammar constrains every draw; a
   well-behaved model does not. 30/30 conformant under temptation is
   enforcement; 22/30 is the model being good.
3. **`auto` vs `required` arms.** llama.cpp treats these differently (lazy
   trigger vs eager grammar). A gap between them localises where enforcement
   lives, which is exactly the seam the stage sits on.

## Second question: is gglib's parser still needed?

Independent of the grammar result. Upstream could constrain arguments
perfectly and still drop calls on the two bugs gglib fixed in #690, so a
second arm steers the model into generating each shape:

  R1 (#24807) a parameter value containing a literal `</parameter>`
  R2 (#20260) a reasoning model emitting prose before `<tool_call>`

These must be *generated*, not replayed as assistant history. An earlier
version of this arm did the latter and reported that the server "accepted"
the markup — which proved only that llama-server will carry arbitrary text in
a prior message, and never touched the parser. It passed for the wrong reason.

Exit status is 0 whatever the findings — this reports, it does not judge.
"""

from __future__ import annotations

import argparse
import json
import sys
import urllib.error
import urllib.request
from dataclasses import dataclass, field

# ── The probe tool ────────────────────────────────────────────────────────────
#
# One tool carrying several distinct constraint kinds, so a violation says
# *which* kind of enforcement is missing rather than just "something is wrong".

PROBE_TOOL = {
    "type": "function",
    "function": {
        "name": "read_file",
        "description": "Read a file from the workspace.",
        "parameters": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to the file."},
                "max_lines": {"type": "integer", "description": "Line cap."},
                "mode": {
                    "type": "string",
                    "enum": ["text", "binary"],
                    "description": "How to read the file.",
                },
                "options": {
                    "type": "object",
                    "properties": {"follow_symlinks": {"type": "boolean"}},
                    "required": ["follow_symlinks"],
                },
            },
            "required": ["path", "mode"],
            "additionalProperties": False,
        },
    },
}

# ── Adversarial prompts ───────────────────────────────────────────────────────
#
# Each names the violation it is trying to induce. A prompt that cannot tempt
# its violation measures nothing, so the temptation is the whole design.

PROMPTS: list[tuple[str, str]] = [
    ("type", "Read file number 42 in text mode. The file is literally named 42."),
    ("enum", "Read /etc/hosts in fast mode — I want the quickest possible read."),
    ("required", "Just read /etc/hosts. Don't worry about any other settings."),
    ("additional", "Read /etc/hosts in text mode, recursively, following all symlinks."),
    ("nested", "Read /etc/hosts in text mode with symlink options configured."),
    ("integer", "Read /etc/hosts in text mode, about three and a half lines."),
]


@dataclass
class Violations:
    """Which schema constraints a single tool call broke."""

    bad_name: bool = False
    missing_required: list[str] = field(default_factory=list)
    wrong_type: list[str] = field(default_factory=list)
    bad_enum: list[str] = field(default_factory=list)
    extra_keys: list[str] = field(default_factory=list)
    nested_missing: list[str] = field(default_factory=list)

    def any(self) -> bool:
        return (
            self.bad_name
            or bool(self.missing_required)
            or bool(self.wrong_type)
            or bool(self.bad_enum)
            or bool(self.extra_keys)
            or bool(self.nested_missing)
        )

    def labels(self) -> list[str]:
        out = []
        if self.bad_name:
            out.append("name")
        out += [f"missing:{k}" for k in self.missing_required]
        out += [f"type:{k}" for k in self.wrong_type]
        out += [f"enum:{k}" for k in self.bad_enum]
        out += [f"extra:{k}" for k in self.extra_keys]
        out += [f"nested:{k}" for k in self.nested_missing]
        return out


SCHEMA = PROBE_TOOL["function"]["parameters"]
PROPS = SCHEMA["properties"]

JSON_TYPES = {
    "string": str,
    "integer": int,
    "boolean": bool,
    "object": dict,
}


def check(name: str, args: dict) -> Violations:
    """Validate one tool call against the probe schema."""
    v = Violations()

    if name != PROBE_TOOL["function"]["name"]:
        v.bad_name = True

    for key in SCHEMA["required"]:
        if key not in args:
            v.missing_required.append(key)

    for key, value in args.items():
        spec = PROPS.get(key)
        if spec is None:
            v.extra_keys.append(key)
            continue

        expected = JSON_TYPES.get(spec["type"])
        # bool is a subclass of int in Python; an integer field must not accept
        # `true`, so the bool case is excluded explicitly.
        if expected is int and isinstance(value, bool):
            v.wrong_type.append(key)
        elif expected and not isinstance(value, expected):
            v.wrong_type.append(key)
        elif "enum" in spec and value not in spec["enum"]:
            v.bad_enum.append(key)

    opts = args.get("options")
    if isinstance(opts, dict) and "follow_symlinks" not in opts:
        v.nested_missing.append("follow_symlinks")

    return v


def post(url: str, payload: dict, timeout: int) -> dict:
    body = json.dumps(payload).encode()
    req = urllib.request.Request(
        url, data=body, headers={"Content-Type": "application/json"}
    )
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read())


def extract_calls(
    message: dict,
) -> tuple[list[tuple[str, dict]], str | None, dict | None]:
    """Structured tool calls from a response message, a note, and evidence.

    Returns `([], note, evidence)` when the model produced no usable call. The
    note distinguishes "emitted raw markup as text" — the failure gglib's
    parser exists for — from "declined to call a tool at all", which is a
    legitimate model choice and not a defect.

    `evidence` carries the **full** offending string when parsing fails. An
    earlier version of this harness truncated it into the note, which
    aggregated distinct failures under one bucket and left the actual cause
    unrecoverable — the reason this signature returns a third value at all.
    """
    calls = []
    for tc in message.get("tool_calls") or []:
        fn = tc.get("function", {})
        raw = fn.get("arguments", "")
        try:
            args = json.loads(raw) if isinstance(raw, str) else raw
        except json.JSONDecodeError as e:
            return (
                [],
                "unparseable arguments JSON",
                {"error": str(e), "len": len(raw), "raw": raw},
            )
        if not isinstance(args, dict):
            return (
                [],
                "arguments not an object",
                {"type": type(args).__name__, "raw": raw},
            )
        calls.append((fn.get("name", ""), args))

    if calls:
        return calls, None, None

    content = (message.get("content") or "") + (message.get("reasoning_content") or "")
    for marker in ("<tool_call>", "<function=", '{"name"'):
        if marker in content:
            return (
                [],
                f"raw markup survived as text (saw {marker!r})",
                {"content": content[:400]},
            )
    return [], "no tool call emitted", None


def run_arm(base: str, tool_choice, samples: int, temp: float, timeout: int) -> dict:
    """One `tool_choice` arm: every prompt sampled `samples` times."""
    stats = {
        "calls": 0,
        "conformant": 0,
        "violations": {},
        "notes": {},
        "examples": [],
        "evidence": [],
        "multi_call": 0,
    }

    for label, prompt in PROMPTS:
        for _ in range(samples):
            payload = {
                "messages": [{"role": "user", "content": prompt}],
                "tools": [PROBE_TOOL],
                "tool_choice": tool_choice,
                "temperature": temp,
                # Generous on purpose. A 256 cap was suspected of truncating
                # calls mid-arguments; measurement showed completions running
                # 107-156 tokens and finish_reason `tool_calls`, so the cap was
                # never binding — but a cap that cannot bind removes the
                # question from the result entirely.
                "max_tokens": 1024,
            }
            try:
                resp = post(f"{base}/v1/chat/completions", payload, timeout)
            except urllib.error.HTTPError as e:
                note = f"HTTP {e.code}: {e.read()[:120].decode(errors='replace')}"
                stats["notes"][note] = stats["notes"].get(note, 0) + 1
                continue
            except Exception as e:  # noqa: BLE001 - report, never abort the sweep
                note = f"{type(e).__name__}: {e}"
                stats["notes"][note] = stats["notes"].get(note, 0) + 1
                continue

            choice = resp.get("choices", [{}])[0]
            msg = choice.get("message", {})
            calls, note, evidence = extract_calls(msg)
            if note:
                stats["notes"][note] = stats["notes"].get(note, 0) + 1
                if evidence and len(stats["evidence"]) < 8:
                    stats["evidence"].append(
                        {
                            "tempted": label,
                            "note": note,
                            "finish_reason": choice.get("finish_reason"),
                            "completion_tokens": resp.get("usage", {}).get(
                                "completion_tokens"
                            ),
                            **evidence,
                        }
                    )
                continue

            if len(calls) > 1:
                stats["multi_call"] += 1

            for name, args in calls:
                stats["calls"] += 1
                v = check(name, args)
                if v.any():
                    for lbl in v.labels():
                        stats["violations"][lbl] = stats["violations"].get(lbl, 0) + 1
                    if len(stats["examples"]) < 5:
                        stats["examples"].append(
                            {"tempted": label, "name": name, "args": args}
                        )
                else:
                    stats["conformant"] += 1

    return stats


# ── Regression arm ────────────────────────────────────────────────────────────
#
# These bugs are about the parser mishandling what the model **generates**, so
# the model has to be steered into generating the shape. An earlier version of
# this arm replayed the markup as assistant *history* and reported that the
# server "accepted" it — which proved only that llama-server will carry
# arbitrary text in a prior message, and never exercised the parser at all. It
# passed for the wrong reason. This version induces the shape instead.
#
# Each case names what a *broken* parser would produce, so a failure is
# recognisable rather than merely different.

REGRESSIONS = [
    {
        "name": "R1 #24807",
        "desc": "argument value containing a literal </parameter>",
        "prompt": (
            "Read the file whose name is exactly a</parameter>b in text mode. "
            "The filename really does contain that text — pass it through "
            "unchanged."
        ),
        "expect_key": "path",
        "expect_value": "a</parameter>b",
        "broken_looks_like": "path truncated at the first </parameter> (e.g. 'a')",
    },
    {
        "name": "R2 #20260",
        "desc": "prose in the content channel before the tool call",
        "prompt": (
            "First write one short sentence explaining what you are about to "
            "do, then read /etc/hosts in text mode."
        ),
        "expect_key": "path",
        "expect_value": "/etc/hosts",
        "broken_looks_like": "no tool_call — parser fails on the leading prose",
    },
]


def run_regressions(base: str, samples: int, timeout: int) -> list[dict]:
    """Steer the model into each known-bad shape and inspect what comes back."""
    out = []
    for case in REGRESSIONS:
        result = {
            "name": case["name"],
            "desc": case["desc"],
            "broken_looks_like": case["broken_looks_like"],
            "attempts": 0,
            "calls": 0,
            "exact": 0,
            "observed": [],
        }
        for _ in range(samples):
            payload = {
                "messages": [{"role": "user", "content": case["prompt"]}],
                "tools": [PROBE_TOOL],
                "tool_choice": "auto",
                "temperature": 0.7,
                "max_tokens": 1024,
            }
            result["attempts"] += 1
            try:
                resp = post(f"{base}/v1/chat/completions", payload, timeout)
            except Exception as e:  # noqa: BLE001
                result["observed"].append(f"request failed: {type(e).__name__}")
                continue

            msg = resp.get("choices", [{}])[0].get("message", {})
            calls, note, _ = extract_calls(msg)
            if note:
                result["observed"].append(note)
                continue

            for name, args in calls:
                result["calls"] += 1
                got = args.get(case["expect_key"])
                if got == case["expect_value"]:
                    result["exact"] += 1
                else:
                    result["observed"].append(f"{case['expect_key']}={got!r}")
        out.append(result)
    return out


def pct(n: int, d: int) -> str:
    return "n/a" if d == 0 else f"{100 * n / d:.0f}%"


def report(arms: dict, regressions: list[dict], samples: int) -> None:
    total = samples * len(PROMPTS)
    print()
    print("═" * 74)
    print("  NATIVE SCHEMA CONFORMANCE — raw llama-server, no gglib in the path")
    print("═" * 74)
    print(f"  {len(PROMPTS)} adversarial prompts × {samples} samples = {total} requests per arm")
    print()
    print(f"  {'arm':<12} {'calls':>7} {'conformant':>12} {'rate':>8}   verdict")
    print(f"  {'-' * 12} {'-' * 7} {'-' * 12} {'-' * 8}   {'-' * 24}")

    for arm, s in arms.items():
        rate = pct(s["conformant"], s["calls"])
        if s["calls"] == 0:
            verdict = "no calls — inconclusive"
        elif s["conformant"] == s["calls"]:
            verdict = "ENFORCED (or lucky)"
        else:
            verdict = "NOT enforced"
        print(f"  {arm:<12} {s['calls']:>7} {s['conformant']:>12} {rate:>8}   {verdict}")

    for arm, s in arms.items():
        if s["violations"]:
            print()
            print(f"  {arm} violations by kind:")
            for k, n in sorted(s["violations"].items(), key=lambda kv: -kv[1]):
                print(f"      {k:<24} {n}")
        if s["examples"]:
            print(f"  {arm} sample bad calls:")
            for ex in s["examples"][:3]:
                print(f"      (tempted {ex['tempted']}) {ex['name']}({json.dumps(ex['args'])[:70]})")
        if s["notes"]:
            print(f"  {arm} non-call outcomes:")
            for k, n in sorted(s["notes"].items(), key=lambda kv: -kv[1]):
                print(f"      {k:<40} {n}")
        if s["multi_call"]:
            print(f"  {arm} responses carrying >1 tool call: {s['multi_call']}")
        if s["evidence"]:
            print(f"  {arm} FULL evidence for non-call outcomes:")
            for ev in s["evidence"]:
                print(f"      [{ev['tempted']}] {ev['note']}")
                print(
                    f"         finish_reason={ev.get('finish_reason')} "
                    f"completion_tokens={ev.get('completion_tokens')}"
                )
                if "error" in ev:
                    print(f"         json error: {ev['error']}")
                if "raw" in ev:
                    print(f"         raw ({ev.get('len', len(ev['raw']))} chars): {ev['raw']!r}")
                if "content" in ev:
                    print(f"         content: {ev['content']!r}")

    print()
    print("─" * 74)
    print("  PARSER REGRESSIONS — do the bugs gglib fixed in #690 still bite?")
    print("  (model steered into generating each shape; history replay proves")
    print("   nothing about the parser and is deliberately not used)")
    print("─" * 74)
    for r in regressions:
        print(f"  {r['name']:<11} {r['desc']}")
        print(
            f"              attempts={r['attempts']} calls={r['calls']} "
            f"exact_match={r['exact']}"
        )
        if r["exact"] == r["attempts"] and r["attempts"] > 0:
            print("              → PASS — upstream handled it every time")
        elif r["calls"] == 0:
            print("              → FAILS or untestable — no tool call produced")
        else:
            print("              → MIXED — see observations")
        print(f"              broken would look like: {r['broken_looks_like']}")
        if r["observed"]:
            seen = {}
            for o in r["observed"]:
                seen[o] = seen.get(o, 0) + 1
            for k, n in sorted(seen.items(), key=lambda kv: -kv[1])[:5]:
                print(f"                observed: {k}  ×{n}")
    print()


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--host", default="127.0.0.1")
    p.add_argument("--port", type=int, default=8081)
    p.add_argument("--samples", type=int, default=5, help="samples per prompt per arm")
    p.add_argument("--temp", type=float, default=1.0, help="high on purpose — see docstring")
    p.add_argument("--timeout", type=int, default=180)
    p.add_argument("--json", action="store_true", help="emit raw results as JSON too")
    args = p.parse_args()

    base = f"http://{args.host}:{args.port}"

    try:
        with urllib.request.urlopen(f"{base}/health", timeout=10) as r:
            r.read()
    except Exception as e:  # noqa: BLE001
        print(f"error: no llama-server at {base} ({e})", file=sys.stderr)
        print("start one with: llama-server -m <model.gguf> --jinja --port "
              f"{args.port}", file=sys.stderr)
        return 2

    arms = {}
    for label, choice in (("auto", "auto"), ("required", "required")):
        print(f"running arm: tool_choice={label} ...", file=sys.stderr)
        arms[label] = run_arm(base, choice, args.samples, args.temp, args.timeout)

    print("running regression arm ...", file=sys.stderr)
    regressions = run_regressions(base, max(3, args.samples), args.timeout)

    report(arms, regressions, args.samples)

    if args.json:
        print(json.dumps({"arms": arms, "regressions": regressions}, indent=2))

    return 0


if __name__ == "__main__":
    sys.exit(main())
