#!/usr/bin/env python3
"""Which of gglib's force-written sampler values does llama.cpp already supply?

`InferenceConfig::with_hardcoded_defaults()` pins seven sampler values, and
`resolve_sampling` force-writes all of them into every request body. Some are
gglib policy. Others restate llama.cpp's own default — and one of those,
`min_p: 0.0`, was silently *disabling* a sampler until #739 caught it by hand.

ADR 0001 classifies the sampling hierarchy as Tier B ("llama.cpp will never do
this, structurally"). That is true of the ladder. It is not obviously true of
the floor beneath it: a floor value equal to the runtime's own default is not
policy, it is a redundant assertion. This harness measures which is which,
instead of arguing about it.

Run against a RAW llama-server, not through gglib:

    llama-server -m Qwen3.5-4B.gguf --jinja --port 8081 -c 8192 -ngl 99
    python3 scripts/experiments/sampler_wire_semantics.py --port 8081

## The methodological traps

Four, all of which cost real time to find, and three of which produce a
confidently wrong answer rather than an obvious failure.

1. **Never probe a server launched with sampler flags.** `/props` reports the
   *effective* defaults, so a server started with gglib's own
   `--temp/--top-p/--top-k/...` reports gglib's values back and you conclude
   they are upstream's. The `defaults` arm therefore refuses to run if it
   detects the values it is trying to measure. This is the same discipline as
   "run raw, not through gglib" in `lazy_grammar_conformance.py`, one layer
   lower.

2. **`params` appears only on an actively-processing slot.** Idle slots carry
   `id`, `n_ctx`, `speculative`, `is_processing` and nothing else. Poll an idle
   server and you will conclude this build has no `params` support at all. The
   echo probes hold a generation open and poll during it.

3. **`/slots.params` is an echo of the request, not the applied chain.** Set
   `mirostat: 2` and the reported `samplers` array is unchanged, still listing
   the full truncation chain. So this stream answers "what did llama-server
   parse out of the body", never "what did the model actually sample with".
   Anything built on it inherits that limit, and `probe_echo_or_applied`
   exists to keep the claim honest rather than assumed.

4. **Defaults could in principle be model-dependent.** If they were, the whole
   comparison would be a property of one GGUF rather than of the build. Run
   `--compare-model-defaults` against a second model and diff; it is one HTTP
   call and it converts an assumption into a line in the report.

## What this does NOT measure

Whether any of it produces better output. A value being redundant with
upstream's default says nothing about whether that default is good. Quality is
a separate instrument (`gglib benchmark agentic`) and a separate question.

gglib's floor is read out of the Rust source at runtime rather than copied
here, so this script cannot report a comparison against a floor that has since
moved. `tests/ts/contracts/settingsBounds.test.ts` uses the same trick for the
same reason.

Exit status is 0 whatever the findings — this reports, it does not judge.
Exit 2 means the harness could not run, which is different and must stay so.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import threading
import time
import urllib.error
import urllib.request
from pathlib import Path

# ── The seven force-written parameters ───────────────────────────────────────
#
# Keyed by the Rust field name in `with_hardcoded_defaults()`; valued by the
# key llama.cpp reports in `/props` and `/slots`. `max_tokens` maps to
# `n_predict` and the three unset DRY fields are listed so the report can show
# what gglib already defers rather than silently omitting them.

FORCE_WRITTEN = {
    "temperature": "temperature",
    "top_p": "top_p",
    "top_k": "top_k",
    "repeat_penalty": "repeat_penalty",
    "presence_penalty": "presence_penalty",
    "min_p": "min_p",
    "dry_multiplier": "dry_multiplier",
}

ALREADY_DEFERRED = {
    "max_tokens": "n_predict",
    "dry_base": "dry_base",
    "dry_allowed_length": "dry_allowed_length",
    "dry_penalty_last_n": "dry_penalty_last_n",
}

# Samplers llama.cpp exposes and `InferenceConfig` does not model at all.
# `frequency_penalty` is the notable one: it is a standard OpenAI field whose
# twin (`presence_penalty`) *is* modelled, so one is governed by the hierarchy
# and the other is not.
UNMODELLED = {
    "typical_p": 0.85,
    "xtc_probability": 0.35,
    "xtc_threshold": 0.22,
    "top_n_sigma": 1.5,
    "frequency_penalty": 0.66,
    "dynatemp_range": 0.4,
    "dynatemp_exponent": 1.2,
    "seed": 999,
}

# Deliberately odd values, chosen so no default could be mistaken for an echo.
ODD = {
    "temperature": 0.123,
    "top_k": 7,
    "min_p": 0.011,
    "top_p": 0.789,
    "repeat_penalty": 1.234,
    "presence_penalty": 0.456,
    "dry_multiplier": 0.321,
}

LONG_PROMPT = (
    "Write a long, detailed essay on the history of clockmaking. "
    "At least 800 words, covering escapements, marine chronometers, and quartz."
)

# Long enough that a slot is reliably still processing when the poll lands,
# short enough that the harness is not dominated by decode. Each echo probe
# must run to completion before the next one starts — otherwise the next
# probe's `/slots` read picks up the previous request's params and reports a
# confidently wrong echo.
ECHO_MAX_TOKENS = 256


# ── Transport ────────────────────────────────────────────────────────────────


def get(base: str, path: str, timeout: int) -> dict | list | None:
    try:
        with urllib.request.urlopen(f"{base}{path}", timeout=timeout) as r:
            return json.loads(r.read())
    except Exception:  # noqa: BLE001
        return None


def post_chat(base: str, body: dict, timeout: int) -> tuple[int, dict | str]:
    """Return (status, parsed-or-raw). Never raises: a 400 is data, not a crash."""
    req = urllib.request.Request(
        f"{base}/v1/chat/completions",
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


def generated_text(resp: dict) -> str:
    """Reasoning **and** content.

    A reasoning-tagged model at a low token budget returns an empty `content`
    with the whole generation in `reasoning_content` — the #621 shape. Hashing
    `content` alone reports every draw as identical, which is a determinism
    result that means nothing.
    """
    m = resp.get("choices", [{}])[0].get("message", {})
    return (m.get("reasoning_content") or "") + (m.get("content") or "")


def busy_slot_params(base: str, timeout: int) -> dict | None:
    """`params` off whichever slot is processing. See trap 2."""
    slots = get(base, "/slots", timeout)
    if not isinstance(slots, list):
        return None
    for s in slots:
        if s.get("is_processing") and "params" in s:
            return s["params"]
    return None


def params_during_generation(
    base: str, body: dict, timeout: int, polls: int = 6, gap: float = 1.5
) -> dict | None:
    """Hold a generation open and poll `/slots` until a busy slot appears."""
    captured: list[dict] = []

    def run() -> None:
        post_chat(base, body, timeout)

    t = threading.Thread(target=run, daemon=True)
    t.start()
    for _ in range(polls):
        time.sleep(gap)
        p = busy_slot_params(base, timeout=10)
        if p:
            captured.append(p)
            break
    t.join(timeout=timeout)
    return captured[0] if captured else None


# ── gglib's floor, read from the Rust source ─────────────────────────────────


def read_gglib_floor(repo_root: Path) -> dict[str, float | int | None]:
    """Parse `with_hardcoded_defaults()` out of `domain/inference.rs`.

    Copying the values into this file would let the script report a
    comparison against a floor that has since moved, which is precisely the
    class of staleness the experiment exists to detect. Raises with the symbol
    it could not find rather than defaulting, so a restructure of the Rust
    turns this red instead of silently retiring the guarantee.
    """
    src = repo_root / "crates/gglib-core/src/domain/inference.rs"
    if not src.is_file():
        raise SystemExit(f"error: cannot find {src} (pass --repo-root)")
    text = src.read_text()

    m = re.search(
        r"fn with_hardcoded_defaults\(\)\s*->\s*Self\s*\{(.*?)\n    \}",
        text,
        re.DOTALL,
    )
    if not m:
        raise SystemExit(
            "error: could not locate `with_hardcoded_defaults()` in inference.rs"
        )
    body = m.group(1)

    floor: dict[str, float | int | None] = {}
    for field in list(FORCE_WRITTEN) + list(ALREADY_DEFERRED):
        fm = re.search(rf"\b{field}:\s*(None|Some\(([^)]+)\))", body)
        if not fm:
            raise SystemExit(
                f"error: `{field}` not found in with_hardcoded_defaults() — "
                "the struct changed shape and this harness needs updating"
            )
        if fm.group(1) == "None":
            floor[field] = None
        else:
            floor[field] = float(fm.group(2).rstrip("f32").rstrip("_"))
    return floor


# ── Probes ───────────────────────────────────────────────────────────────────


def probe_defaults(base: str, timeout: int) -> dict:
    """Probe 1 — the authoritative upstream default table."""
    props = get(base, "/props", timeout)
    if not isinstance(props, dict):
        return {"status": "unmeasurable", "why": "GET /props returned nothing usable"}
    gen = props.get("default_generation_settings") or {}
    params = gen.get("params")
    if not isinstance(params, dict):
        return {
            "status": "unmeasurable",
            "why": "no default_generation_settings.params on this build",
        }
    return {
        "status": "ok",
        "build_info": props.get("build_info"),
        "params": params,
        "samplers": params.get("samplers"),
    }


def probe_echo_shape(base: str, timeout: int) -> dict:
    """Probe 3 — does `/slots` echo what we sent, and under what key names?"""
    body = {
        "messages": [{"role": "user", "content": LONG_PROMPT}],
        "max_tokens": ECHO_MAX_TOKENS,
        "stream": False,
        **ODD,
    }
    p = params_during_generation(base, body, timeout)
    if p is None:
        return {
            "status": "unmeasurable",
            "why": "no busy slot carried `params` (see trap 2)",
        }
    checks = {}
    for k, sent in ODD.items():
        got = p.get(k)
        checks[k] = {
            "sent": sent,
            "observed": got,
            "match": got is not None and abs(float(got) - float(sent)) < 1e-4,
        }
    return {"status": "ok", "checks": checks, "param_keys": sorted(p)}


def probe_echo_or_applied(base: str, timeout: int) -> dict:
    """Probe 3b — echo or applied chain? See trap 3.

    Enabling mirostat should displace the truncation samplers if this stream
    reports the *applied* chain. If `samplers` is unchanged and `top_k` still
    reads back, it is an echo of the parsed request.
    """
    body = {
        "messages": [{"role": "user", "content": LONG_PROMPT}],
        "mirostat": 2,
        "mirostat_tau": 4.0,
        "temperature": 0.123,
        "top_k": 7,
        "min_p": 0.011,
        "max_tokens": ECHO_MAX_TOKENS,
        "stream": False,
    }
    p = params_during_generation(base, body, timeout)
    if p is None:
        return {"status": "unmeasurable", "why": "no busy slot carried `params`"}
    return {
        "status": "ok",
        "mirostat": p.get("mirostat"),
        "top_k_still_reported": p.get("top_k"),
        "samplers": p.get("samplers"),
        "verdict": "echo of the request" if p.get("top_k") == 7 else "applied chain",
    }


def probe_body_vs_launch(base: str, timeout: int, launch_temp: float) -> dict:
    """Probe 4 — does the request body override a launch flag?"""
    body = {
        "messages": [{"role": "user", "content": LONG_PROMPT}],
        "temperature": 1.5,
        "top_k": 3,
        "max_tokens": ECHO_MAX_TOKENS,
        "stream": False,
    }
    p = params_during_generation(base, body, timeout)
    if p is None:
        return {"status": "unmeasurable", "why": "no busy slot carried `params`"}
    return {
        "status": "ok",
        "launch_temperature": launch_temp,
        "body_temperature": 1.5,
        "observed_temperature": p.get("temperature"),
        "winner": "body" if abs(float(p.get("temperature", 0)) - 1.5) < 1e-4 else "launch",
    }


def probe_unmodelled(base: str, timeout: int) -> dict:
    """Probe 5 — do samplers gglib does not model reach the server?"""
    body = {
        "messages": [{"role": "user", "content": LONG_PROMPT}],
        "max_tokens": ECHO_MAX_TOKENS,
        "stream": False,
        **UNMODELLED,
    }
    p = params_during_generation(base, body, timeout)
    if p is None:
        return {"status": "unmeasurable", "why": "no busy slot carried `params`"}
    checks = {}
    for k, sent in UNMODELLED.items():
        got = p.get(k)
        checks[k] = {
            "sent": sent,
            "observed": got,
            "accepted": got is not None and abs(float(got) - float(sent)) < 1e-4,
        }
    return {"status": "ok", "checks": checks}


def probe_tolerance(base: str, timeout: int) -> dict:
    """Probe 5b — what malformed values does upstream accept?

    Calibrates gglib's own coercion policy against upstream's rather than
    inventing one. `from_openai_json` currently rejects the whole client layer
    on any of these; upstream's answer is the reference.
    """
    cases = {
        "unknown_key": {"gglib_nonsense_key": 123},
        "max_tokens_negative_one": {"max_tokens": -1},
        "top_k_as_float": {"top_k": 40.0},
        "temperature_as_string": {"temperature": "0.7"},
    }
    out = {}
    for name, extra in cases.items():
        body = {
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 8,
            "stream": False,
            **extra,
        }
        status, payload = post_chat(base, body, timeout)
        msg = ""
        if isinstance(payload, dict) and "error" in payload:
            msg = str(payload["error"].get("message", ""))[:120]
        out[name] = {"sent": extra, "status": status, "error": msg}
    return {"status": "ok", "cases": out}


def probe_determinism(base: str, timeout: int, samples: int) -> dict:
    """Probe 7 — the noise floor.

    Without this, no later A/B can attribute an output change to a config
    change. A fixed seed collapsing to one hash means arms can be *paired* on
    seeds, which is a far more sensitive design than comparing distributions.

    # The control matters more than the result

    The first version of this probe asked for three primary colours at
    temperature 0.8. On Qwen3.5 the unseeded arm gave 5 distinct draws and the
    seeded arm gave 1, which looks like proof. On Llama-3.2-3B *both* arms gave
    1 — because a 3B instruct model answering a closed factual question emits
    the same tokens whatever the seed. The probe reported "seeding is
    deterministic" from a comparison in which nothing could have varied.

    So the unseeded arm is the control, not a garnish: if it does not vary,
    the seeded arm proves nothing and this reports `inconclusive`. The prompt
    is open-ended and the temperature high specifically to give the control
    something to detect.
    """
    prompt = (
        "Invent a name for a fictional seaside town, then describe its harbour "
        "in two sentences. Be imaginative and specific."
    )

    def hashes(seed: int | None) -> list[str]:
        out = []
        for _ in range(samples):
            body = {
                "messages": [{"role": "user", "content": prompt}],
                "temperature": 1.0,
                "max_tokens": 80,
                "stream": False,
            }
            if seed is not None:
                body["seed"] = seed
            status, resp = post_chat(base, body, timeout)
            if status != 200 or not isinstance(resp, dict):
                out.append(f"ERR:{status}")
                continue
            out.append(hashlib.sha256(generated_text(resp).encode()).hexdigest()[:12])
        return out

    fixed = hashes(12345)
    free = hashes(None)
    n_fixed, n_free = len(set(fixed)), len(set(free))

    if n_free == 1:
        verdict = "inconclusive — the unseeded control did not vary either"
    elif n_fixed == 1:
        verdict = "seeding is deterministic; arms can be PAIRED on fixed seeds"
    else:
        verdict = "seeding did NOT collapse the draws — pairing is unavailable"

    return {
        "status": "ok",
        "fixed_seed": fixed,
        "fixed_seed_distinct": n_fixed,
        "default_seed": free,
        "default_seed_distinct": n_free,
        "verdict": verdict,
    }


# ── Report ───────────────────────────────────────────────────────────────────


def fmt(v: object) -> str:
    if isinstance(v, float):
        return f"{round(v, 6):g}"
    return "—" if v is None else str(v)


def report_floor(defaults: dict, floor: dict, arm: str) -> list[dict]:
    print("── The floor, against upstream's own defaults " + "─" * 33)
    print()
    if defaults["status"] != "ok":
        print(f"  UNMEASURABLE: {defaults['why']}")
        print()
        return []
    if arm == "flagged":
        # Trap 1, made unmissable in the output rather than only in the
        # docstring. On a flagged server `/props` reports gglib's own flags
        # back, so every row reads EQUALS and the table looks like a much
        # stronger result than it is. It is not a defaults reading at all.
        print("  !! NOT A DEFAULTS READING — this server was launched with sampler")
        print("     flags, so /props reports those flags back. Shown only so the")
        print("     body-vs-launch probe below has its launch state on the record.")
        print("     The authoritative table comes from `--arm defaults`.")
        print()
    up = defaults["params"]
    rows = []
    print(f"  {'parameter':22} {'gglib floor':>12} {'upstream':>12}   verdict")
    print(f"  {'-' * 22} {'-' * 12} {'-' * 12}   {'-' * 24}")
    for field, wire in FORCE_WRITTEN.items():
        g, u = floor.get(field), up.get(wire)
        if g is None or u is None:
            verdict = "unmeasurable"
        elif abs(float(g) - float(u)) < 1e-6:
            verdict = "EQUALS → compensation"
        else:
            verdict = "DIVERGES → policy"
        rows.append({"param": field, "gglib": g, "upstream": u, "verdict": verdict})
        print(f"  {field:22} {fmt(g):>12} {fmt(u):>12}   {verdict}")
    print()
    print("  already deferred (gglib sends no value):")
    for field, wire in ALREADY_DEFERRED.items():
        print(f"    {field:24} upstream default = {fmt(up.get(wire))}")
    print()
    n_eq = sum(1 for r in rows if r["verdict"].startswith("EQUALS"))
    print(f"  → {n_eq} of {len(rows)} force-written values restate an upstream default.")
    print()
    return rows


def report(results: dict, floor: dict, note: str, arm: str) -> None:
    print()
    print("═" * 78)
    print("  Sampler wire semantics — what llama.cpp supplies vs what gglib asserts")
    d = results.get("defaults", {})
    if d.get("status") == "ok":
        print(f"  build: {d.get('build_info')}")
    if note:
        print(f"  note:  {note}")
    print("═" * 78)
    print()

    report_floor(d, floor, arm)

    if d.get("status") == "ok" and d.get("samplers"):
        print("── Sampler chain order " + "─" * 55)
        print()
        print("  " + " → ".join(d["samplers"]))
        print()
        print("  gglib sends four truncation samplers at once and never sets")
        print("  `--samplers`, so this composition order is load-bearing and was")
        print("  previously unstated anywhere in the tree.")
        print()

    for key, title in (
        ("echo_shape", "Does /slots echo the request?"),
        ("echo_or_applied", "Echo, or the applied chain?"),
        ("body_vs_launch", "Body vs launch flag"),
        ("unmodelled", "Samplers gglib does not model"),
        ("tolerance", "What upstream accepts"),
        ("determinism", "Noise floor"),
    ):
        r = results.get(key)
        if r is None:
            continue
        print(f"── {title} " + "─" * max(4, 74 - len(title)))
        print()
        if r["status"] != "ok":
            print(f"  UNMEASURABLE: {r['why']}")
            print()
            continue

        if key == "echo_shape":
            for k, c in r["checks"].items():
                mark = "ok" if c["match"] else "** MISMATCH **"
                print(f"  {k:20} sent={fmt(c['sent']):<10} observed={fmt(c['observed']):<12} {mark}")
            print()
            print(f"  {len(r['param_keys'])} params exposed on a processing slot.")
        elif key == "echo_or_applied":
            print(f"  mirostat set to      : {r['mirostat']}")
            print(f"  top_k still reported : {r['top_k_still_reported']}")
            print(f"  samplers             : {' → '.join(r['samplers'] or [])}")
            print()
            print(f"  VERDICT: {r['verdict']}")
        elif key == "body_vs_launch":
            print(f"  launch --temp {r['launch_temperature']}, body temperature {r['body_temperature']}")
            print(f"  observed {fmt(r['observed_temperature'])} → {r['winner'].upper()} WINS")
        elif key == "unmodelled":
            for k, c in r["checks"].items():
                print(f"  {k:20} sent={fmt(c['sent']):<10} observed={fmt(c['observed']):<12} "
                      f"{'accepted' if c['accepted'] else '** not accepted **'}")
        elif key == "tolerance":
            for name, c in r["cases"].items():
                line = f"  {name:26} HTTP {c['status']}"
                if c["error"]:
                    line += f"  — {c['error']}"
                print(line)
        elif key == "determinism":
            print(f"  unseeded control : {r['default_seed_distinct']} distinct of {len(r['default_seed'])}")
            print(f"  fixed seed 12345 : {r['fixed_seed_distinct']} distinct of {len(r['fixed_seed'])}")
            print()
            print(f"  VERDICT: {r['verdict']}")
        print()


# ── Entry point ──────────────────────────────────────────────────────────────


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--host", default="127.0.0.1")
    p.add_argument("--port", type=int, default=8081)
    p.add_argument("--timeout", type=int, default=300)
    p.add_argument("--samples", type=int, default=5, help="draws per determinism arm")
    p.add_argument(
        "--arm",
        choices=("defaults", "flagged"),
        default="defaults",
        help="`defaults` needs a server launched with NO sampler flags (see trap 1); "
        "`flagged` needs one launched with gglib's floor as flags",
    )
    p.add_argument(
        "--launch-temp",
        type=float,
        default=0.7,
        help="temperature the flagged server was launched with",
    )
    p.add_argument(
        "--compare-model-defaults",
        metavar="FILE",
        help="path to a previous run's --json output from a DIFFERENT model; "
        "diffs the default tables to prove they are build-level, not model-level",
    )
    p.add_argument(
        "--repo-root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
        help="gglib checkout, for reading the floor out of inference.rs",
    )
    p.add_argument("--note", default="", help="free text recorded in the header")
    p.add_argument("--json", action="store_true", help="emit raw results as JSON too")
    args = p.parse_args()

    base = f"http://{args.host}:{args.port}"

    if get(base, "/health", 10) is None:
        print(f"error: no llama-server at {base}", file=sys.stderr)
        print(
            "start one with: llama-server -m <model.gguf> --jinja "
            f"--port {args.port} -c 8192 -ngl 99",
            file=sys.stderr,
        )
        print("  (the `defaults` arm needs NO sampler flags — see trap 1)", file=sys.stderr)
        return 2

    floor = read_gglib_floor(args.repo_root)
    results: dict = {}

    print(f"probing defaults ...", file=sys.stderr)
    results["defaults"] = probe_defaults(base, args.timeout)

    # Trap 1, enforced rather than documented: if every force-written value
    # already matches gglib's floor including the one known to diverge, this is
    # almost certainly a server launched with gglib's flags and the `defaults`
    # arm would report gglib's own numbers as upstream's.
    if args.arm == "defaults" and results["defaults"]["status"] == "ok":
        up = results["defaults"]["params"]
        if all(
            floor.get(f) is not None
            and up.get(w) is not None
            and abs(float(floor[f]) - float(up[w])) < 1e-6
            for f, w in FORCE_WRITTEN.items()
        ):
            print(
                "error: every value already equals gglib's floor — this server was "
                "probably launched WITH gglib's sampler flags.\n"
                "       Relaunch with none of them, or pass --arm flagged.",
                file=sys.stderr,
            )
            return 2

    if args.arm == "flagged":
        print("probing body-vs-launch precedence ...", file=sys.stderr)
        results["body_vs_launch"] = probe_body_vs_launch(
            base, args.timeout, args.launch_temp
        )
    else:
        for label, fn in (
            ("echo_shape", lambda: probe_echo_shape(base, args.timeout)),
            ("echo_or_applied", lambda: probe_echo_or_applied(base, args.timeout)),
            ("unmodelled", lambda: probe_unmodelled(base, args.timeout)),
            ("tolerance", lambda: probe_tolerance(base, args.timeout)),
            ("determinism", lambda: probe_determinism(base, args.timeout, args.samples)),
        ):
            print(f"probing {label} ...", file=sys.stderr)
            results[label] = fn()

    report(results, floor, args.note, args.arm)

    if args.compare_model_defaults:
        other = json.loads(Path(args.compare_model_defaults).read_text())
        a = results["defaults"].get("params", {})
        b = other.get("defaults", {}).get("params", {})
        keys = sorted(set(FORCE_WRITTEN.values()) | set(ALREADY_DEFERRED.values()))
        diffs = [k for k in keys if a.get(k) != b.get(k)]
        print("── Model independence " + "─" * 56)
        print()
        for k in keys:
            flag = "  <-- DIFFERS" if k in diffs else ""
            print(f"  {k:22} {fmt(a.get(k)):>12} {fmt(b.get(k)):>12}{flag}")
        print()
        print(
            f"  → {len(diffs)} differences: these are "
            f"{'SERVER defaults, model-independent' if not diffs else 'MODEL-dependent'}."
        )
        print()

    if args.json:
        print(json.dumps({"floor": floor, **results}, indent=2, default=str))

    return 0


if __name__ == "__main__":
    sys.exit(main())
