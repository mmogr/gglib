# gglib proxy, serve and daemon

The endpoint, and the process behind it. `proxy` serves every model in the
library from one port and swaps them on demand; `serve` is the same stack
pinned to one model; `daemon` is the process that owns llama-server for
both, and for every other surface.

```bash
gglib proxy                       # every model, on the stored proxy_port (8080)
gglib proxy --port 8081 --host 0.0.0.0 --allowed-host desk.local
gglib serve Qwen3-4B-Instruct-2507        # one model; other names are refused
gglib proxy dashboard             # live connections, slots, cache, requests
gglib proxy test                  # one real request through it, then how a client connects
gglib proxy stop
```

<!-- snippet: daemon-status -->
```console
$ gglib daemon status

  ╭─ ℹ️ Daemon ───────────────────────────╮
  Status:  running at http://127.0.0.1:9907
  PID:     11834
  Proxy:   On at http://127.0.0.1:8080/v1
  API key: None needed

  ╰───────────────────────────────────────╯
```
<!-- /snippet -->

## `proxy`

Serves `/v1/chat/completions`, `/v1/embeddings`, `/v1/models`,
`/v1/images/generations` and `/mcp` (MCP Streamable HTTP) from one port. When a request names a model that is
not resident, the proxy queues it, launches the model, and applies flags
derived from its capability tags: `mtp` → speculative decoding, `reasoning`
→ `--reasoning-format`, `agent` → `--jinja`. Every surface uses the same
config builder, so a model behaves identically however it was started.

- Sampling flags given here are proxy-wide defaults; per-request and
  per-model values still win ([Sampling resolution](../sampling.md)).
- Without `--port`, `proxy`, `serve` and `up` start the endpoint on the
  stored `proxy_port` (`gglib config set proxy-port`), where the
  desktop app starts it, and `proxy dashboard` and `proxy cache-clear` look
  for it there.
- The endpoint runs on the daemon, so it outlives the terminal that started
  it; `proxy stop` ends it.
- The flag groups `proxy` shares with `serve`, sampling, cache and access,
  are in [the index](README.md#flags-several-commands-share).

### `test`

Sends one real request through the running endpoint and says who answered
and in how long, the load included, then how a client connects to it: the
address, whether a request needs the key, the Model ID a client sends, and a
recipe per client (opencode, Cline / Roo / Continue, Open WebUI, VS Code
Copilot, any OpenAI client). All of it is the daemon's: it sends the request
to its own endpoint with the key the endpoint demands, and writes the
recipes from the address it is bound at, so `gglib up`, this command and the
desktop app show one block. With no model named it uses the pinned model,
else the one loaded chat model, else asks you to name one. Exits non-zero
when the proof fails, so a script can tell; the block is printed first.

### `dashboard`, `cache-clear`, `trips`, `stop`

A live terminal view of an already-running proxy, with its connections,
slots, cache and requests, and the last five requests with what gglib did
to each, in words: a tool call repaired, old messages trimmed, tool calls
constrained, a loop noted or stopped, a request refused. Each loaded model's
row says what it is doing and, under it, how it was launched in one line
(quant, context, KV cache types, speculative decoding, size); the second
slot says how much room a second model has, or why the last one was
refused it:

<p align="center">
  <img src="../assets/cli-proxy-dashboard.gif" alt="gglib proxy dashboard while a request is answered" width="800">
</p>

`cache-clear`, when the disk cache is on, clears every disk slot and
recycles llama-server, which is the one way to drop the host-RAM prompt
cache; the recycle is skipped while a
request is in flight, and `--session-id` clears one session's disk slots
instead. `trips` is the loop guard's log: what it scanned and what it acted
on, by day.

<!-- snippet: proxy-trips -->
```console
$ gglib proxy trips
The loop guard's log, the last 30 day(s):
DAY (UTC)   MODEL                         VERSION    MODE    SCANNED  TRIPS  LOOPS  STAGN.  SESSIONS
2026-11-09  Qwen3-4B-Instruct-2507        0.31.0-rc.8  note          9      0      0       0         0
2026-11-07  Qwen3-4B-Instruct-2507        0.31.0-rc.3  note          1      0      0       0         0
2026-11-07  Qwen3-4B-Instruct-2507        0.31.0-rc.5  note          1      0      0       0         0
2026-11-07  Qwen3.8-27B                   0.31.0-rc.3  note          1      0      0       0         0
2026-10-31  Qwen3-30B-A3B                 0.29.0-rc.1  note          1      0      0       0         0
2026-10-31  Qwen3-4B-Instruct-2507        0.29.0-rc.4  note          2      0      0       0         0
2026-10-31  Qwen3.8-27B                   0.29.0-rc.1  note         10      0      0       0         0
2026-10-31  Qwen3.8-27B                   0.29.0-rc.4  note          1      0      0       0         0

A trip is the guard's decision, not a delivery: a noted request can still fail to reach the model.
```
<!-- /snippet -->

The guard itself is described in
[gglib-proxy](../../crates/gglib-proxy/README.md#loop--stagnation-defence).

## `serve`

The same proxy stack pinned to one model: a request naming any other is
refused rather than swapped. That is what clients which cannot switch models
through `/v1/models` need, VS Code Copilot's BYOK endpoint among them.

- `--port` is the endpoint; the daemon allocates the upstream llama-server
  port behind it, from `llama_base_port`
  (`gglib config set llama-base-port`).
- An image model is served by stable-diffusion.cpp's `sd-server` instead.
  `serve` offers to install it if it is missing, or older than its pinned
  release (`gglib config sd install`), prints the model's family and each
  component linked or missing, starts the same pinned proxy, and loads the
  model once (`POST /v1/models/{name}/load`), so a refusal, the runtime not
  installed, a component missing or no room beside a held model, is printed
  in words before the dashboard attaches. The chat-only flags (`--ctx-size`,
  `--mlock`, `--jinja`, MTP, sampling, a profile) are refused by name.

## `daemon`

One process owns the runtime: it holds an exclusive lock on the data
directory and launches every llama-server and sd-server. Commands that need
it start one when none is up; a foreign process on its port is reported, not
fought.

- `status` says whether it runs, its pid, and whether the proxy is up.
- `run` runs it in the foreground. It listens on `127.0.0.1` only;
  `--allowed-host` accepts one more Host header value beside loopback.
- `stop` stops the daemon, and with it every llama-server it owns.

## `gui` and `web`

`gglib gui` launches the desktop app (`--dev` for hot reload, for
contributors). `gglib web` makes sure the daemon is up and prints the web
page's address; the daemon serves the page and the API from one loopback
port, and the link carries the daemon's token, so `web` opens your browser
on it unless `--no-open`. The page is for this machine only: the daemon
listens on loopback, and another machine reaches models through the proxy
(`proxy --host`) or a paired tunnel (`gglib remote`).

## Flags

`gglib proxy --help`, `gglib serve --help` and `gglib daemon run --help`
list every flag.
