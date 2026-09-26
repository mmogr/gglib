# C3 reviews in the cloud session (2026-09-26)

| round | sha | lens | verdict |
|---|---|---|---|
| r1 | 6e8e7966 (base c0c951c8) | build | FAIL: wording only (r1-build-FAIL-6e8e7966.md); both fail-open fixes held |
| replay | 462d0c3c (base d18bc523) | - | no conflict; gates green |
| fix r1 | f0e1904c | author | fix-r1-author-report-f0e1904c.md |
| r2 | f0e1904c | build + replay | PASS; three optional notes (usage error vs "every invocation"; "named here"; launch.rs:114 outside the range, filed as #1164) |
| repair | 42dea081 | orchestrator | the optional notes applied: "runs before every check"; commit 3 body wording (commit3-body-*.txt) |
| focused | 42dea081 | focused | PASS, no defects |

Gate summaries (Linux clippy with CI's command, fmt, make enforce, the check and its self-test): gate-*.log.summary.
gate-6e8e7966 shows `RC enforce=2`: LANG was unset, and check_workflow_yaml.sh's ruby failed on UTF-8; the same `make enforce` under LANG=C.UTF-8 gave rc 0. Later gates set LANG=C.UTF-8.
