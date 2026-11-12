# gglib chat, q and run

Three ways to ask a model from the terminal. `chat` is a session with tools
that remembers; `q` is one question, built for pipes; `run` starts a reply
the daemon finishes while you do something else. Every one goes through the
daemon, so the model it loads is the model the proxy and the chat page see.
`chat` and `q` are clients of the same door as the chat page: the daemon runs
each turn through the endpoint, with the chat's tools, and saves it to the
chat. While the model loads, the reply waits on one line on stderr,
`Loading Qwen3-4B… 12 s`.

<!-- snippet: q -->
```console
$ gglib q "Write a haiku about a cold start"
The system begins,  
Cold wake of servers awaiting—  
Silent, steady light.
  0.7 s · 94 t/s · 0 tokens reused

Continue chatting? (Y/n): 
n
```
<!-- /snippet -->

## `q`

One-shot. `{}` in the question is replaced by piped or `--file` input, and
`-Q` / `--quiet` strips tool progress and reasoning tokens for a script.
Without `-m` it asks the default model (`default-model-id`, [config set](config.md#set-unset-show));
under `--remote`, the model last asked of that machine.

<!-- snippet: q-pipe -->
```console
$ grep -E "WARN|ERROR" server.log | gglib q -Q "What went wrong here?"
The logs indicate that the application is experiencing a **database connection pool exhaustion** issue. Here's what went wrong:

1. **Database Pool at Capacity**:
   - The log shows "pool at 10/10 connections", meaning the database connection pool has reached its maximum limit of 10 connections.
   - Every request that requires a database connection (like `/orders` or `/orders/export`) is being queued because no additional connections are available.

2. **Connection Timeout Errors**:
   - The error "timed out after 5s waiting for a connection" means the application waited up to 5 seconds for a connection but couldn't get one, resulting in a **500 Internal Server Error**.

3. **Request Failures**:
   - The failed requests (`GET /orders`, `GET /orders/export`) result in HTTP 500 errors, indicating the server could not fulfill these requests due to the database being unresponsive from the connection pool perspective.

### Root Cause
The database connection pool has been filled up with active connections (possibly due to:
- Too many concurrent requests,
- Long-running database queries,
- Inefficient query handling or lack of proper connection reuse,
- No connection timeout or auto-recovery mechanisms).

### Recommended Actions
1. **Increase the connection pool size** in the configuration if possible.
2. **Optimize queries** to reduce execution time and avoid holding connections for long durations.
3. **Implement connection pooling recovery** or circuit breakers to handle failures gracefully.
4. **Add logging** to identify which requests are causing the most connections.

Without additional context (e.g., code, configuration), this is the best interpretation of the problem.
```
<!-- /snippet -->

```bash
gglib q --file README.md "Summarize this project"
gglib q --image shot.png "What is the error?"
gglib q --remote -m <id> "Why is this test flaky?"   # on the paired machine
```

- At a terminal, once the answer is in, `q` offers to go on as a chat
  (`Continue chatting?`); `-Q`, or input from a pipe, skips the offer.
- The question is a saved chat from the start, made before it is asked:
  a question that fails keeps what arrived of its reply, marked as stopped.
- Its file tools read the directory it was asked in.
- `question` is the long spelling of the same command.

## `chat`

An interactive session with tool access: gglib's own file tools plus any
[MCP server](mcp.md) you added. `--no-tools` for plain chat.

<!-- snippet: chat -->
```console
$ gglib chat Qwen3-4B-Instruct-2507
Files: /Users/mmogr/Library/Application Support/gglib/home/orders-api (read-only)
Agentic chat ready. Type /help for help, /quit to exit.
Session #20 — resume later with: gglib chat --continue 20
You: Read app.py and say in one sentence what it serves.

  ⚙  Read File  app.py …
  ✓  Read File  0ms  22 lines  import io
The app.py file serves a Flask application that provides an endpoint to retrieve the 
latest 50 orders in JSON format and another endpoint to export orders in CSV format.
  0.5 s · 89 t/s · 728 tokens reused
You: /exit
Session #20 saved. Resume with: gglib chat --continue 20
```
<!-- /snippet -->

### Conversations persist

- `--continue <id>` resumes one on the machine it ran on, and `gglib chat
  history` lists them, each model by its id and its machine.
- A turn is the chat page's, saved by the daemon: your message when you send
  it, the reply when the turn ends, with its reasoning and how it was made.
  Ctrl+C stops the run on the daemon, and a turn stopped by it or by an error
  keeps what had arrived of its reply, marked as stopped.
- The flags a session names are saved with the chat, and the daemon reads
  them on every turn, the chat page's included: the model, `--tools`,
  `--no-tools`, the tool limits, `--max-tool-iterations`, `--profile` and five
  sampling flags. `--continue` keeps what the chat saved, with what its
  command line names in its place; `-s` replaces the chat's system prompt.
- `/retry`, `/edit` and `/branch` are made by the daemon, as the chat page
  makes them: a change it would make in place while a reply is still being
  written, as one just stopped by Ctrl+C may be, it makes on a new branch.
- A model this machine does not have is refused before a chat is made.
- A chat the daemon is still replying to, for the chat page or a paired
  device, is not continued: `--continue` says the chat is running elsewhere
  and names the run, before it stores anything. Wait for the reply, or stop
  it with `gglib run cancel <run>`.

### The folder it reads

The file tools, `read_file`, `list_directory` and `grep_search`, read a new
chat's folder: the directory `chat` was launched in, read-only, and nothing
outside it. The session says so once, on stderr: `Files: <dir> (read-only)`.

- A new chat's turns say that folder, as the chat page's do, and the chat
  remembers it.
- `--continue` keeps the folder the chat remembers while it is still there,
  the one the chat page chose included, wherever it is run, and the line
  names it. A chat that remembers none, or one since deleted, takes the
  directory it is continued in, as a new chat does.
- A new chat launched in your home folder or in `/` reads no files, and says
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

- The choice goes on each of the session's turns, and the daemon remembers
  it on the chat. `off` runs with a thinking budget of `0`; `on` with the
  model's own.
- Without it a resumed chat runs as it remembers.

[The two reasoning controls](../sampling.md#the-two-reasoning-controls)
explains the budget.

### Guardrails for tool calls

Local models need guardrails to finish a tool-calling task, so the loop
carries them.

- `--max-tool-iterations` caps the loop; without it, the limit a resumed chat
  saved, then the `max-tool-iterations` setting, which is the order the chat
  page and a paired device's turn follow too.
- `--tools` is an allowlist, saved with the chat; parallelism and timeout
  caps bound each step.
- The endpoint's loop guard stops a turn that repeats itself, as it does
  for every client.

### Sampling

`chat` and `q` take five sampling flags, `--temperature`, `--top-p`,
`--top-k`, `--max-tokens` and `--repeat-penalty`, and `--profile`, and the
chat keeps them. The other sampling flags are `serve`'s and `proxy`'s.

- Each turn sends the five as a client's sampling. The endpoint takes them
  only while **Let clients set sampling** is on (`gglib config set
  trust-client-sampling true`), `--max-tokens` whatever it is; while it is
  off, a session that names one of the other four says so once, on stderr,
  before its first turn.
- Each turn names the chat's profile on its model, `<id>:<profile>`, and the
  endpoint applies it whether or not it trusts clients' sampling. Under
  `--remote` the profile is the paired machine's.
- There is no `--reasoning-budget-tokens` here: `--thinking off` stops a
  chat's thinking.

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
- The choice is said on each turn that draws, and is not saved with the
  chat; the chat model is neither told of it nor able to make it.

## `run`

A reply the daemon finishes on its own. `start` prints the run's id and
returns; `show` prints the reply so far; `list` shows the daemon's runs,
newest first; `cancel` stops one.

<!-- snippet: run-start -->
```console
$ gglib run start -m Qwen3-4B-Instruct-2507 "Summarise the builder pattern in two sentences"
run-4cdda1b431fe
  Started. Read it with `gglib run show run-4cdda1b431fe`.
```
<!-- /snippet -->

<!-- snippet: run-show -->
```console
$ gglib run show run-4cdda1b431fe
The Builder pattern is a creational design pattern that separates the construction of a complex object from its representation, allowing the same construction process to create different representations. It is particularly useful when dealing with objects that have many optional components or when you need to create objects with complex configurations.
  run run-4cdda1b431fe completed
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
