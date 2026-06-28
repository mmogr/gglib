# Council

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-pages-Council-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-pages-Council-complexity.json)

<!-- module-docs:start -->

Primary page for multi-agent orchestration workflows. Hosts the full Council feature: goal submission, real-time task graph execution, human-in-the-loop plan approval, debate streaming, and post-run synthesis. Delegates rendering to `components/Council/Thread/` and graph visualisation to `components/Council/`.

## Architecture

```
CouncilPage
    ├── CouncilThread            ← live run lifecycle (SSE stream)
    │     ├── Goal form
    │     ├── CollapsibleCastingSheet   (team roles)
    │     ├── CollapsibleDagView        (task graph)
    │     ├── NodePanel[]               (per-node progress)
    │     ├── HitlApprovalModal         (approve/reject/steer)
    │     └── Synthesis panel
    └── HistoricalCouncilThread  ← read-only replay from DB
```

Orchestration state is driven by an SSE stream from `POST /api/council/run`. Phase transitions (casting → planning → execution → synthesis) are handled by `useCouncilRunStream` via reducer dispatch.

<!-- module-docs:end -->
