# ADR 0023 — A chat's folder is where gglib's own file tools read: the directory `gglib chat` was launched in, the folder the page chose, and none for a paired device

- **Status:** Accepted
- **Date:** 2026-10-23
- **Depends on:** [ADR 0008](0008-two-binaries-one-daemon.md),
  [ADR 0012](0012-the-remote-tunnel.md),
  [ADR 0017](0017-history-is-never-rewritten.md),
  [ADR 0018](0018-gglib-draws-with-stable-diffusion-cpp.md)
- **Supersedes:** nothing
- **Superseded by:** nothing
- **Log:** [log-0023](log-0023.md)

> Line numbers under "The report" and "What existed" are at `843047f5`, the
> `main` the arc started from. Every other line number is at `508bb2d1`, the
> commit `feat(chat): a chat has a folder: the CLI's is where it ran, the
> page's is chosen, a device's is none`, which ends the arc's line of
> changes. The two changes made on `main` by their own commits, beside that
> line, are named by their subjects.

## Context

### The report

One user, one run, as reported. The GUI's "Filesystem Access" template saved
the arguments `["-y", "@modelcontextprotocol/server-filesystem",
"/path/to/allowed"]` (`src/components/AddMcpServerModal/ServerTemplatePicker.tsx:27-34`), and `validate_server` (`crates/gglib-mcp/src/service.rs:188`)
checks a server's command and working directory, never its arguments, so the
row saved as valid. The first `gglib chat` prewarmed the lazy server
(`crates/gglib-cli/src/handlers/agent_chat/config.rs:177`).
`@modelcontextprotocol/server-filesystem` 2.0.0, which `npx -y` installs, exits
while it resolves its arguments, with `Error: None of the specified
directories are accessible`, before any handshake. gglib piped the child's
stderr and never read it (`crates/gglib-mcp/src/client.rs:217`), so the one
trace was `Server closed connection` (`client.rs:464`) as a `tracing::warn`,
and the chat went on with no file tools. The reproduction of the server's exit
was run by hand on the owner's machine and is not in this repository.

The user expected the model to see "the folder we are in".

### What existed

Before the arc gglib already shipped three sandboxed, read-only, in-process
file tools, `builtin:read_file`, `builtin:list_directory` and
`builtin:grep_search` (`crates/gglib-mcp/src/builtin/`), offered only to a
loop composed with a sandbox root. Only `gglib q` gave one, its working
directory (`crates/gglib-cli/src/handlers/inference/agent_question.rs:98`);
`gglib chat` passed none (`agent_chat/mod.rs:91`), and the daemon composed
every run with none (`crates/gglib-runtime/src/compose.rs:143`). `docs/cli.md`
promised `chat` "filesystem plus any configured MCP servers", true only of
`q`. The page's Tools popover listed the three tools
(`GET /api/builtin/tools`), and every call of one in a daemon run was refused
with "filesystem tools require a sandbox root" (`builtin/mod.rs:299`). No
description named a folder, so a model that had one did not know which.

The read before the arc also found four faults in the tools themselves: an
absolute path inside the root was rebased under it and reported missing,
`grep_search` followed a symlink out of the root, `read_file` could panic
cutting a long file inside a character, and the tool timeout could not end a
walk, which ran inline on the async executor ([log-0023](log-0023.md), first
entry).

### Facts the decision rests on

- **An MCP server is per OS process, not per chat.** The daemon builds one
  MCP service (`crates/gglib-axum/src/bootstrap.rs`), each CLI invocation
  builds its own, and within a process one child per server id serves every
  chat (`crates/gglib-mcp/src/manager.rs`). A server cannot hold two chats'
  folders at once.
- **MCP Roots do not fit.** server-filesystem 2.0.0 takes roots from a client
  that declares them, and roots *replace* its command-line list, in one
  process-wide allow-list; a command-line directory that does not exist kills
  it before roots are read, so roots would not have saved the reported run.
  The 2026-07-28 MCP specification deprecates Roots and calls them
  informational guidance rather than access control. gglib's client also
  reads a server's request only while a request of its own is in flight
  (`client.rs`), so a re-query after `list_changed` would sit unread.
- **The daemon's working directory is nobody's folder.** It is whatever
  process first started the daemon, fixed for its life; no spawn path sets
  one (ADR 0008's daemon is started detached by either client).
- **Under `gglib chat --remote` the loop runs on the laptop.** The MCP
  servers and the builtins run there; tool definitions, calls and results
  cross the tunnel as ordinary chat-completion payload, and modelpipe forwards
  that HTTP with no notion of a tool or a folder.
- **A chat's settings are one JSON column copied on branch.**
  `ConversationSettings` has no `deny_unknown_fields`, and a branch copies the
  column verbatim, so a new optional key needs no migration and survives a
  branch ([ADR 0017](0017-history-is-never-rewritten.md)). The Thinking
  choice is the precedent for a per-conversation value said on the run that
  changes it and remembered by the daemon.

### What is unmeasured

No reading exists for "a small model does better when told its folder". The
agentic eval's tools return a canned `{"status":"ok"}` and touch no disk, and
its arms vary the request pipeline, not a task's tools or prompt. The
cheapest reading there is has **not been taken**: two custom task suites,
identical but for whether the file tools' descriptions name a folder, each run
on one model with the default three seeds:

```
gglib benchmark agentic --model <model> --task-suite <folder-named.json>
gglib benchmark agentic --model <model> --task-suite <folder-unnamed.json>
```

Its scope, stated before it exists: it measures whether the model writes
plausible paths into calls of tools named like the builtins, scored against
hand-written `required_args`, with no filesystem behind them; each run's A/A
drift is its own, and the eval compares nothing across two runs. The two suite
files are not written; the shape to copy is
`crates/gglib-core/assets/tune_default_suite.json`, whose tasks carry their
own `tools` and descriptions. It needs a model on the owner's machine, and it
is a follow-up in the log.

## Decision

### 1. The folder is a property of the chat run, and reaches only gglib's own three file tools

`compose_agent_loop` takes the run's folder as `sandbox_root`
(`crates/gglib-runtime/src/compose.rs:131`), and the three builtin file tools
read under it as their per-run root, and nothing else does. External MCP
servers learn nothing: no roots, no `{cwd}` in stored arguments, no server
per chat. They keep their static arguments and working directory, shared by
every chat in the process that started them, and `docs/cli.md` says so.

### 2. `gglib chat`'s folder is the directory it was launched in

Every invocation, `--continue` included, reads the directory it runs in
(`crates/gglib-cli/src/handlers/agent_chat/files.rs:20`) through
`folder::launched` (`crates/gglib-core/src/domain/folder.rs:33`), which
answers the canonical path, and none for this machine's home folder or `/`.
The session says so once on stderr, unless quiet:

- `Files: <dir> (read-only)`;
- `Files: off in your home folder or /; cd into a project to read it, or
  --no-tools` when it was launched in either;
- `Files: off; the folder this ran in cannot be read (<reason>). cd into a
  project to read it, or --no-tools` when the directory was deleted since the
  shell entered it; the session starts without file tools.

A `--tools` list is honoured as typed: the session has a folder only when
its list admits a file tool, every tool (no list) or a list naming one, bare
or as `builtin:<name>` (`files.rs:62`). A list that names none, `--no-tools`
included, gets no folder, no line, and its directory is not read. The CLI
never reads or writes `ConversationSettings.folder`, so a folder the page
chose for the same chat survives a `--continue` unread. `gglib q` is
unchanged: it keeps its working directory as the root, its own prompt line and
no home guard.

### 3. The page's folder is the person's explicit pick, and the daemon remembers it

A run says `folder` beside `thinking` (`AgentRunRequest.folder`,
`crates/gglib-axum/src/handlers/agent/dto.rs:316`): a path sets it, `""`
forgets it, and absent runs on what the conversation remembers. Without a
`conversation_id` a path is that run's alone and `""` is the same as absent.
`settle_folder` (`run.rs:233`) settles it before a slot is taken, so a
refusal writes nothing:

- `""` is never checked, so a folder that is gone can always be cleared;
- a said path goes through `folder::chosen` (`folder.rs:51`: absolute,
  exists, a folder) and is refused **400 `folder_unusable`** when it fails, or
  when its canonical form is not UTF-8 and a conversation would have to store
  it;
- a remembered folder is checked again and refused **409 `folder_missing`**
  when it no longer resolves, whatever the run's tool filter; only saying
  another path or `""` gets past it.

The canonical path is what the run reads, what the conversation remembers
(`record_folder`, `crates/gglib-core/src/services/chat_history.rs:129`, called
from `launch.rs:186`) in `ConversationSettings.folder`
(`crates/gglib-core/src/domain/chat.rs:332`), and what the page shows: the
folder shown is the folder read. A branch copies it with the other settings.

### 4. A paired device's turn never has a folder

A device's turn is composed by `held_for_device` (`hub_turn.rs:201`) with no
folder, whatever its chat remembers, and `POST /api/agent/chat` composes with
none. `HubTurn` gains no key, so a device cannot name a path, and
`hub_chats::open()` strips `settings.folder` from the conversation a device
reads (`crates/gglib-app-services/src/hub_chats.rs:98`), since it is a path on
this machine. [ADR 0012, decision 5](0012-the-remote-tunnel.md#5-mcp-is-refused-over-the-tunnel-unless-asked-for)
stays exactly as closed as it was: `--allow-mcp` names no file tool.

### 5. Choosing the folder is the switch

The state "a file tool is on and no folder is chosen" does not exist, so no
send is blocked for it. In the daemon `with_folder` (`compose.rs:151`) adds the
three qualified names to a tool filter that names tools when the run has a
folder; `[]`, a model that calls no tool, stays `[]`, and a run with no
filter is offered them already. It runs before `with_drawing` (`compose.rs:237`), so a `[]` sent
with Draw does not become a list that gains them. The page's tool list
(`bare_definitions`, `crates/gglib-mcp/src/builtin/mod.rs:147`) leaves out a
tool that needs a root, as it leaves out the one that needs a drawing tool
([ADR 0018](0018-gglib-draws-with-stable-diffusion-cpp.md)), so the Tools popover no longer offers three tools it cannot use. The three
names have one source, `FILE_TOOLS` (`builtin/mod.rs:33`), which the builtin
table, the CLI's `--tools` check and the daemon's filter read.

On the page the folder is a chip beside Thinking (`FolderChip`,
`src/components/ChatMessagesPanel/components/FolderChip.tsx`, driven by
`src/hooks/useChatFolder.ts`): its name, the full path on hover, Change and
Clear, and the 400 or 409 as its error state, its title the reason. A chat
with a folder counts as having tools configured. The chip is not offered to a
model that calls no tools, unless its chat already remembers a folder
(`useChatFolder.ts:160`), so a folder that is gone can still be cleared.

### 6. The model is told its folder in the three tools' own descriptions, per run, and never in a prompt

Each file tool's description, built when the tools are listed and never
stored, ends ``The working folder is `<root>`; a path is relative to it.``
(`builtin/mod.rs:210`), and without a root ``A path is relative to the working
folder.`` (`:196`). No system prompt is edited on any surface: the page's is
the person's own, and the CLI's is stored at session start, where a path
would go stale on a `--continue` from another directory or on a branch.

An absolute path that already lies under the root is taken as is; any other
absolute path is read as relative to it; every path is canonicalised and
refused unless it stays under the canonical root. A refusal names the path as
the model can use it again and never says "sandbox": a path that names
nothing ends `in the working folder. Call list_directory to see what is
there.` (`MISSING`), and one that leaves the root ends `is outside the
working folder` (`OUTSIDE`, `crates/gglib-mcp/src/builtin/sandboxing.rs:18,
23`). The model-facing text is short and fixed, as the loop guard's sentence
is ([ADR 0021](0021-the-loop-guard-ends-the-loop.md)).

### 7. The sandbox is hardened before it reaches more surfaces than `gglib q`

`grep_search`'s walk (`fs_grep.rs:152`) reads each entry's own type, follows
a symlink only when its canonical target lies under the canonical root, and
never enters a folder it is already inside. `read_file` cuts a long file
between characters (`fs_read.rs:38`). The three tools run under
`spawn_blocking` (`builtin/mod.rs:361`), and a guard dropped with a call the
tool timeout gave up on (`StopOnDrop`, `:387`) sets a flag the walk checks
before each entry (`fs_grep.rs:111`). There is no walk budget: no reading of
a cut-short search exists (kill criterion 5).

### 8. The "Filesystem Access" template is removed

Made on `main` by its own commit, `fix(gui): drop the Filesystem Access
template`, beside the arc's line. The picker says instead: `Reading files is built in: choose a folder in a chat.
Every template here needs an API key.` No template metadata, no `{folder}`
token, no home-folder default.

### 9. A server that fails to start says what it printed

Made on `main` by its own commit, `fix(mcp): a server that fails to start
says what it printed, and the CLI shows it`, beside the arc's line. The MCP
client reads a server's stderr to its
end into a 4 KiB tail (`crates/gglib-mcp/src/stderr_tail.rs` in that change): in a task, and
on a thread of its own on Windows. A closed connection ends ` - stderr:
<tail>`, so the start's error, and with it the Test button's result, carries
the server's own words. `prewarm_lazy` returns each lazy server it could not start
or found invalid, and `gglib chat` and `gglib q` print one stderr line each
unless `-Q`. No argument heuristic and no dry-run route.

### 10. `--remote` changes nothing in the mechanism

The folder is on the machine that runs the loop: the laptop's launch
directory for `gglib chat --remote`, the laptop's chosen folder for a page's
chat on a paired model. The descriptions with the path, the calls and the
file contents cross the tunnel as chat payload to the desktop's model and
logs, the same owner's by ADR 0012. The CLI's line says so, `Files: <dir> on
this machine (read-only; what is read is sent to <machine>)` (`files.rs:80`),
and so does the chip's title (`FolderChip.tsx:12`). modelpipe is not changed.

### 11. One rule, written once

`crates/gglib-core/src/domain/folder.rs` holds `launched` and `chosen` and
`FolderError`: two functions, no newtype. The CLI and the daemon call them;
neither re-states the rule.

### 12. Nothing new to configure

No setting, flag, route, Tauri command, table, migration, process or protocol
feature. The desktop's picker is the dialog plugin gglib already registers,
`open({ directory: true })` (`pickFolder`, `src/services/platform/fileDialogs.ts:63`);
the web build takes a typed absolute path, which the daemon checks.

### Where it lives

`gglib-core`'s `domain::folder` (the rule) and `ConversationSettings.folder`;
`gglib-mcp`'s `builtin` (the tools, their descriptions and their sandbox) and
`stderr_tail`; `gglib-runtime`'s `compose_agent_loop`; `gglib-axum`'s
`handlers/agent` (`run`, `launch`, `compose`, `hub_turn`);
`gglib-app-services`' `hub_chats`; `gglib-cli`'s `agent_chat/files.rs`; and
on the page `useChatFolder`, `FolderChip` and `pickFolder`.

## Consequences

What a person will notice:

- `gglib chat` in a project offers three read-only file tools and prints
  `Files: <dir> (read-only)` once; from home or `/` it prints the refusal
  line. `docs/cli.md`'s promise of filesystem tools is true of `chat`.
- A page's chat gains a folder chip beside Thinking. Choosing a folder is
  what turns the file tools on; the Tools popover no longer lists them.
- A server that cannot start says what it printed, in Test and on the CLI's
  stderr, instead of `Server closed connection`.
- The Filesystem Access template is gone. A hand-added filesystem server
  still works with a real path, shared by every chat in that process.

Costs, stated plainly:

- A `gglib chat` session carries the three tools' definitions, a few hundred
  tokens, and a small model may make file calls a chat does not need.
- Under `--remote` the laptop's path and the contents of files read go to the
  desktop's model and logs. Same owner; said, not prevented.
- A typed `--tools` list that names no file tool gets none, and no `Files:`
  line.
- Launching from an ancestor of home, such as `/home`, is not refused.
- An explicit pick of the home folder on the page exposes all of it,
  read-only, to that chat, and a search there can hit the tool timeout; the
  stop flag ends the walk afterwards.
- Hidden files inside the folder are readable by name; there is no deny list.
- A hand-added filesystem server beside the builtins gives the model two
  `read_file`s, and a bare `--tools read_file` passes both.
- After a run accepts a typed or picked path, the chip shows it as said until
  the page next reads the conversation and shows the canonical path.
- On Windows the recorded folder is a verbatim `\\?\C:\...` path, which the
  chip, the CLI's line and the descriptions show.
- Downgrade: an older binary's write of a chat's settings drops `folder`, and
  the chip asks again; an older daemon ignores `folder`, so the chat silently
  gets no file tools.
- A folder recorded in `ConversationSettings` is a host path in the database.
  It is stripped from what a device reads, and no route lets a device set it.

Wire and contract changes: `AgentRunRequest.folder` and
`ConversationSettings.folder` (both optional, no migration); the error codes
`folder_unusable` (400) and `folder_missing` (409); `GET /api/builtin/tools`
lists no file tool; `compose_agent_loop` and the daemon's `prepare_held`,
`held_over` and `prepare_over` take the folder; `HubChatOpen` omits
`settings.folder`. No new route and no new query key.

## Kill criteria

Each is a report over the owner's own database, not the A/B ruler,
and says so. An assistant row that called tools holds
`metadata.tool_calls[].{id,name,arguments}` and its model in
`metadata.modelName`; a tool row's `content` is the result as the model read
it (`crates/gglib-core/src/domain/agent/transcript.rs`,
`crates/gglib-db/src/setup.rs:271`). A zero is recorded with its denominator,
its model and its build.

1. **The folder in the descriptions is not enough.** Calls since the change
   merged:

   ```sql
   SELECT count(*) AS calls
   FROM chat_messages m, json_each(json_extract(m.metadata, '$.tool_calls')) tc
   WHERE m.role = 'assistant' AND m.created_at >= :merged
     AND json_extract(tc.value, '$.name')
         IN ('builtin:read_file', 'builtin:list_directory', 'builtin:grep_search');
   ```

   and refusals, the two phrases of decision 6:

   ```sql
   SELECT count(*) AS refused
   FROM chat_messages
   WHERE role = 'tool' AND created_at >= :merged
     AND (content LIKE '%in the working folder. Call list_directory%'
          OR content LIKE '%is outside the working folder%');
   ```

   Counted independently, with no join on `tool_call_id` (small models reuse
   `call_0`), per model. `refused / calls` above 1 in 4 over at least 100
   calls on one model of 9B parameters or fewer moves the folder into a
   non-persisted note at the front of the request, in the request pipeline as
   `drawn_note` is, rather than a stored prompt line. Second reading: of chats
   whose `json_extract(chat_conversations.settings, '$.folder')` is not null,
   more than half of at least 30 with no file-tool call means the model is not
   noticing the tools, with the same next step. `calls = 0` after 30 days is
   recorded as "unused", not as a pass.
2. **The builtins do not replace the template.** `SELECT count(*),
   max(created_at) FROM mcp_servers WHERE args LIKE '%server-filesystem%'`,
   taken when the template's removal merged and 30 days later. New rows mean
   demand for writes or for several folders, and open a server per chat for
   that one server as a new decision, with the folder as a plain argument; not
   the template's return.
3. **The home guard is in the way.** A report, not a counter. Two reports that
   the refusal blocked a legitimate `gglib chat` from home add an explicit
   `--folder <dir>`, unguarded. The guard stays.
4. **Containment.** `cargo test -p gglib-mcp builtin`, plus one reproduction,
   filed or found, of a file tool returning bytes from a path whose canonical
   form is outside the run's folder. One reproduction sends the CLI's and the
   daemon's root back to none until it is fixed.
5. **The walk needs a budget.** Tool rows whose `content LIKE 'Tool
   ''builtin:grep_search'' timed out after%'` (the one producer is
   `crates/gglib-agent/src/tool_execution/mod.rs:92`) over calls of
   `builtin:grep_search` in the same window. More than 1 in 10 over at least
   30 calls adds a function-local budget beside `MAX_MATCHES` in `fs_grep.rs`
   with a result line that asks for a narrower path. With the stop flag, this
   counts stalls a person saw, not threads the pool absorbed.

## Out of scope

- **Write, edit, move or delete tools.** gglib has no tool-confirmation
  story. Kill criterion 2 is what would open it.
- **External servers per chat.** Only if kill criterion 2 reads non-zero
  after the template is gone, and then as a new decision for that one server.
- **A `--folder <dir>` flag.** `cd` is the flag and `--no-tools` the off
  switch, until kill criterion 3. No `--no-folder` either: `--tools` is
  honoured as typed.
- **A deny list, or several folders per chat.**
- **A folder for a device's turn, for `/mcp` gateway sessions, or on
  `HubTurn`.** ADR 0012 keeps that grant closed; opening it is a new decision.
- **The host OS or the date in a prompt.** A field nobody named is not sent
  ([ADR 0022](0022-the-proxy-does-less.md)).
- **`gglib q`'s prompt line and its unguarded folder.**
- **An eval arm for the folder.** It needs a real-filesystem executor over a
  fixture directory, a tool-set axis per arm and per-call outcome fields; its
  own arc.
- **The daemon prewarming lazy servers, the page's server registry resyncing
  after an add, the add form splitting arguments on spaces, and
  `update_server` not restarting a server.** Real, older than this arc, and
  each its own change.
- **Any modelpipe change.**
