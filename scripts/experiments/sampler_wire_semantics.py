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

4. **Defaults ARE model-dependent, and diffing two models does not prove
   otherwise.** This trap was identified and the mitigation was too weak. Run
   `--compare-model-defaults` against a second model and the tables match — but
   they match whenever *both* models are silent, which is what "no
   `general.sampling.*` in either GGUF" looks like. Two silent models cannot
   distinguish "the build decides" from "the model decides and neither of these
   two has an opinion".

   Since llama.cpp PR #17120 (merged 2025-11-25, in the pin),
   `common_init_sampler_from_model` overwrites `params.sampling` from the
   model's own `general.sampling.*` keys for every field no CLI flag set, and
   `/props` is rendered from that same struct. So `/props` answers "what will
   this server with this model default to", never "what does this build default
   to", and the two coincide only while the model is silent.

   `--arm model-embedded` is the positive control the diff could not be: stamp
   a key into a copy of a GGUF with `--stamp`, launch bare on it, and watch the
   table move.

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
import shutil
import struct
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

# ── Model-embedded sampling (llama.cpp PR #17120) ────────────────────────────
#
# The GGUF keys a model author can use to move llama.cpp's own defaults, keyed
# by the short name this script takes on the command line. Valued by the GGUF
# key, the wire name `/props` reports it under, and the GGUF value type — which
# has to match what llama.cpp reads it back as (`get_int32` vs `get_float` in
# `common_init_sampler_from_model`).
#
# Five of the twelve `general.sampling.*` keys map onto a parameter gglib has a
# floor opinion about. `presence_penalty` and `dry_multiplier` have NO GGUF key
# at all, which is why they stay build-attributable no matter what a model
# ships — that asymmetry is the finding, not an implementation detail.

GGUF_F32, GGUF_I32 = 6, 5

MODEL_EMBEDDED = {
    "temp": ("general.sampling.temp", "temperature", GGUF_F32),
    "top_p": ("general.sampling.top_p", "top_p", GGUF_F32),
    "top_k": ("general.sampling.top_k", "top_k", GGUF_I32),
    "min_p": ("general.sampling.min_p", "min_p", GGUF_F32),
    "penalty_repeat": ("general.sampling.penalty_repeat", "repeat_penalty", GGUF_F32),
}

# `general.sampling.*` keys llama.cpp reads that no gglib floor field tracks.
# Listed so the report can say what a model asked for that gglib is not
# watching, rather than silently ignoring it.
MODEL_EMBEDDED_UNTRACKED = (
    "general.sampling.sequence",
    "general.sampling.xtc_probability",
    "general.sampling.xtc_threshold",
    "general.sampling.penalty_last_n",
    "general.sampling.mirostat",
    "general.sampling.mirostat_tau",
    "general.sampling.mirostat_eta",
)

# What this build defaults to with a silent model, so `--stamp` can refuse a
# value that could not possibly move anything. Same discipline as trap 1: a
# control that cannot move proves nothing when it does not move.
BUILD_DEFAULTS = {
    "temp": 0.8,
    "top_p": 0.95,
    "top_k": 40,
    "min_p": 0.05,
    "penalty_repeat": 1.0,
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


# ── GGUF metadata surgery ────────────────────────────────────────────────────
#
# Appending a key-value pair to a GGUF, in stdlib, so the positive control for
# trap 4 is committed alongside the claim it supports rather than depending on
# llama.cpp's `gguf-py` being checked out.
#
# The layout, all little-endian:
#
#   magic "GGUF" | version u32 | tensor_count u64 | kv_count u64
#   kv block     : [ key:string, type:u32, value ] * kv_count
#   tensor infos : [ name:string, n_dims:u32, dims:u64*, type:u32, offset:u64 ]
#   padding      : to `general.alignment` (default 32)
#   tensor data
#
# where string = len:u64 followed by that many bytes, unterminated.
#
# The one fact that makes this tractable: **a tensor's `offset` is relative to
# the start of the data section**, not to the start of the file (`gguf-py`'s
# reader computes `data_offs = start_offs + offset_tensor[0]`). So inserting
# metadata shifts where the data section begins and rewrites none of the
# offsets inside it. Only a value *skipper* is needed to find the block
# boundaries — the values themselves are copied through as bytes.

GGUF_MAGIC = b"GGUF"

# Fixed-width value type ids → size in bytes. STRING (8) and ARRAY (9) are
# variable and handled separately.
_GGUF_FIXED = {0: 1, 1: 1, 7: 1, 2: 2, 3: 2, 4: 4, 5: 4, 6: 4, 10: 8, 11: 8, 12: 8}
_GGUF_STRING, _GGUF_ARRAY = 8, 9


class GgufError(Exception):
    """The file is not a GGUF this script knows how to rewrite."""


def _u32(b: bytes, o: int) -> int:
    return struct.unpack_from("<I", b, o)[0]


def _u64(b: bytes, o: int) -> int:
    return struct.unpack_from("<Q", b, o)[0]


def _skip_value(b: bytes, o: int, vtype: int) -> int:
    """Byte offset just past one metadata value."""
    if vtype in _GGUF_FIXED:
        return o + _GGUF_FIXED[vtype]
    if vtype == _GGUF_STRING:
        return o + 8 + _u64(b, o)
    if vtype == _GGUF_ARRAY:
        elem, count = _u32(b, o), _u64(b, o + 4)
        o += 12
        if elem in _GGUF_FIXED:
            return o + _GGUF_FIXED[elem] * count
        for _ in range(count):
            o = _skip_value(b, o, elem)
        return o
    raise GgufError(f"unknown GGUF value type {vtype}")


def _scan_gguf(head: bytes) -> dict:
    """Locate the block boundaries. `head` must cover through the tensor infos."""
    if head[:4] != GGUF_MAGIC:
        raise GgufError("not a GGUF file (bad magic)")
    version = _u32(head, 4)
    if version not in (2, 3):
        raise GgufError(f"unsupported GGUF version {version}")
    tensor_count, kv_count = _u64(head, 8), _u64(head, 16)

    o = 24
    kv_start = o
    keys, alignment = set(), 32
    for _ in range(kv_count):
        klen = _u64(head, o)
        key = head[o + 8 : o + 8 + klen].decode("utf-8", "replace")
        o += 8 + klen
        vtype = _u32(head, o)
        o += 4
        vstart = o
        o = _skip_value(head, o, vtype)
        keys.add(key)
        if key == "general.alignment" and vtype == 4:
            alignment = _u32(head, vstart)
    kv_end = o

    for _ in range(tensor_count):
        o += 8 + _u64(head, o)  # name
        ndims = _u32(head, o)
        o += 4 + 8 * ndims + 4 + 8  # dims, type, offset
    tensor_info_end = o

    data_start = (tensor_info_end + alignment - 1) // alignment * alignment
    return {
        "kv_start": kv_start,
        "kv_end": kv_end,
        "tensor_info_end": tensor_info_end,
        "data_start": data_start,
        "kv_count": kv_count,
        "keys": keys,
        "alignment": alignment,
    }


def _read_head(path: Path) -> tuple[bytes, dict]:
    """Read enough of the file to scan, growing the window if it was short."""
    size = path.stat().st_size
    want = min(size, 1 << 20)
    while True:
        with path.open("rb") as fh:
            head = fh.read(want)
        try:
            return head, _scan_gguf(head)
        except (struct.error, IndexError):
            if want >= size:
                raise GgufError("header region is truncated or malformed") from None
            want = min(size, want * 8)


def _kv_bytes(key: str, vtype: int, raw: str) -> bytes:
    kb = key.encode()
    out = struct.pack("<Q", len(kb)) + kb + struct.pack("<I", vtype)
    if vtype == GGUF_F32:
        return out + struct.pack("<f", float(raw))
    if vtype == GGUF_I32:
        return out + struct.pack("<i", int(raw))
    raise GgufError(f"cannot encode value type {vtype}")


def stamp_gguf(src: Path, dst: Path, pairs: dict[str, str]) -> None:
    """Write `general.sampling.*` keys into a copy of `src`.

    Appends rather than edits: a key the file already carries is refused, so a
    stamped model can never end up with two values for one key and a reading
    that depends on which one llama.cpp happens to take.
    """
    head, layout = _read_head(src)

    additions = b""
    for short, value in pairs.items():
        gguf_key, _, vtype = MODEL_EMBEDDED[short]
        if gguf_key in layout["keys"]:
            raise GgufError(
                f"{src.name} already carries {gguf_key}; this script only appends, "
                "so stamping it again would leave two values for one key"
            )
        additions += _kv_bytes(gguf_key, vtype, value)

    # The padding is recomputed, not copied: the metadata just grew, so the
    # data section starts at a different offset and needs its own run-up to
    # the alignment boundary. Copying the original padding would leave the
    # tensor data misaligned by exactly `len(additions) % alignment`.
    align = layout["alignment"]
    shifted_end = layout["tensor_info_end"] + len(additions)
    pad = b"\0" * (((shifted_end + align - 1) // align * align) - shifted_end)

    with src.open("rb") as fin, dst.open("wb") as fout:
        fout.write(head[:8])  # magic + version
        fout.write(head[8:16])  # tensor_count
        fout.write(struct.pack("<Q", layout["kv_count"] + len(pairs)))
        fout.write(head[layout["kv_start"] : layout["kv_end"]])
        fout.write(additions)
        fout.write(head[layout["kv_end"] : layout["tensor_info_end"]])
        fout.write(pad)
        fin.seek(layout["data_start"])
        shutil.copyfileobj(fin, fout, length=8 << 20)


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


def probe_model_embedded(base: str, timeout: int, expected: dict[str, str]) -> dict:
    """Did the model's own `general.sampling.*` keys move `/props`?

    The positive control trap 4's model-diff could not be. A stamped value
    that reaches `/props` proves the default table is a property of the model
    as well as of the build — which is what makes `UPSTREAM_DEFAULTS` alone an
    insufficient baseline.
    """
    props = get(base, "/props", timeout)
    if not isinstance(props, dict):
        return {"status": "unmeasurable", "why": "/props did not return an object"}
    params = (props.get("default_generation_settings") or {}).get("params")
    if not isinstance(params, dict):
        return {
            "status": "unmeasurable",
            "why": "no default_generation_settings.params in /props",
        }

    rows = []
    for short, raw in expected.items():
        _, wire, _ = MODEL_EMBEDDED[short]
        want, got = float(raw), params.get(wire)
        if got is None:
            verdict = "unmeasurable — /props does not report this field"
        elif abs(float(got) - want) < 1e-6:
            verdict = "MOVED → the model decides this, not the build"
        elif abs(float(got) - float(BUILD_DEFAULTS[short])) < 1e-6:
            verdict = "unmoved — still the build default; was the model stamped?"
        else:
            verdict = "unexpected — neither the stamp nor the build default"
        rows.append(
            {
                "param": short,
                "wire": wire,
                "stamped": want,
                "build_default": BUILD_DEFAULTS[short],
                "observed": got,
                "verdict": verdict,
            }
        )

    untracked = [k for k in MODEL_EMBEDDED_UNTRACKED]
    return {"status": "ok", "rows": rows, "untracked_keys": untracked}


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


def report_model_embedded(res: dict) -> None:
    print("── Model-embedded sampling defaults " + "─" * 42)
    print()
    if res["status"] != "ok":
        print(f"  UNMEASURABLE: {res['why']}")
        print()
        return
    print(f"  {'parameter':16} {'stamped':>10} {'build':>10} {'/props':>12}   verdict")
    print(f"  {'-' * 16} {'-' * 10} {'-' * 10} {'-' * 12}   {'-' * 34}")
    for r in res["rows"]:
        print(
            f"  {r['param']:16} {fmt(r['stamped']):>10} {fmt(r['build_default']):>10} "
            f"{fmt(r['observed']):>12}   {r['verdict']}"
        )
    print()
    moved = sum(1 for r in res["rows"] if r["verdict"].startswith("MOVED"))
    print(f"  → {moved} of {len(res['rows'])} stamped values reached /props.")
    print()
    if moved:
        print("  So `/props` answers 'what will this server with THIS MODEL default")
        print("  to', not 'what does this build default to'. A baseline check that")
        print("  compares it against a per-build constant reports drift that is the")
        print("  model's recommendation, not a pin bump.")
        print()
    print("  gglib tracks no floor field for these keys, so a model setting them")
    print("  moves sampling with nothing watching:")
    for key in res["untracked_keys"]:
        print(f"    {key}")
    print()
    print("  And these two have NO general.sampling.* key at all, so they stay")
    print("  attributable to the build whatever a model ships:")
    print("    presence_penalty")
    print("    dry_multiplier")
    print()


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

    if arm == "model-embedded":
        report_model_embedded(results.get("model_embedded", {"status": "unmeasurable", "why": "not probed"}))
        return

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
        choices=("defaults", "flagged", "model-embedded"),
        default="defaults",
        help="`defaults` needs a server launched with NO sampler flags (see trap 1); "
        "`flagged` needs one launched with gglib's floor as flags; "
        "`model-embedded` needs one launched bare on a GGUF stamped by --stamp",
    )
    p.add_argument(
        "--stamp",
        nargs=2,
        metavar=("SRC", "DST"),
        help="write the --set keys into a copy of SRC at DST and exit; no server "
        "is contacted. Launch llama-server on DST, then re-run with "
        "--arm model-embedded and the same --set flags",
    )
    p.add_argument(
        "--set",
        action="append",
        default=[],
        metavar="KEY=VALUE",
        help=f"model-embedded sampler to stamp or expect, one of "
        f"{'/'.join(MODEL_EMBEDDED)} — e.g. temp=0.33. Repeatable. The same "
        f"flags describe the stamp and the expectation, so the two cannot drift",
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

    # `--set` is parsed and validated the same way for both modes, so a value
    # that could not be stamped cannot be expected either.
    stamped: dict[str, str] = {}
    for item in args.set:
        key, _, value = item.partition("=")
        if key not in MODEL_EMBEDDED:
            print(
                f"error: --set {key}: not a model-embedded sampler. "
                f"Choose from {', '.join(MODEL_EMBEDDED)}.",
                file=sys.stderr,
            )
            return 2
        try:
            numeric = float(value)
        except ValueError:
            print(f"error: --set {key}={value!r}: not a number", file=sys.stderr)
            return 2
        # Trap 1's discipline, applied to this arm: a control that cannot move
        # proves nothing when it does not move.
        if abs(numeric - float(BUILD_DEFAULTS[key])) < 1e-6:
            print(
                f"error: --set {key}={value} is already this build's default, so "
                f"a /props reading of {value} would prove nothing. Pick a value "
                f"no default could be mistaken for.",
                file=sys.stderr,
            )
            return 2
        stamped[key] = value

    if args.stamp:
        if not stamped:
            print("error: --stamp needs at least one --set KEY=VALUE", file=sys.stderr)
            return 2
        src, dst = Path(args.stamp[0]), Path(args.stamp[1])
        try:
            stamp_gguf(src, dst, stamped)
        except (GgufError, OSError) as exc:
            print(f"error: could not stamp {src}: {exc}", file=sys.stderr)
            return 2
        print(f"wrote {dst} ({dst.stat().st_size} bytes)", file=sys.stderr)
        for short, value in stamped.items():
            print(f"  {MODEL_EMBEDDED[short][0]} = {value}", file=sys.stderr)
        print(
            f"\nnow: llama-server -m {dst} --port {args.port} -c 8192 -ngl 99\n"
            f"then: {sys.argv[0]} --arm model-embedded "
            + " ".join(f"--set {k}={v}" for k, v in stamped.items()),
            file=sys.stderr,
        )
        return 0

    if args.arm == "model-embedded" and not stamped:
        print(
            "error: --arm model-embedded needs the --set flags the model was "
            "stamped with, so it knows what to look for",
            file=sys.stderr,
        )
        return 2

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

    if args.arm == "model-embedded":
        print("probing model-embedded sampling defaults ...", file=sys.stderr)
        results["model_embedded"] = probe_model_embedded(base, args.timeout, stamped)
    elif args.arm == "flagged":
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
