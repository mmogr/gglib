---
name: subagents-run-on-opus
description: "Matt wants every subagent spawned with model \"opus\" (Opus 5.5); asked 2026-09-25 mid-task, one Fable agent was stopped and relaunched"
metadata:
  node_type: memory
  type: feedback
  originSessionId: d2f123ee-56ab-4c4c-a881-dbf2eef763a6
  modified: 2026-09-25T08:27:27.799Z
---

When spawning subagents with the Agent tool, pass `model: "opus"` (Opus 5.5). Matt asked for this on 2026-09-25 while an audit-verification fan-out was starting: "if you use subagents can you please use Opus5.5".

**Why:** not stated; the request came immediately after the first agent launched on the session default, so the likely reason is cost or quota rather than capability. Do not assume it applies to forks (`subagent_type: "fork"` ignores the model override anyway).

**How to apply:** set `model: "opus"` on every Agent call unless Matt names a different model for that task. If an agent is already running on another model when the preference is stated, stop it and relaunch rather than let it finish. Related: [[gglib-adversarial-review-loop]].
