#!/usr/bin/env python3
"""Probe: request-level lazy grammar under tool_choice-auto-shaped traffic.

The question PR 7 hangs on. ADR 0002 documents that a *full* grammar cannot
be installed under `auto` — it constrains from the first token and forbids
the prose answers `auto` exists to permit. llama.cpp's lazy grammar
(`grammar_lazy` + `grammar_triggers`) claims to square that circle: sampling
runs free until a trigger appears in the output, and the grammar constrains
only from there. gglib's prospective repair-learning mechanism would inject
exactly that, per model, once the repair-rate signal says the model's `auto`
path is unconstrained.

Method, per the house rule: raw llama-server, gglib out of the path. No
`tools` field — the server then installs nothing natively, which mirrors the
deployment target (models whose template gets no native lazy grammar under
`auto`). The tool protocol is described in the system prompt; the request
carries the candidate grammar.

The grammar forces a CANARY (`"name": "canary_fn"`, integer arg `x`) that
the prompt never mentions, so engagement is visible in the bytes: a
conformant canary call proves the grammar ran; the model's own invented
shape proves it did not. A null result cannot hide as a success.

Three arms:
  1. engagement — a prompt demanding a tool call; expect the canary.
  2. prose — a plain question; expect ordinary prose, no trigger, natural stop.
  3. reasoning — thinking elicited before the call; expect an unconstrained
     think block AND a canary call after it. 3b mentions the literal trigger
     inside the reasoning to hunt a false trigger.
"""

import json
import sys
import urllib.request

BASE = "http://127.0.0.1:15600"

# Root begins with the trigger text: llama.cpp's lazy grammars match from
# the trigger onward, so the trigger must be derivable by the grammar.
GRAMMAR = r"""
root ::= "<tool_call>" nl "{\"name\": \"canary_fn\", \"arguments\": {\"x\": " int "}}" nl "</tool_call>"
int ::= [0-9]+
nl ::= "\n"
"""

# The build's actual contract, read from its own source
# (tools/server/server-common.h:73 and common/common.h:142): `type` is an
# integer enum — 0 TOKEN, 1 WORD, 2 PATTERN, 3 PATTERN_FULL — and a WORD
# trigger that tokenizes to a single (special) token must also appear in
# `preserved_tokens`, or the handler throws. The first probe sent
# `"type": "word"` as a string; the handler's `.get<int>()` threw, the
# failure was swallowed, and the grammar ran NON-lazily — which is what
# produced the canary on the prose arm and demonstrated ADR 0002's wall
# by accident.
TRIGGER_VARIANTS = [
    (
        "word+preserved",
        {
            "preserved_tokens": ["<tool_call>"],
            "grammar_triggers": [{"type": 1, "value": "<tool_call>"}],
        },
    ),
    (
        "pattern",
        {"grammar_triggers": [{"type": 2, "value": "<tool_call>"}]},
    ),
]

SYSTEM = (
    "You can invoke tools. To invoke a tool, emit exactly this format:\n"
    "<tool_call>\n{\"name\": \"<function>\", \"arguments\": {...}}\n</tool_call>\n"
    "Available function: get_weather(city: string) — current weather.\n"
    "When the user asks something needing a tool, invoke it. Otherwise answer normally."
)


def chat(messages, trigger_extra, max_tokens=600):
    body = {
        "messages": messages,
        "max_tokens": max_tokens,
        "temperature": 1.0,
        "top_k": 20,
        "grammar": GRAMMAR,
        "grammar_lazy": True,
        **trigger_extra,
    }
    req = urllib.request.Request(
        f"{BASE}/v1/chat/completions",
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(req, timeout=240) as r:
            out = json.load(r)
    except urllib.error.HTTPError as e:
        return {"http_error": e.code, "body": e.read().decode()[:300]}
    msg = out["choices"][0]["message"]
    return {
        "content": msg.get("content") or "",
        "reasoning": msg.get("reasoning_content") or "",
        "finish": out["choices"][0].get("finish_reason"),
        "tool_calls": msg.get("tool_calls"),
    }


def canary_engaged(content):
    return '"name": "canary_fn"' in content and "<tool_call>" in content


def main():
    print("== probe: lazy grammar under auto-shaped traffic ==\n")

    # ── Field discovery: which trigger spelling does this build accept? ──
    # Verified by BEHAVIOUR (the canary), never by status code — unknown
    # keys return 200 and do nothing (ADR 0003 finding 6).
    demand = [
        {"role": "system", "content": SYSTEM},
        {"role": "user", "content": "What's the weather in Oslo right now? Use the tool."},
    ]
    chosen = None
    for name, extra in TRIGGER_VARIANTS:
        r = chat(demand, extra)
        if "http_error" in r:
            print(f"  [{name}] HTTP {r['http_error']}: {r['body'][:120]}")
            continue
        hit = canary_engaged(r["content"])
        print(f"  [{name}] engaged={hit} finish={r['finish']}")
        print(f"      content: {r['content'][:160]!r}")
        if hit and chosen is None:
            chosen = (name, extra)
    if not chosen:
        print("\nVERDICT: no trigger spelling engaged — mechanism NOT viable as-is")
        sys.exit(1)
    name, extra = chosen
    print(f"\n  -> trigger spelling that works: {name}\n")

    # ── Arm 1 already answered engagement; repeat for stability (3 draws) ──
    hits = 0
    for i in range(3):
        r = chat(demand, extra)
        hits += canary_engaged(r["content"])
    print(f"[arm 1] engagement: {hits}/3 demanded-call draws produced the canary\n")

    # ── Arm 2: prose untouched ──
    prose_msgs = [
        {"role": "system", "content": SYSTEM},
        {"role": "user", "content": "In one short sentence, what is a lighthouse for?"},
    ]
    r = chat(prose_msgs, extra, max_tokens=400)
    prose_clean = (
        "<tool_call>" not in r["content"]
        and "canary_fn" not in r["content"]
        and r["finish"] == "stop"
        and len(r["content"].strip()) > 0
    )
    print(f"[arm 2] prose: clean={prose_clean} finish={r['finish']}")
    print(f"    content: {r['content'][:160]!r}\n")

    # ── Arm 3: reasoning coexistence ──
    think_msgs = [
        {"role": "system", "content": SYSTEM},
        {"role": "user", "content": (
            "Think carefully step by step about which city I mean — the one "
            "famous for fjords in Norway — then check its weather with the tool."
        )},
    ]
    r = chat(think_msgs, extra, max_tokens=900)
    reasoned = len(r["reasoning"].strip()) > 20
    engaged = canary_engaged(r["content"])
    print(f"[arm 3] reasoning: think_block={reasoned} canary_after={engaged} finish={r['finish']}")
    print(f"    reasoning[:120]: {r['reasoning'][:120]!r}")
    print(f"    content[:160]:   {r['content'][:160]!r}\n")

    # ── Arm 3b: the literal trigger inside the think block ──
    trap_msgs = [
        {"role": "system", "content": SYSTEM},
        {"role": "user", "content": (
            "First, in your reasoning, write the literal string <tool_call> "
            "as an example of the format, and explain it. Then answer in prose "
            "only: what is the capital of Norway? Do NOT invoke any tool."
        )},
    ]
    r = chat(trap_msgs, extra, max_tokens=900)
    trigger_in_think = "<tool_call>" in r["reasoning"]
    hijacked = canary_engaged(r["content"]) or "<tool_call>" in r["content"]
    print(f"[arm 3b] trap: trigger_in_think={trigger_in_think} content_hijacked={hijacked} finish={r['finish']}")
    print(f"    reasoning[:160]: {r['reasoning'][:160]!r}")
    print(f"    content[:160]:   {r['content'][:160]!r}\n")

    print("== summary ==")
    print(f"  trigger spelling : {name}")
    print(f"  engagement       : {hits}/3")
    print(f"  prose untouched  : {prose_clean}")
    print(f"  think + canary   : think={reasoned} canary={engaged}")
    print(f"  false trigger    : hijacked={hijacked} (trigger_in_think={trigger_in_think})")


if __name__ == "__main__":
    main()
