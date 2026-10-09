#!/usr/bin/env python3
"""Read the verdict on a llama.cpp release from the evidence collected for it.

llama_upstream.py collects what moving the pin would meet; whether gglib must
change, and what the change is, is a reading of that evidence against gglib's
code and ADRs. This asks Claude for that reading, through the Claude Agent SDK
with read-only tools: Read, Grep and Glob over the checkout, and git, for the
llama.cpp clone. Nothing it can call writes to the checkout or reaches the
network.

It runs on the Agent SDK rather than the Claude Code GitHub Action because the
API credits included with a Claude Max or Team plan cover the Agent SDK run
with an API key and do not cover the Action, which counts as Claude Code.

Usage:
  scripts/llama_upstream_verdict.py --report <dir> --llama <clone> --out <file>
                                    [--model <id>] [--max-budget-usd <n>]

Needs ANTHROPIC_API_KEY, and `pip install claude-agent-sdk`, which carries
its own Claude Code binary.

Writes --out as JSON, always: {"verdict", "summary", "body"} when a reading
came back, {"error"} when it did not, with the reason the session gave, such
as a credit balance too low. Exit 0 with a verdict, 1 without one.
"""

import argparse
import asyncio
import json
import sys
from pathlib import Path

from claude_agent_sdk import AssistantMessage, ClaudeAgentOptions, ResultMessage, TextBlock, query

ROOT = Path(__file__).resolve().parent.parent

SCHEMA = {
    "type": "object",
    "properties": {
        "verdict": {"type": "string", "enum": ["compatible", "changes needed", "unclear"]},
        "summary": {"type": "string"},
        "body": {"type": "string"},
    },
    "required": ["verdict", "summary", "body"],
}

PROMPT = """\
You are reading what a llama.cpp release changes for gglib, the repository checked out here. gglib launches and proxies `llama-server`, and pins the release it installs: `PINNED_LLAMA_RELEASE` in crates/gglib-runtime/src/llama/download/mod.rs. The evidence for a candidate release is in {report}: facts.json, report.md, and evidence/ (the commit list, both `--help` texts and their diff, the launch logs and /props, and the diff of each upstream file an ADR cites). A bare clone of llama.cpp is at {llama}; `git -C {llama} show <tag>:<path>` reads a file at either tag.

Decide whether gglib can move its pin to the candidate as it is, and if not, what must change. Read against gglib's own code and documents, not from memory of llama.cpp:
- crates/gglib-runtime/src/command.rs builds the launch command line. A flag it emits that the candidate's `--help` no longer lists is a change needed; say the replacement, from the candidate's `--help` or common/arg.cpp.
- crates/gglib-proxy reads llama-server's HTTP responses. tools/server/README.md's diff says what the API changed.
- docs/adr/ holds the decisions. Each ADR has criteria that reopen it; report.md's "Files the ADRs cite" names which upstream files each one reads from. For each such file that changed, read the diff for the specific behaviour the ADR relies on (ADR 0005: the chat endpoint overwriting grammar_lazy, grammar_triggers and preserved_tokens; ADR 0003/0004: the sampler defaults; ADR 0007: chat_template_caps and reasoning_effort; ADR 0015: /props modalities and cache behaviour with a projector) and say whether that behaviour changed or only the code around it.
- docs/tool-call-repair.md and crates/gglib-proxy/src/repair.rs compensate for tool-call parsing; an upstream parser fix for a dialect gglib repairs is worth naming.

Write `body` as GitHub-flavoured Markdown with these sections, each short and concrete, every claim with the file and line it comes from, in gglib or in the upstream diff: "Verdict" (one paragraph), "Changes needed" (a list, each with the gglib file and line and the change, or "None"), "ADR criteria" (one line per ADR whose cited file changed: touched or not touched, and if touched which reading the ADR says to take), "Worth knowing" (upstream changes gglib does not need but might want, at most five). Say what you could not determine rather than guessing. `summary` is one sentence. `verdict` is "compatible" when nothing in "Changes needed" and no ADR criterion is touched, "changes needed" when the list is not empty, and "unclear" otherwise.
"""


def one_line(text: str, limit: int = 400) -> str:
    return " ".join(str(text).split())[:limit]


async def read_verdict(args: argparse.Namespace) -> dict:
    options = ClaudeAgentOptions(
        model=args.model,
        cwd=str(ROOT),
        system_prompt={"type": "preset", "preset": "claude_code"},
        # The repository's own .claude settings are for people working in it.
        setting_sources=[],
        allowed_tools=["Read", "Grep", "Glob", "Bash(git *)"],
        # dontAsk turns anything that would prompt into a denial, and these
        # are out of reach entirely: nothing writes, fetches or delegates.
        disallowed_tools=["Write", "Edit", "NotebookEdit", "WebFetch", "WebSearch", "Agent"],
        permission_mode="dontAsk",
        max_turns=args.max_turns,
        max_budget_usd=args.max_budget_usd,
        output_format={"type": "json_schema", "schema": SCHEMA},
    )
    prompt = PROMPT.format(report=args.report, llama=args.llama)

    last_said = None
    outcome = None
    try:
        async for message in query(prompt=prompt, options=options):
            if isinstance(message, AssistantMessage):
                text = " ".join(b.text for b in message.content if isinstance(b, TextBlock))
                last_said = text or last_said
            elif isinstance(message, ResultMessage):
                cost = message.total_cost_usd
                print(
                    f"result: {message.subtype}, {message.num_turns} turns,"
                    f" ${cost if cost is not None else '?'}",
                    file=sys.stderr,
                )
                out = message.structured_output
                if message.subtype == "success" and not message.is_error and isinstance(out, dict):
                    outcome = {k: out.get(k) for k in ("verdict", "summary", "body")}
                elif message.subtype == "success" and not message.is_error:
                    # It finished, but without the answer the schema asks for.
                    outcome = {"error": one_line(f"the reading ended without a verdict: {message.result or last_said or ''}")}
                else:
                    reason = message.result or "; ".join(map(str, message.errors or [])) or last_said
                    outcome = {"error": one_line(reason or f"the session ended with {message.subtype}")}
    except Exception as exc:  # the SDK raises after an error result, and on a CLI failure
        if outcome is None:
            outcome = {"error": one_line(last_said or f"{type(exc).__name__}: {exc}")}
    return outcome or {"error": one_line(last_said or "the session returned no result")}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--report", required=True, help="llama_upstream.py's --out directory")
    ap.add_argument("--llama", required=True, help="the llama.cpp clone the report was made from")
    ap.add_argument("--out", required=True, help="where to write the verdict JSON")
    ap.add_argument("--model", default="claude-opus-5-5")
    ap.add_argument("--max-turns", type=int, default=40)
    ap.add_argument(
        "--max-budget-usd",
        type=float,
        default=5.0,
        help="stop the reading when its estimated cost reaches this; the estimate can pass it",
    )
    args = ap.parse_args()

    outcome = asyncio.run(read_verdict(args))
    Path(args.out).write_text(json.dumps(outcome, indent=2) + "\n")
    if "error" in outcome:
        print(f"no verdict: {outcome['error']}", file=sys.stderr)
        return 1
    print(f"verdict: {outcome['verdict']}: {outcome['summary']}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
