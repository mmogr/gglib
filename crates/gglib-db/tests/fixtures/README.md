# Library fixtures

`library-0.33.0.sqlite` is a library made by released gglib 0.33.0: schema
generation 5, `user_version` 5. The tests in `src/setup_fixture_tests.rs`
open a copy with this build and read every table back.

- Made 2026-11-12 by `scripts/make_db_fixture_0_33.sh` (#1890), from main
  `ad3d5369` plus #1887.
- From `gglib-0.33.0-aarch64-apple-darwin.tar.gz` of the GitHub release
  `v0.33.0`, SHA256
  `228d5bad2d28e61b1ad7b345a66b7de3b32bcc80f8e695cc28dd49c6cc081aa3`, checked
  against the release's `SHA256SUMS`.
- Its data folder was `/tmp/gglib-fixture`, so the stored paths start
  `/private/tmp/gglib-fixture/`. The script deletes that folder when it is
  done, and the tests need no file there.
- The only secret is the MCP env value `dummy`.

## What 0.33.0 wrote, and what was put in by hand

Only rows 0.33.0's own code wrote prove that a later build reads what a
release wrote.

Written by 0.33.0's CLI:

- `models`: three, from `model add` of three small GGUF files (a tool-calling
  `qwen3`, a plain `llama`, and a Qwen-Image 2.1 image model), then
  `model update` (sampling defaults, `--jinja`, `--mtp`, metadata) and
  `model capabilities`. This gives `inference_defaults`, `defaults_origin`
  (`user`), `server_defaults`, `dialect_spec` (dropped since), `tags`
  (step 8 strips its `format:hermes`), `metadata`, `capabilities` and
  `image_family`.
- `settings_kv`: `config default`, `config profile set`, and
  `config settings set` (context size, proxy port, tool iterations, loop
  guard mode, client sampling, allowed hosts).
- `mcp_servers`, `mcp_server_env`: `mcp add` twice, `mcp allow-remote`,
  `mcp disable`.

Put in by hand with `sqlite3`, because 0.33.0 writes them only with a model
running or from a Hub download. The chat `settings`, the template caps, the
tune rows, the tune run's `applied_json` and the agentic report are what
serde makes of 0.33.0's types, which are unchanged between `v0.33.0` and
the tree it was made from. The message `metadata` is typed by hand with the
keys 0.33.0 writes (`tool_calls` with each call's `display_name` and
`server`, `tool_call_id`, and a reply's counts):

- `models.template_caps` on model 1, and `models.defaults_origin` set to
  `published` on model 2.
- `model_files`: one row.
- `chat_conversations`, `chat_messages`, `attachments`,
  `message_attachments`: a chat with its `settings` (step 9 strips its
  `temperature` and `top_k`), a system, user,
  assistant (tool call), tool and assistant message with their `metadata`, a
  1x1 PNG on the user message, and a branch of it.
- `benchmark_runs` (one of each type and a failed one, the tune run with its
  `applied_json`), `model_perf_results`, `model_compare_results`,
  `model_benchmark_summaries`, `benchmark_tune_results`,
  `benchmark_agentic_results`.
- `loop_guard_trips`, `loop_guard_scans`.

Left empty or unset: `model_components`, the `models` columns `hf_*`,
`projector_path`, `variant_label` and `file_paths_json`, and every
`benchmark_runs.config_json`. A migration test that needs them adds them.

The file was then checkpointed, switched out of WAL mode so it is one file,
and vacuumed.

## Rules

- Never edit or regenerate this file. A test copies it and opens the copy.
- A migration's test copies it, adds the rows its step needs, and checks the
  step.
- Add a new `library-<version>.sqlite` only for a release that raised the
  schema generation.
