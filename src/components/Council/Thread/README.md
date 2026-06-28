# Thread

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-Council-Thread-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-Council-Thread-complexity.json)

<!-- module-docs:start -->

Self-contained orchestrator run lifecycle component. Owns goal submission, SSE stream initiation, reducer-driven phase transitions, and read-only historical replay from the database.

## Key Files

| File | Role |
|------|------|
| `CouncilThread.tsx` | Live run view: goal form, SSE streaming, DAG/casting/node panels |
| `HistoricalCouncilThread.tsx` | Read-only replay of a completed run from the database |
| `useCouncilRunStream.ts` | SSE stream consumer; phase-state reducer; cost estimates |

## State Machine

```
idle → streaming → casting → plan_review → executing → done
                                  ↑
                         human-in-the-loop approval
```

<!-- module-docs:end -->
