# gglib proxy, serve and daemon

The endpoint, and the process behind it. `proxy` serves every model in the
library from one port and swaps them on demand; `serve` is the same stack
pinned to one model; `daemon` is the process that owns llama-server for
both, and for every other surface.

```bash
gglib proxy                       # every model, on the stored proxy_port (8080)
gglib proxy --port 8081 --host 0.0.0.0 --allowed-host desk.local
gglib serve Qwen3-4B-Instruct-2507        # one model; other names are refused
gglib proxy                       # every model again, after serve
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
`/v1/models/{name}`, `/v1/images/generations` and `/mcp` (MCP Streamable HTTP) from one port. When a request names a model that is
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
- It is usually on already: gglib turns it on when it starts
  (`proxy-autostart`). See [Switching](#switching) for a `proxy` or `serve`
  that asks for it otherwise.
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
to each, in words: a tool call repaired, old messages trimmed, a loop
noted or stopped, a request refused. Each loaded model's
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
DAY (UTC)   MODEL                   VERSION      SCANNED  TRIPS  LOOPS  STAGN.  SESSIONS
2026-11-09  Qwen3-4B-Instruct-2507  0.31.0-rc.8        9      0      0       0         0
2026-11-07  Qwen3-4B-Instruct-2507  0.31.0-rc.3        1      0      0       0         0
2026-11-07  Qwen3-4B-Instruct-2507  0.31.0-rc.5        1      0      0       0         0
2026-11-07  Qwen3.8-27B             0.31.0-rc.3        1      0      0       0         0
2026-10-31  Qwen3-30B-A3B           0.29.0-rc.1        1      0      0       0         0
2026-10-31  Qwen3-4B-Instruct-2507  0.29.0-rc.4        2      0      0       0         0
2026-10-31  Qwen3.8-27B             0.29.0-rc.1       10      0      0       0         0
2026-10-31  Qwen3.8-27B             0.29.0-rc.4        1      0      0       0         0

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
- `--jinja auto|on|off` is this launch's choice of chat templates, the
  values `model update --jinja` takes for the model's own default, which it
  outranks; `auto`, or no flag, leaves it to that default and the tags.
- An image model is served by stable-diffusion.cpp's `sd-server` instead.
  `serve` offers to install it if it is missing, or older than its pinned
  release (`gglib config sd install`), prints the model's family and each
  component linked or missing, starts the same pinned proxy, and loads the
  model once (`POST /v1/gglib/models/{name}/load`), so a refusal, the runtime not
  installed, a component missing or no room beside a held model, is printed
  in words before the dashboard attaches. The chat-only flags (`--context-size`,
  `--mlock`, `--jinja`, MTP, sampling, a profile) are refused by name.

## Switching

There is one endpoint, so a `proxy` or `serve` that finds it on otherwise
than its flags ask changes it, and that cuts off the clients connected to it.
So it asks first, naming what runs and what switching does:

```console
$ gglib serve Qwen3-4B
  The endpoint is on at port 8080, serving every model. Serve only Qwen3-4B? Clients connected now are cut off for a moment. (Y/n):
```

- One model and back: `serve <model>` asks to serve only that model, `serve
  <other>` asks to serve it instead, and `proxy` with no flags, while one
  model is served, asks to serve every model again. After `serve`, the
  closing lines name `gglib proxy` for the way back, and `daemon status`
  says it under the pinned endpoint.
- Flags: a `--port`, `--host`, sampling flag, `--cache`, `--api-key` or
  `--allowed-host` the running endpoint was not started with is named in
  the question, and a yes restarts it with them.
- A switch to another port or address says so: clients connected now stay
  cut off until they are pointed at the new address.
- Yes switches it, as one operation on the daemon, even with a reply still
  streaming: a start that is refused then, a port another program holds for
  one, puts back what ran and says so, or says the endpoint is off when that
  fails too. No changes nothing, says so and exits non-zero, so a script
  after it does not go on. An endpoint already as asked is said so, and
  nothing is asked.
- `-y` answers yes. The question is asked only when both the keyboard and
  the screen it is written to are a terminal. Otherwise, with no `-y`,
  nothing changes, the command exits non-zero, and it names the command to
  run: the same command line with `-y`.
- When gglib is not running, a `proxy`, `serve`, `up` or `model load --pin`
  that has to start it leaves its endpoint off at that start and starts its
  own: nothing is asked, in a terminal or a script. The question is for an
  endpoint that was on before the command ran.
- `model load --pin` asks the same way, and takes `-y` too.
- `serve` with the model already served but other launch options, a
  `--jinja` or a context of its own, asks to serve it with those.
- `up` never switches it: it asks for the endpoint as the stored settings
  describe it, and one already on otherwise, serving one model or on another
  port, is left as it is and said so, with what it did not do and the
  command that would (`gglib proxy`, with `--port` when `up` was given one).
  `-y` does not switch it either.
- The desktop app's Serves menu, Unpin and pinned Load are the choice
  themselves: each switches without asking, in the same one operation. A
  service older than the app does not switch, and the app says so.
- An endpoint off because its port is taken, by another program or another
  gglib service, says so wherever it is shown: `daemon status` (`Why:`),
  the sidebar and the Endpoint page. The service still starts.

## `daemon`

One process owns the runtime: it holds an exclusive lock on the data
directory and launches every llama-server and sd-server. Commands that need
it start one when none is up; a foreign process on its port is reported, not
fought.

- `status` says whether it runs, its pid, and whether the proxy is up, and
  why not when its port was taken;
  under one serving a single model, the command that serves every model
  again.
- `run` runs it in the foreground. It listens on `127.0.0.1` only;
  `--allowed-host` accepts one more Host header value beside loopback.
- `stop` stops the daemon, and with it every llama-server it owns.

## `gui` and `web`

`gglib gui` launches the desktop app. `gglib web` makes sure the daemon is
up and prints the web page's address; the daemon serves the page and the
API from one loopback port, and the link carries the daemon's token, so
`web` opens your browser on it unless `--no-open`. The page is for this
machine only: the daemon listens on loopback, and another machine reaches
models through the proxy (`proxy --host`) or a paired tunnel
(`gglib remote`).

## Flags

`gglib proxy --help`, `gglib serve --help` and `gglib daemon run --help`
list every flag.
