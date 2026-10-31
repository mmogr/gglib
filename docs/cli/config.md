# gglib config

Settings, the engines that run models, and where everything is. Settings
are optional and persist; a change affects what happens next, not a model or
a download already there.

<!-- snippet: config-settings-show -->
```console
$ gglib config settings show
Current application settings:

  General
  ────────────────────────────────────────
  agentic-sampling            None
  bind-host                   None
  close-to-tray               None
  default-context-size        None (sized per launch)
  default-download-path       None
  default-image-model-id      None
  default-model-id            1 (Qwen3-30B-A3B)
  inference-profiles          None
  llama-base-port             9300
  loop-guard-mode             None
  max-download-queue-size     None
  max-stagnation-steps        None
  max-tool-iterations         None
  mcp-drawing                 None (off)
  proxy-api-key               None
  proxy-autostart             None
  proxy-port                  8080
  remote-devices              None
  remote-enabled              None
  remote-pairing              None
  remote-serve                None
  share-lan                   None
  show-memory-fit-indicators  None
  start-at-login              None
  title-generation-prompt     None
  tool-call-repair            None
  trust-client-sampling       None

  Inference Defaults
  ────────────────────────────────────────
  (none configured)
```
<!-- /snippet -->

## `settings`

Context size, ports, the download queue, the agent loop's limits, display
preferences, and the switches the desktop app offers (`--proxy-autostart`,
`--close-to-tray`, `--start-at-login`).

```bash
gglib config settings set --default-context-size 131072
gglib config settings set --proxy-port 8081 --llama-base-port 9300
gglib config settings unset default-context-size
gglib config settings reset   # the defaults; keeps the pairing, its devices, remote serving, the API key
gglib config settings reveal proxy-api-key  # a stored secret, alone on stdout
```

- `show` prints every setting and its value; a secret is shown as set or
  not, and `reveal <name>` prints it alone on stdout, for a script.
- `proxy-port` is where `proxy`, `serve` and `up` start the endpoint when
  they are given no `--port`, where the desktop app starts it, and where
  `proxy dashboard` and `proxy cache-clear` look for it.
- `llama-base-port` is the first upstream port the daemon allocates
  llama-server from, behind the endpoint.

## `default`

The model a command uses when none is named: `gglib q` without `-m`, for
one. `gglib config default` shows it, `gglib config default <model>` sets
it, `--clear` unsets it. `up` fills an empty default with the model it
served.

## `profile`

Named sampling profiles, selectable per request as `<model>:<profile>`:
`Qwen3-4B:coding` in a client's model field picks the model and the profile,
and `--profile coding` does the same on `serve`, `chat` and `q`.

- `list`, `show <name>`, `set <name> [flags]`, `rm <name>`. `set` stores
  only the flags you pass, merged over what the profile already holds;
  `--unset <parameter>` clears one. The rest fall through to the model's
  own defaults, then the global ones ([Sampling resolution](../sampling.md)).
- `install-templates` creates the built-in profiles, `coding` among them.

## `models-dir`

Where downloads land. `show` prints the directory; `prompt` asks;
`set <PATH>` stores one.

- A running daemon's next download goes to the new directory; a download
  already running finishes where it started.
- `GGLIB_MODELS_DIR` in the daemon's own environment outranks the stored
  directory.

## `llama` and `sd`

The two engines: llama.cpp's `llama-server`, which serves chat models, and
stable-diffusion.cpp's `sd-server`, which serves image models. Each is
pinned to a release, and `status` says where the install stands against the
pin.

<!-- snippet: config-llama-status -->
```console
$ gglib config llama status
Status: Installed
Binary: /Users/mmogr/Library/Application Support/gglib/.llama/bin/llama-server
Pinned release: b11669, which is the one installed.
Health: ✓ Functional

Pre-built download:
  Version: b11669
  Platform: macOS ARM64 (Metal)
  Installed: 2026-10-31 06:24:56 UTC

Binary version: version: 0.7.0-dev (build 11669, commit 620d2225c)
  Build: b11669
  Native capabilities: RuntimeFlags(PEG_NATIVE_TOOL_CALLS)
```
<!-- /snippet -->

- `install` (also spelled `update`) does nothing when the pinned release,
  or a later one, is installed, or when what is there is a binary gglib has
  no record of installing or a release it cannot compare offline. When the
  install is older than the pin it installs the pin: this platform's
  pre-built release, warning first when that is a CPU-only build; or a
  source build of the pinned tag, where no release fits, over a source
  build of a release already there, or when `--build` asks for one. A
  source build is never replaced by a CPU-only download without `--force`.
- `--force` installs the pin whatever is there, taking the download over a
  source build unless `--build` is given too.
- `uninstall` removes the engine. `llama detect` reports the GPU
  acceleration this machine offers and whether a source build is ready to
  run.
- Neither engine is installed or removed while the daemon that serves this
  data root runs a model on it, as the desktop app's Settings refuses too.
  With no such daemon running nothing serves one, since only a daemon
  launches them.
- `gglib serve` and `gglib up` offer the install when an engine is missing,
  and the pin when what is installed is older. `make setup` installs both
  without asking, the image runtime after llama.cpp and skipped with a line
  if it fails; a source build, where one is needed, still asks before it
  starts at a terminal.

The image runtime has its own page:
[Stable Diffusion](../../crates/gglib-runtime/src/sd/README.md).

## `check-deps`

What a source build needs on this platform, what is missing, and the exact
install commands. It installs nothing unless asked, and exits with an error
while a required dependency is missing.

## `fast-downloads`

The optional `hf_xet` download accelerator: `status`, `enable`, `disable`,
`prompt`. Downloads work without it, over native HTTP.

- `enable` builds a Python environment gglib owns, under its own data
  directory, using `uv` when it is available; `--python <path>` names the
  interpreter to build it from. `disable` removes it.

## `paths`

Every location gglib resolved, with its source.

<!-- snippet: config-paths -->
```console
$ gglib config paths
binary = /Users/mmogr/.local/src/gglib/target/release/gglib
version = 0.29.0-rc.4 (12c44308c253)
data_root_source = GGLIB_DATA_DIR
daemon_port = 9907
data_root = /Users/mmogr/Library/Application Support/gglib
resource_root = /Users/mmogr/Library/Application Support/gglib
database_path = /Users/mmogr/Library/Application Support/gglib/data/gglib.db
llama_server_path = /Users/mmogr/Library/Application Support/gglib/.llama/bin/llama-server
sd_server_path = /Users/mmogr/Library/Application Support/gglib/.sd/bin/sd-server
models_dir = /Users/mmogr/Library/Application Support/gglib/models
models_source = EnvVar
```
<!-- /snippet -->

- `data_root` holds the database, the engines, logs and pids; it is per
  user, `~/Library/Application Support/gglib` on a Mac, and every build of
  gglib shares it unless `GGLIB_DATA_DIR` names another. This recording
  ran with `GGLIB_DATA_DIR` and `GGLIB_MODELS_DIR` set.
- `daemon_port` is the daemon's management port, not the endpoint's.

## Flags

`gglib config <subcommand> --help` lists every flag; `gglib config settings
set --help` lists every setting.
