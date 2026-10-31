# gglib chat, q and run

Three ways to ask a model from the terminal. `chat` is a session with tools
that remembers; `q` is one question, built for pipes; `run` starts a reply
the daemon finishes while you do something else. Every one goes through the
daemon, so the model it loads is the model the proxy and the chat page see,
unless `--port` names a llama-server already running here.

<!-- snippet: q -->
```console
$ gglib q "In one sentence, what is a GGUF file?"

  ╭─ ℹ️ Info ─────────────────────────────╮
  Starting llama-server for 'Qwen3-4B-Instruct-2507' via the gglib daemon (this may take a moment) …
  llama-server ready on port 9300

  ╰───────────────────────────────────────╯
A GGUF file is a general-purpose, flexible format for storing models in the LLM (Large 
Language Model) ecosystem, specifically designed to support various model architectures 
and hardware types.

Continue chatting? (Y/n): 
n
```
<!-- /snippet -->

## `q`

One-shot. `{}` in the question is replaced by piped or `--file` input, and
`-Q` / `--quiet` strips tool progress and reasoning tokens for a script.
Without `-m` it asks the default model ([config default](config.md#default));
under `--remote`, the model last asked of that machine.

<!-- snippet: q-pipe -->
```console
$ grep -E "WARN|ERROR" server.log | gglib q -Q "What went wrong here?"
The logs indicate that the application is experiencing a database connection issue. Specifically:

- The database connection pool has reached its maximum capacity of 10 connections.
- Each time a request arrives (e.g., `/orders/export`, `/orders`), the system tries to allocate a new connection, but all 10 slots are already in use.
- As a result, requests are being queued and eventually timeout after 5 seconds.
- When the timeout occurs, the server returns a 500 Internal Server Error.

### Root Cause:
The database pool is saturated. This suggests either:
1. Too many concurrent requests are hitting the system (e.g., due to high traffic or misconfigured load).
2. The application is not properly closing database connections after use (leak).
3. The pool size (10) is too small for current demand.

### Recommendations:
- Increase the database connection pool size (configurable in the DB setup).
- Ensure all database connections are properly closed in the code.
- Add logging or monitoring to track request volume and connection usage.
- Implement circuit breakers or rate limiting to prevent overloading.
```
<!-- /snippet -->

```bash
gglib q --file README.md "Summarize this project"
gglib q --image shot.png "What is the error?"
gglib q --remote -m <id> "Why is this test flaky?"   # on the paired machine
```

- At a terminal, once the answer is in, `q` offers to go on as a chat
  (`Continue chatting?`); `-Q`, or input from a pipe, skips the offer.
- `question` is the long spelling of the same command.

## `chat`

An interactive session with tool access: gglib's own file tools plus any
[MCP server](mcp.md) you added. `--no-tools` for plain chat.

<!-- snippet: chat -->
```console
$ gglib chat Qwen3-4B-Instruct-2507

  ╭─ ℹ️ Info ─────────────────────────────╮
  Starting llama-server for 'Qwen3-4B-Instruct-2507' via the gglib daemon (this may take a moment) …
  llama-server ready on port 9300

  ╰───────────────────────────────────────╯
Files: /Users/mmogr/Library/Application Support/gglib/home/orders-api (read-only)
Agentic chat ready. Type /help for help, /quit to exit.
Session #10 — resume later with: gglib chat --continue 10
You: Read app.py and say in one sentence what it serves.

  ⚙  Read Text File  app.py …
  ✓  Read Text File  3ms  import io

from flask import Flask, Response

from db import pool
from export i…
The app.py file serves a Flask application that provides an API endpoint to retrieve the
most recent 50 order records and another endpoint to export orders in CSV format.
You: /exit
Session #10 saved. Resume with: gglib chat --continue 10
```
<!-- /snippet -->

### Conversations persist

- `--continue <id>` resumes one on the machine it ran on, and `gglib chat
  history` lists them, each model by its id and its machine.
- A turn is saved as the chat page's is: your message when you send it, the
  reply when the turn ends, with its reasoning and how it was made. A turn
  stopped by Ctrl+C or an error keeps what had arrived of its reply, marked
  as stopped.
- A chat the daemon is still replying to, for the chat page or a paired
  device, is not continued: `--continue` says the chat is running elsewhere
  and names the run, before it stores anything. Wait for the reply, or stop
  it with `gglib run cancel <run>`.

### The folder it reads

The file tools, `read_file`, `list_directory` and `grep_search`, read the
directory `chat` was launched in, read-only, and nothing outside it. The
session says so once, on stderr: `Files: <dir> (read-only)`.

- That is where each invocation runs, `--continue` included: a folder the
  chat page chose for the same conversation is not read here.
- Launched in your home folder or in `/` it reads no files, and says
  `Files: off in your home folder or /; cd into a project to read it, or
  --no-tools`; launched in a directory deleted since, it reads none and says
  it cannot read it.
- Under `--tools` the file tools are kept only when the list names one, bare
  (`read_file`) or as `builtin:read_file`; a list that names none gets none,
  no `Files:` line, and its directory is not read.
- An MCP server you added is not told this folder: it reads what its own
  arguments name ([mcp](mcp.md)).
- Under `--remote` the line reads `Files: <dir> on this machine (read-only;
  what is read is sent to <machine>)`, and `/draw` answers that it cannot
  draw here.

### Thinking

`--thinking on|off` is the chat page's Thinking switch, from the terminal:
the choice runs this session and the chat remembers it, on a new chat or
with `--continue`.

- `off` runs with a thinking budget of `0`, whatever
  `--reasoning-budget-tokens` says; `on` runs with the budget typed.
- Without it a resumed chat runs as it remembers, and a chat switched off
  says so on stderr when that sets a typed budget aside.

[The two reasoning controls](../sampling.md#the-two-reasoning-controls)
explains the budget.

### Guardrails for tool calls

Local models need guardrails to finish a tool-calling task, so the loop
carries them.

- `--max-iterations` caps the loop; without it, the limit a resumed chat
  saved, then the `max-tool-iterations` setting, which is the order the chat
  page and a paired device's turn follow too.
- `--tools` is an allowlist, evaluated once at session start; parallelism
  and timeout caps bound each step.
- A dual-threshold loop guard: `--observation-tool` and
  `--max-observation-steps` let read-only tools repeat more often than
  mutating ones before loop detection fires.

### `/draw`

In the session, `/draw` lets the model draw a picture for the next message
typed, and only that one, with the daemon's image model
([gglib image](image.md)).

- `/draw list` prints the daemon's image models, one to a line: a `*` on
  the one the next `/draw` message would draw with, its id, name and
  family, and whether it is the default, loaded, this session's choice, or
  missing a file.
- `/draw <model>` chooses one for the session, by its id, its name, or a
  part of the name no other has, and arms the next message. A name that
  fits none or several changes nothing and is answered with the list; a
  model that cannot draw, with the daemon's reason.
- The choice is held until the session ends: every later `/draw` draws with
  it and says so. `/draw default` forgets it, and arms the next message
  with the default. A chosen model that has since
  gone, or lost a file, is never swapped for the default: `/draw` says why it
  cannot draw and how to choose again.
- The choice is not saved with the chat, and the chat model is neither told
  of it nor able to make it.

## `run`

A reply the daemon finishes on its own. `start` prints the run's id and
returns; `show` prints the reply so far; `list` shows the daemon's runs,
newest first; `cancel` stops one.

<!-- snippet: run-start -->
```console
$ gglib run start -m Qwen3-4B-Instruct-2507 "Summarise the builder pattern in two sentences"
run-d28875f6b650
  Started. Read it with `gglib run show run-d28875f6b650`.
```
<!-- /snippet -->

<!-- snippet: run-show -->
```console
$ gglib run show run-d28875f6b650
The builder pattern is a creational design pattern that separates the construction of a complex object from its representation, allowing the same construction process to create different representations. It is especially useful when an object has many optional components, as it provides a clean and readable way to build the object step by step.
  run run-d28875f6b650 completed
```
<!-- /snippet -->

- `start -m <model> "<prompt>"` names the model as the proxy does;
  `--follow` prints the reply as it arrives instead of the id. `show -f`
  keeps printing until the run ends.
- A run is what the chat page and a paired device start too, which is why
  `chat --continue` names one when the chat it wants is still being
  answered.

## `attachment save`

A chat stores the images it carries, yours and the ones a tool makes, in
this machine's database, and shows each as a marker with its size and the
start of its id: `[image 1024x1024 3f9a2c1e]`. A tool's images end its line
in the live turn, and `--continue` shows the last turn's. `gglib attachment
save <id> [path]` writes one to a file, as stored.

```bash
gglib attachment save 3f9a2c1e
gglib attachment save 3f9a2c1e ~/Pictures/dot.png --force
```

- The id may be whole, or its first 8 characters or more. A start two
  images share is refused with how many share it, and one no image has is
  refused too.
- Without a path the file is `./<id8>.png` (or `.jpg`); a directory takes
  that name inside it. An existing file is not replaced without `--force`.

## Flags

`gglib chat --help`, `gglib q --help` and `gglib run start --help` list
every flag; the shared groups are in
[the index](README.md#flags-several-commands-share).
