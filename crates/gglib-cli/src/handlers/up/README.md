# Up

<!-- module-docs:start -->

`gglib up` — the first-run path, from a clean machine to a working
OpenAI-compatible endpoint in one command.

This module **composes**; it implements nothing that already exists elsewhere.
Every step delegates, which is what keeps `up` in step with the commands it
stands in for.

| Step | Delegates to |
|------|--------------|
| 1. Hardware | [`gglib_app_services::SetupOps::get_status`] — the same status the GUI's setup renders |
| 2. Engines | [`ensure_installed`](crate::handlers::config::engine::ensure_installed) for llama.cpp, as `gglib serve` uses, then for stable-diffusion.cpp, which is optional: skipping or failing it never stops `up` |
| 3. Model | [`gglib_runtime::system::recommend_here`] (`gglib_core::domain::recommend` against the load's budget), then `DownloadManagerPort::queue_smart` of the size it names + the shared interactive monitor; the model becomes the default when none is set, so `gglib q` needs no `-m` after `up` |
| 4. Proxy | [`start_or_leave`](crate::handlers::inference::proxy::start_or_leave): the unpinned endpoint as the stored settings describe it, as the service's own launch starts it; an endpoint already on otherwise (one model, another port) is left as it is and said so, with `gglib proxy` as the way back, never asked about and never switched by `-y` |
| 5. Warm-up | the daemon's proof request (`POST /api/proxy/prove`), the context it was served at in the daemon's fit sentence (`GET /api/models/{id}/explain`), and client block (`GET /api/proxy/clients`), printed as answered; the proof and the client block are the two `gglib proxy test` prints |

## Why the warm-up is an HTTP request

An unpinned proxy loads nothing until traffic arrives, so without a first
request the launch narration never prints and `up` ends on a bound socket
rather than evidence. Going over HTTP — rather than reaching into the runtime —
exercises the router, admission control, model resolution and the forward
pipeline, so the endpoint is demonstrated to work rather than assumed to.

The daemon owns the proxy, so `up` can await the warm request inline rather
than racing a foreground server. It never fails the command: a slow first load
leaves a perfectly usable proxy, and tearing that down would be worse than the
cold start it was avoiding.

## Not responsible for

Configuring anything beyond that empty default, or changing the endpoint.
`up` asks for the endpoint where the stored settings put it, takes three flags
(`--yes`, `--model`, `--port`), and leaves an endpoint that is already on as
it is. Host, upstream port, sampling and cache behaviour belong to `gglib
proxy`.

<!-- module-docs:end -->
