# gglib config

Settings, the engines that run models, and where everything is. Settings
are optional and persist; a change affects what happens next, not a model or
a download already there.

<!-- snippet: config-show -->
```console
$ gglib config show
Current application settings:

  General
  ────────────────────────────────────────
  close-to-tray            None
  default-context-size     None (sized per launch)
  default-image-model-id   None
  default-model-id         2 (Qwen3-4B-Instruct-2507)
  inference-profiles       None
  llama-base-port          9300
  loop-guard               None
  max-download-queue-size  None
  max-parallel-tools       None
  max-stagnation-steps     None
  max-tool-iterations      None
  mcp-drawing              None (off)
  models-dir               /Users/mmogr/.local/share/gglib-demo/models (environment, Writable)
  proxy-allowed-hosts      None
  proxy-api-key            None
  proxy-autostart          None
  proxy-cache-disk-gb      None
  proxy-cache-on-disk      None
  proxy-host               None
  proxy-port               8080
  remote-devices           None
  remote-enabled           None
  remote-pairing           None
  remote-serve             None
  start-at-login           None
  title-generation-prompt  None
  tool-call-repair         None
  tool-timeout-ms          None
  trust-client-sampling    None

  Inference Defaults
  ────────────────────────────────────────
  (none configured)
```
<!-- /snippet -->

## `set`, `unset`, `show`

`show` prints every setting and the models directory, each under the key
`set` and `unset` take. `set <key> <value>` stores one; `config set --help`
lists the keys and what each takes. A value is a whole number, `true` or
`false`, text, or a comma-separated list for `proxy-allowed-hosts`; one a
setting does not allow is refused, and nothing is stored.

```bash
gglib config set default-context-size 131072
gglib config set proxy-port 8081
gglib config set default-model-id Qwen3-4B   # an id or a name
gglib config set models-dir ~/models         # created when missing
gglib config unset default-context-size
gglib config reset   # the defaults; keeps the pairing, its devices, remote serving, the API key
gglib config reveal proxy-api-key  # a stored secret, alone on stdout
```

- A secret is shown as set or not; `reveal <name>` prints it alone on
  stdout, for a script.
- `default-model-id` is the model a command uses when none is named: `gglib
  q` without `-m`, for one. `up` fills an empty default with the model it
  served. `default-image-model-id` is the same for drawing, and is refused
  unless the model draws.
- `proxy-port` is where `proxy`, `serve` and `up` start the endpoint when
  they are given no `--port`, where the desktop app starts it, and where
  `proxy dashboard` and `proxy cache-clear` look for it.
- `llama-base-port` is the first upstream port the daemon allocates
  llama-server from, behind the endpoint.
- `proxy-host`, `proxy-allowed-hosts`, `proxy-cache-on-disk` and
  `proxy-cache-disk-gb` are how the endpoint starts when nothing says
  otherwise: the address it binds (loopback until set; `0.0.0.0` accepts
  LAN clients and makes it demand the API key), the host names it answers
  to beyond its own, and whether its prompt cache lives on disk and within
  how many GiB. `proxy --host`, `--allowed-host`, `--cache` and
  `--cache-disk-gb` override them for one run; the desktop app and the
  tray follow them, and `up` binds loopback by name. A stored budget
  outranks `GGLIB_CACHE_DISK_GB`, as the flag does.
- `max-parallel-tools` and `tool-timeout-ms` are how many tool calls an
  agent run makes at once and how long one may take, for every daemon turn
  whose chat saved none of its own; the desktop app sets them under
  Settings › Tools › Agent limits.
- `proxy-autostart`, `close-to-tray` and `start-at-login` are the switches
  the desktop app offers under Settings › General.
- `inference-profiles` is edited with `profile`, below, and
  `inference-defaults` under Settings › Models in the app; `unset` clears
  either. The `remote-*` keys are written by `gglib remote`.

### `models-dir`

The download folder: where `gglib model download`, `gglib up` and the app
put the files they fetch. A model you `add` stays where it is, so the library
need not be one folder. `show` prints the directory, where it was chosen
(`explicit`, `environment` or `default`) and its state, the word Settings
shows: Writable, Will be created or Not writable. `unset models-dir` removes
the stored folder, so the default applies.

- A running daemon's next download goes to the new directory; a download
  already running finishes where it started.
- `GGLIB_MODELS_DIR` in the daemon's own environment outranks the stored
  directory.
- The daemon's first start stores the default folder, so it then shows as
  `environment`, and a later release with another default keeps using it.

## `profile`

Named sampling profiles, selectable per request as `<model>:<profile>`:
`Qwen3-4B:coding` in a client's model field picks the model and the profile,
and `--profile coding` does the same on `serve`, `chat` and `q`.

- `list`, `show <name>`, `set <name> [flags]`, `remove <name>`. `set` stores
  only the flags you pass, merged over what the profile already holds;
  `--unset <parameter>` clears one. The rest fall through to the model's
  own defaults, then the global ones ([Sampling resolution](../sampling.md)).
- `install-templates` creates the built-in profiles, `coding` among them.

## `llama` and `sd`

The two engines: llama.cpp's `llama-server`, which serves chat models, and
stable-diffusion.cpp's `sd-server`, which serves image models. Each is
pinned to a release, and `status` says where the install stands against the
pin. Its first line is the engine's state in the word Settings shows:
Installed, Installed, not working (llama.cpp's binary failed its check) or
Not installed.

<!-- snippet: config-llama-status -->
```console
$ gglib config llama status
Status: Installed
Binary: /Users/mmogr/Library/Application Support/gglib/.llama/bin/llama-server
Pinned release: b11960, which is the one installed.
Health: ✓ Functional

Pre-built download:
  Version: b11960
  Platform: macOS ARM64 (Metal)
  Installed: 2026-11-09 10:12:02 UTC

Binary version: version: 0.8.0-dev (build 11960, commit 8642541b2)
  Build: b11960
  Native capabilities: RuntimeFlags(PEG_NATIVE_TOOL_CALLS)
```
<!-- /snippet -->

- `install` is how an engine is updated too. It does nothing when the pinned release,
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
  data root runs a model on it, as Settings › Engines in the app refuses too.
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
`prompt`. Downloads work without it, over native HTTP. `status` says
`Fast downloads: On` or `Off`, the word Settings shows.

- `enable` builds a Python environment gglib owns, under its own data
  directory, using `uv` when it is available; `--python <path>` names the
  interpreter to build it from. `disable` removes it.

## `paths`

Every location gglib resolved, with its source.

<!-- snippet: config-paths -->
```console
$ gglib config paths
binary = /Users/mmogr/.local/src/gglib/target/release/gglib
version = 0.31.0-rc.8 (062c4956c712)
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

`gglib config <subcommand> --help` lists every flag; `gglib config set
--help` lists every setting.
