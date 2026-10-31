# gglib up

From nothing to a working endpoint, in one command. `up` reads your
hardware, installs llama.cpp and the image runtime if they are missing (or
offers the pinned release when what is installed is older), recommends a model that fits your memory and downloads it (asking first),
starts the proxy, sends one real request through it to prove the endpoint
answers, prints the settings to paste into your client, and ends with what
to run next.

<p align="center">
  <img src="../assets/cli-up.gif" alt="gglib up: from nothing to a working endpoint, then a log piped into gglib q" width="800">
</p>

```bash
gglib up                      # ask before downloading
gglib up --yes                # take the recommendation
gglib up --model qwen3.6 --port 8081
```

## Running it again

Safe to re-run: an engine at its pinned release or newer and a model already
in the library are skipped, so a second run is "start the proxy" and the proof that
it answers.

<!-- snippet: up-again -->
```console
$ gglib up

  gglib up — from nothing to a working endpoint

[1/5] Hardware
    backend   Metal
    memory    96.0 GiB  (unified, usable share)
    models    /Users/mmogr/Library/Application Support/gglib/models

[2/5] Engines
    chat      llama.cpp already installed
    images    stable-diffusion.cpp already installed

[3/5] Model
    model     Qwen3.8-27B  (already installed; --model picks another)

[4/5] Endpoint

  ✓ endpoint answered in 20.0s

  Connect your client

    Cline / Roo / Continue   provider: OpenAI Compatible
      Base URL   http://127.0.0.1:8080/v1
      API Key    not-needed  (unused, but most clients demand one)
      Model ID   Qwen3.8-27B

    Open WebUI               Settings -> Connections -> OpenAI API
      URL        http://127.0.0.1:8080/v1

    VS Code Copilot (BYOK) needs one fixed model, so use:
      gglib serve 3

    Check it from another shell:
      curl http://127.0.0.1:8080/v1/models

  Dashboard  http://127.0.0.1:8080/v1/proxy/status

  Ready. The endpoint keeps running in the background.

    gglib chat 3                        chat in this terminal
    gglib web                           open the chat page in your browser
    gglib proxy dashboard --port 8080   watch requests as they arrive
    gglib proxy stop                    stop the endpoint
```
<!-- /snippet -->

## What to know

- The endpoint keeps running on the daemon after `up` returns, until
  `gglib proxy stop`.
- The model `up` serves becomes the default when there is none, so
  `gglib q "…"` needs no `-m` afterwards. A default you set is kept.
- `up` is deliberately unconfigurable beyond `--yes`, `--model` and
  `--port`: it binds loopback with gglib's defaults. Reach for
  [`gglib proxy`](proxy.md) to choose the host, the upstream port, sampling
  or cache behaviour.
- The image runtime is optional: skipping or failing its install never
  stops `up`. Chat models need only llama.cpp.
- `up` composes the commands it stands in for, and runs the same code:
  `config llama install`, `model download`, `proxy`. The
  [up module](../../crates/gglib-cli/src/handlers/up/README.md) lists each
  step and what it delegates to.

## Flags

`gglib up --help` lists them all.
