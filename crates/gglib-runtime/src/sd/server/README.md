# server

<!-- module-docs:start -->

Running `sd-server` and drawing with it: its command line, its launch, and
the one driver every door draws through. The command line, readiness and
the drawing rules are in [the parent README](../README.md).

| File             | What it holds                                                           |
|------------------|-------------------------------------------------------------------------|
| `config.rs`      | `SdServerConfig`: the model, its family, its components and the port    |
| `args.rs`        | `sd-server`'s argv from the config and the family's recipe              |
| `spawn.rs`       | starting it, with piped output, as llama-server is started              |
| `job.rs`         | `SdImageDriver`, the image generation port: admit, turn, submit, follow |
| `job_plan.rs`    | a request's model, size and count, refused before anything queues       |
| `job_api.rs`     | sd-server's async job API: submit, read a job, cancel                    |
| `job_poll.rs`    | following one job: stages, steps, decode, cancel, stall and deadline    |
| `fake_server.rs` | a test-only stand-in for `sd-server`: its model list and its job API    |

<!-- module-docs:end -->
