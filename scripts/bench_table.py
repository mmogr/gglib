#!/usr/bin/env python3
"""bench_table.py - Merge agentic A/B eval reports into a Markdown table.

Usage: ./scripts/bench_table.py REPORT.json [REPORT.json ...]
       ./scripts/bench_table.py bench/*.json > bench/TABLE.md

Reads the JSON produced by `gglib benchmark agentic --output FILE`
(the AgenticEvalReport interchange format) and emits a Markdown summary
suitable for pasting into the README or a writeup.

Reports that fail to parse are reported on stderr and skipped; the table
is still emitted for whatever did parse. Exit code is 1 if nothing parsed.
"""

import json
import sys

DASH = "—"


def fmt_score(value):
    """Format a 0.0-1.0 axis score, or a dash when the axis went unmeasured."""
    return DASH if value is None else f"{value:.3f}"


def fmt_duration(ms):
    """Render milliseconds at a human scale: ms under a second, then s, then min."""
    if ms is None:
        return DASH
    if ms < 1000:
        return f"{ms} ms"
    seconds = ms / 1000
    if seconds < 90:
        return f"{seconds:.1f} s"
    return f"{seconds / 60:.1f} min"


def fmt_count(value):
    return DASH if value is None else f"{value:,}"


def fmt_ratio(value):
    """Render a raw/gglib ratio. Sub-1.0 means gglib was slower — show it honestly."""
    if value is None:
        return DASH
    if value < 10:
        return f"{value:.1f}x"
    return f"{round(value):,}x"


def model_label(report):
    """Name plus quantization, e.g. `qwen3.6 (Q4_K_M)`."""
    name = report.get("model_name", "?")
    quant = report.get("quantization")
    return f"{name} ({quant})" if quant else name


def load(path):
    with open(path, encoding="utf-8") as handle:
        report = json.load(handle)
    # Fail loudly here rather than emitting a table full of dashes: a file
    # missing these keys is not an unmeasured axis, it is the wrong file.
    for key in ("raw", "gglib", "delta"):
        if key not in report:
            raise KeyError(f"missing top-level key {key!r}")
    return report


def quality_table(reports):
    lines = [
        "| Model | Tool accuracy (raw / gglib) | Task completion (raw / gglib) | Loop avoidance (raw / gglib) |",
        "|---|---|---|---|",
    ]
    for report in reports:
        raw, gglib = report["raw"], report["gglib"]
        lines.append(
            f"| {model_label(report)} "
            f"| {fmt_score(raw['tool_accuracy'])} / {fmt_score(gglib['tool_accuracy'])} "
            f"| {fmt_score(raw['task_completion'])} / {fmt_score(gglib['task_completion'])} "
            f"| {fmt_score(raw.get('loop_avoidance'))} / {fmt_score(gglib.get('loop_avoidance'))} |"
        )
    return lines


def cost_table(reports):
    lines = [
        "| Model | Suite wall time (raw / gglib) | Speedup | Tokens generated (raw / gglib) | Token ratio | Time to first tool call (raw / gglib) |",
        "|---|---|---|---|---|---|",
    ]
    for report in reports:
        raw, gglib, delta = report["raw"], report["gglib"], report["delta"]
        raw_ttfc = raw.get("mean_time_to_first_tool_call_ms")
        gglib_ttfc = gglib.get("mean_time_to_first_tool_call_ms")
        lines.append(
            f"| {model_label(report)} "
            f"| {fmt_duration(raw.get('total_wall_ms'))} / {fmt_duration(gglib.get('total_wall_ms'))} "
            f"| **{fmt_ratio(delta.get('wall_time_speedup'))}** "
            f"| {fmt_count(raw.get('total_completion_tokens'))} / {fmt_count(gglib.get('total_completion_tokens'))} "
            f"| **{fmt_ratio(delta.get('completion_token_ratio'))}** "
            f"| {fmt_duration(None if raw_ttfc is None else round(raw_ttfc))}"
            f" / {fmt_duration(None if gglib_ttfc is None else round(gglib_ttfc))} |"
        )
    return lines


def main(argv):
    if len(argv) < 2 or argv[1] in ("-h", "--help"):
        print(__doc__.strip())
        return 0

    reports, failed = [], 0
    for path in argv[1:]:
        try:
            reports.append(load(path))
        except (OSError, ValueError, KeyError) as err:
            print(f"warning: skipping {path}: {err}", file=sys.stderr)
            failed += 1

    if not reports:
        print("error: no readable reports", file=sys.stderr)
        return 1

    reports.sort(key=lambda r: (r.get("param_count_b", 0), r.get("model_name", "")))
    ctx_sizes = sorted({r.get("ctx_size") for r in reports if r.get("ctx_size")})
    ctx_note = (
        f"{ctx_sizes[0]:,} tokens"
        if len(ctx_sizes) == 1
        else ", ".join(f"{c:,}" for c in ctx_sizes) + " tokens"
    )

    out = []
    out.append("### Quality: what the pipeline does to correctness")
    out.append("")
    out.extend(quality_table(reports))
    out.append("")
    out.append("### Cost: what it takes to get there")
    out.append("")
    out.extend(cost_table(reports))
    out.append("")
    out.append(
        f"Both arms run the same 9-task BFCL-style suite against the same loaded "
        f"model at {ctx_note} — once with the gglib request pipeline bypassed "
        f"(bare llama-server defaults) and once through it. One admission lease "
        f"covers both arms, so no figure includes a model swap. Reproduce with "
        f"`gglib benchmark agentic -m <model> --output report.json`."
    )

    print("\n".join(out))
    if failed:
        print(f"\n<!-- {failed} report(s) skipped; see stderr -->")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
