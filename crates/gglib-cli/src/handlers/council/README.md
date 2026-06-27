# council

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-council-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-council-complexity.json)

<!-- module-docs:start -->

`gglib council` subcommand group.

Organised as a directory module; each subcommand lives in its own file.

# Subcommands

| File | Subcommand | Purpose |
|------|------------|---------|
| [`run`]    | `council run "<goal>"` | Plan and execute a new task graph |
| [`list`]   | `council list [--status]` | List past orchestrator runs |
| [`show`]   | `council show <id>` | Detailed event timeline for a run |
| [`resume`] | `council resume <id>` | Continue an interrupted run |
| [`rewind`] | `council rewind <id> --wave N` | Roll back to a previous wave and re-execute |

# Shared helpers (private to the module)

| Symbol | Purpose |
|--------|---------|
| [`parse_hitl_mode`] | Parse `--hitl` string → [`HitlMode`] |
| [`init_session`]    | Spin up (or reuse) a llama-server, compose [`CouncilPorts`] |
| [`resolve_port`]    | Select an explicit port or auto-allocate one |
| [`stop_server`]     | Gracefully stop an auto-started llama-server |

# Internal architecture

```text
  stdin
    │
    ▼
presentation::input::spawn_input_router
    ├── /note <text>  ──►  NoteQueue  ──►  CouncilConfig  ──►  executor
    └── other line   ──►  mpsc::UnboundedReceiver<String>
                               │
                               ▼
                      approve::prompt_and_resolve
                      (tokio::time::timeout around recv())
                               │
                               ▼
                      CouncilApprovalRegistry::resolve
```

The event loop in `run` / `resume` / `rewind` receives [`CouncilEvent`]s
from the engine over a [`tokio::sync::mpsc`] channel and dispatches them
to [`render::render_event`], which either serialises them as JSONL
(`--json` mode) or renders them to the terminal with ANSI colour.

[`CouncilEvent`]: gglib_core::domain::council::events::CouncilEvent

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`approve.rs`](approve.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-approve-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-approve-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-approve-coverage.json) |
| [`list.rs`](list.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-list-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-list-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-list-coverage.json) |
| [`render.rs`](render.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-render-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-render-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-render-coverage.json) |
| [`resume.rs`](resume.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-resume-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-resume-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-resume-coverage.json) |
| [`rewind.rs`](rewind.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-rewind-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-rewind-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-rewind-coverage.json) |
| [`run.rs`](run.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-run-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-run-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-run-coverage.json) |
| [`show.rs`](show.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-show-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-show-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-council-show-coverage.json) |
<!-- module-table:end -->

</details>
