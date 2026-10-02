# ADR 0014 — A model is named by its machine: an id on this machine or the paired one, carried as structure and resolved once

- **Status:** Accepted
- **Date:** 2026-10-03
- **Depends on:** [ADR 0012](0012-the-remote-tunnel.md),
  [ADR 0013](0013-the-target-is-a-value.md)
- **Supersedes:** in [ADR 0013](0013-the-target-is-a-value.md), decision 2's
  placement of every which-machine question in `crates/gglib-cli/src/target.rs`
  alone, decision 3's model rows of the reach table, and decision 4's
  statement that the remembered model is a name in that machine's
  catalogue — and nothing else in it
- **Superseded by:** nothing
- **Log:** none

## Context

ADR 0012 put one machine's proxy on another and ADR 0013 made the flag that
reaches it a value. A model was still what it was before there were two
machines: a row id in one SQLite file, or a name that file does not keep
unique. With a second machine, `3` is ambiguous, and what a person ran into
was that ambiguity surfacing in seven places.

- **The far list gives no id to type.** `gglib model list --remote`
  (`crates/gglib-cli/src/handlers/model/list_far.rs`) prints the NAME and
  CONTEXT columns of the far proxy's `/v1/models`, whose `ModelInfo`
  (`crates/gglib-proxy/src/models.rs`) carries the name as `id` and no
  catalogue id at all. Its footer says `gglib chat --remote <name>`.
- **An id is refused by a pinned far proxy before it is looked up.**
  `docs/remote.md` and the `chat` help in `crates/gglib-cli/src/commands.rs`
  say the id form is local-only and comes back 404. On an unpinned far proxy
  it does not: the catalogue resolves through `get_by_identifier`
  (`crates/gglib-core/src/ports/model_repository.rs`), numeric id first. On a
  pinned one — a machine running `gglib serve qwen3` — `check_pinned` in
  `crates/gglib-runtime/src/process/residency/mod.rs` compares the raw request
  string with the pinned name before `resolve` runs, and
  `foreign_model_is_refused_before_catalog_lookup` in `manager_tests.rs` holds
  that order. `gglib chat 3 --remote` is refused as a foreign model when 3 is
  the pinned model.
- **What is admitted is keyed by what was typed.** In
  `crates/gglib-proxy/src/server.rs`, `resolve_route` strips a profile suffix
  and nothing more; the loop guard, the connection registry behind the
  dashboard and the forward all use the remaining string. A request for
  `"3"` and one for `"qwen3"` are two dashboard rows and two loop-guard
  histories for one model, and the sampling audit, which `slots_poller.rs`
  runs under the resolved name, never finds the intent of a request
  registered as `"3"` (`in_flight_sampling` in `connections.rs` filters by
  name). `models_endpoint.rs` matches the running model and the pin by name.
- **An id can be handed to the next model.** `models.id` is
  `INTEGER PRIMARY KEY` without `AUTOINCREMENT`
  (`crates/gglib-db/src/setup.rs`), so SQLite gives a new row the largest id
  plus one: remove the newest model and the next one added takes its id.
  `benchmark_runs.model_ids` is JSON with no foreign key, and nothing outside
  the file — another machine's remembered model — is told of a delete. An id
  that can be reassigned is not something to remember.
- **A resumed chat changes machine with the flag.** `ConversationSettings`
  (`crates/gglib-core/src/domain/chat.rs`) records `model_name` as typed and
  nothing about where it ran; `apply_saved_settings` in
  `crates/gglib-cli/src/handlers/agent_chat/resume_settings.rs` refills the
  identifier and this invocation's flag picks the machine. On the hub,
  `hub_model::choose` (`crates/gglib-axum/src/handlers/agent/hub_model.rs`)
  falls back to that name, or the name the last reply recorded, as a name on
  this machine. The web's far chat creates its conversation with
  `modelId: null` and no settings
  (`src/hooks/useGglibRuntime/useGglibRuntime.ts`).
- **The web has a text box where a list should be.** Remote chat in the web
  is a checkbox and a free-text "Model on that machine" field
  (`src/components/remote/JoinSection.tsx`), typed against a list the page
  never shows. `gglib web --remote` is refused because `target::reach` puts
  `Commands::Web` on the plain `Reach::Local` arm every command about this
  machine gets — the default, not a decision about whether the page should
  see the other machine.
- **The far machine is read in four places, and named by a fingerprint.** The
  "connected, and holding a key" read is written three times in the daemon —
  `far()` in `crates/gglib-axum/src/handlers/remote/chats.rs`, `resolve` in
  `crates/gglib-axum/src/handlers/agent/remote_upstream.rs` and
  `stopping_key` in `crates/gglib-app-services/src/remote/connect.rs` — and a
  fourth time in the CLI, `Target::far` in
  `crates/gglib-cli/src/target_remote.rs`. Only `stopping_key` checks that the
  stored key belongs to the connected machine; `join` keeps the two in
  agreement today, so the others are not wrong, but they rest on a caller
  elsewhere. Every banner and status line names the paired machine by its
  ticket fingerprint (`target_remote.rs`, `print_connection` in
  `crates/gglib-cli/src/handlers/remote/join.rs`).

As with ADR 0013's six `if remote` branches, none of these is a rule about
remoteness. They are what follows from a model having no name that says
which machine it is on.

## Decision

### 1. A model is (machine, id)

A model is a `ModelRef { machine, id }`, in
`crates/gglib-core/src/domain/machine.rs`. `Machine` is `Local` or
`Paired { fingerprint }`: this machine, or the one this machine is paired
with, identified by the ticket fingerprint of the stored pairing. There is
one pairing (ADR 0013, *Out of scope*), so there are two machines.

The fingerprint is the identity. It is compared and never shown. What a
person reads is the paired machine's host name, which its proxy reports and
this machine stores in `RemotePairing.name`: read once the connection is
installed, bounded and non-fatal, refreshed on every connect, and kept by a
re-pairing only when it is the same machine. A host name is untrusted input
and never identifies anything — two machines may share one — so both sides
reduce it to its first DNS label of `A`–`Z`, `a`–`z`, `0`–`9`, `_` and `-`,
up to 63 characters, and a machine with no usable name is shown as "the
paired machine".

This replaces ADR 0013 decision 2's placement of every which-machine
question in `crates/gglib-cli/src/target.rs`. `Target` stays: it is the flag's
value, chosen before any fingerprint is known. Which machine a *model* is on
is a core type, because the daemon, the web page and stored conversations
ask it too.

### 2. Ids are never reused

`models.id` is `INTEGER PRIMARY KEY AUTOINCREMENT`. A library made before
this is rebuilt once, on first open, at the end of `create_schema` after
every table exists: on one connection, with foreign keys off outside any
transaction, inside `BEGIN IMMEDIATE` with the shape checked again so a
second process opening the same file is a no-op, copying the columns the old
and new tables share, requiring no foreign-key violation that was not there
before, and seeding the sequence above the largest id anything still names —
`models`, `chat_conversations.model_id`, `benchmark_runs.model_ids` and
`default_model_id`. An upsert binds the existing row's id, so updating a
model consumes no id.

An identifier keeps today's rules on its machine: an all-digit string is an
id first, and a model named `3` stays shadowed by id 3, which is now stable.

### 3. A request is resolved once in the proxy, before anything compares or keys on it

The proxy resolves the requested model right after `resolve_route`, in
`server.rs` and `embeddings.rs`. Everything after that reads the result:
the loop guard keys by the resolved name; the pin compares ids, after
resolving and still before the queue, so a foreign model never waits behind
the pinned one; admission is asked for the id, so the runtime launches
exactly that model; and once admitted, the connection registry, the forward,
calibration, the dashboard, the SSE echo and the sampling audit all use the
name of the model that was admitted. A model the catalogue does not have is
`404 model_not_found`, pinned or not.

Two catalogue reads per request remain for a bare identifier: the proxy's
resolution, and the runtime's own when it builds a launch spec inside
admission. A `{model}:{profile}` request also pays `resolve_route`'s one or
two reads before them, so `qwen3:coding` costs at least three. Every place
after the resolution reads its answer rather than resolving again. The
admission queue's own keys are still names, so two same-named models on one
machine can still share a turn; that is
[#1228](https://github.com/mmogr/gglib/issues/1228), not this decision.

### 4. One table says what may be done to a model on the paired machine

`ModelAction` in `crates/gglib-core/src/domain/machine.rs` is `List`,
`Detail`, `Chat`, `Load` and `Manage`, and `ModelAction::on_paired` is one
exhaustive match: the first four are allowed on the paired machine, `Manage`
— adding, removing, editing, tagging, verifying, benchmarking, setting a
default, anything that changes this machine's library — is not. It is
ADR 0013 decision 3's line, *use the machine, don't change it*, stated once
for models. `Load` is on the use side because it is what `serve --remote`
already does.

Every surface reads it rather than restating it. The CLI's `reach` derives
its `model`, `chat`, `q` and `serve` rows from it, which adds
`model inspect --remote`. The daemon sends the paired machine's row as data
with the far list. The web page shows a far model's action only when that
row lists it, and keeps no copy of the table. It is deliberately not called
a capability: `ModelCapabilities` already means something else.

This replaces ADR 0013 decision 3's model rows of the reach table. The rest
of that table, and the line it draws, stand.

### 5. The machine travels as structure, never as text

There is no `desk/3` or `machine:id` form, and no parser for one. `/` and
`:` are legal in model names — `profile_route` already gives `:` a meaning,
and splits on the last one so a name may hold more — and so are all-digit
names, so any separator either breaks names that exist or needs a rule
about which reading wins. Every input already carries the machine out of
band: `--remote` on the command line, the `/api/remote/...` prefix over
HTTP, a structured `ModelRef` in JSON. No proxy parses a machine, so a
request cannot be passed on through a second pairing: a daisy chain is
impossible by construction, not by a check.

**A bare id always means this machine and is never forwarded.** `gglib chat
3` with no model 3 here does not try the paired machine; it says that the
paired machine's models need `--remote`, and names `gglib model list
--remote`. Silently trying another machine is the ambiguity this ADR
removes.

What is shown is always something a person can type: an ID column under a
header that names the machine, the banner `qwen3 (3) on desk`, a machine
badge in the web, and the exact command — `gglib chat 3 --remote` — beside
a far model in the web.

### 6. The far model list carries ids, and a far model can be read

The proxy's `/v1/models` gives each entry `gglib_id`, the catalogue id on
that machine (a `{model}:{profile}` entry shares its base's and carries
`profile`), and the response `machine_name`, the sanitised host name read
per request. The OpenAI `id` stays the name, because an OpenAI client sends
`id` back as `model` and shows it in a picker. `machine_name` sits behind the
same bearer and device gate as the list and never on `/health`. The running
model and the pin are matched by id.

`GET /v1/models/{name}/detail` returns `ModelLookup { profile, detail }` for
any identifier the chat route accepts, profile suffix included, with the
file path and port removed and whether it is serving read from the
admission snapshot by id. It is on the use side: it is in `TUNNEL_REACHABLE`
(`crates/gglib-proxy/src/router_tests.rs`) behind the same gate as
`/v1/models`. The daemon reaches both through one client, built by one
"connected, and this key is for that machine" check, and the CLI and the web
read the paired machine's models only through the daemon's
`/api/remote/models` routes.

### 7. A conversation remembers its machine, and a stale pairing refuses

`ConversationSettings.model` is an optional `ModelRef`. The CLI writes it on
every turn; the web writes it when it creates the conversation. A
conversation's machine is fixed for its life.

- A resumed conversation follows its stored machine, and the banner says
  so. A paired ref whose fingerprint is not the stored pairing's is refused,
  because that machine's id 3 is not this pairing's id 3. A local ref with
  `--remote` is refused. With no stored ref, the flag decides, as before.
- On the hub, `hub_model::choose` admits a local ref by id and refuses a
  paired one with 409: a turn there runs on this machine's models.
- A web chat request names a far model as a `ModelRef`; a local ref there is
  400, a stale fingerprint 409.

`RemotePairing.default_model` keeps its key and its type and now holds
`<id>` or `<id>:<profile>`. It still lives and dies with the pairing. This
replaces ADR 0013 decision 4's statement that the remembered model is a name
in that machine's catalogue; remembering per pairing, and remembering what
was last asked for rather than what last worked, stand.

### 8. One web library shows both machines

Plain `gglib web` shows this machine's models and, after them, a group
headed by the paired machine's name. The page runs on this machine. A far
row is read-only and offers only the actions decision 4's row lists; a chat
opened from it is a conversation stored here whose model is on the paired
machine. A far row is a separate type — the far list's `ModelInfo`, never
the local `GgufModel` — so the surfaces that are about this machine
(benchmarks, the default-model setting, downloads, tags, MCP, setup, the
native menu) cannot be handed one, and the compiler says so. A far fetch
never blocks or blanks the local rows; while the paired machine is away its
rows are kept and marked stale.

The free-text model field and its checkbox are deleted. `gglib web
--remote` stays refused, with a sentence saying plain `gglib web` already
shows the paired machine's models.

### 9. A far build without ids is refused, not shimmed

A paired machine whose `/v1/models` carries no `gglib_id`, or whose
`/detail` answers a bare 404 with no error code, is an older gglib, and the
daemon answers 409 with a sentence that names it and says to update it.
There is no fallback to names: a fallback is the ambiguity this ADR exists
to remove, kept alive for as long as anyone runs an old build. An older
machine pointed at a newer one keeps working, because the fields are
additive.

## Consequences

**What a person notices:**

- `gglib model list --remote` has ID and PROFILES columns under a header
  that names the machine, and a footer that says `gglib chat <id> --remote`.
  `gglib chat 3 --remote` answers from a pinned machine.
- `gglib model list` ends with one line about the paired machine — paired
  and how, not connected, or away — taken from what the daemon already
  knows. It makes no far request.
- Banners read `qwen3 (3) on desk`; `gglib remote join` and
  `gglib remote status` name the machine. No fingerprint is printed or
  rendered.
- The web library shows the paired machine's models beside this one's.
- On the paired machine, a model's dashboard row, loop-guard history and
  calibration are one row whether it was asked for by id or by name.
  Readings that used to be split across `"3"` and `"qwen3"` merge, which
  includes the loop-guard trips the A/B eval's proxy arm reads.

**Costs, accepted:**

- **Both machines upgrade together.** A newer machine refuses an older far
  one with "update it", by decision 9.
- **A far database reset restarts its ids.** A reinstall or a new data
  directory on the paired machine numbers its models from 1 again, and
  nothing here can tell. A remembered `<id>` or a conversation's ref then
  names whatever took that id. It is visible rather than silent because
  every banner and history line shows `name (id)`.
- **The loop guard runs after a catalogue read.** A request it refuses now
  costs one database read it did not before, in exchange for refusing the
  model it would have run rather than the string it was sent.
- **`/v1/models/{name}/detail` publishes a model's GGUF metadata** to every
  holder of that proxy's key, the same audience as `/v1/models`. The path
  and port are not in it.
- **`gglib remote join` waits up to 3 s longer** when the far side is slow to
  report its name. A missed name is read again at the next connect.
- **Same-named models on one machine** still share an admission queue turn
  ([#1228](https://github.com/mmogr/gglib/issues/1228)) and, since everything
  after admission uses the resolved name, a dashboard row.

## Kill criteria

- If the first open of a real library loses anything, the rebuild in
  decision 2 is withdrawn rather than patched, and ids stay reusable on
  libraries made before it until a rebuild is proven. The reading is taken on
  a copy of `gglib.db`, opened with `GGLIB_DATA_DIR` pointed at a
  directory whose `data/gglib.db` is that copy: row counts of `models`,
  `model_files`, `model_benchmark_summaries` and `count(model_id)` of
  `chat_conversations`, and `PRAGMA foreign_key_check`, before and after any
  `gglib model list` against the copy. Any count that differs, or any violation that was not there before,
  reverses it. After release the reading is the issue tracker:
  `gh issue list --label "component: db"`, where any open issue reporting a
  model, file, benchmark summary or conversation lost or re-pointed on the
  first open after upgrading reverses it.
- If gglib stores a second pairing, decision 5 is reopened rather than
  extended. `Settings.remote_pairing` (`crates/gglib-core/src/settings.rs`)
  is one `Option<RemotePairing>`, and `gglib remote status` prints one
  `Connected:` line. The day either holds two, `--remote` and
  `/api/remote` no longer name one machine, and the question decision 5
  answers for two machines is open again.
- If the far list cannot be read within its bound over a relayed
  connection, decision 8's far group stops loading with the library and is
  fetched when opened instead. The reading is `time gglib model list
  --remote` on the connecting machine while `gglib remote status` shows
  `Connected: … (relayed)`; the daemon bounds the far list read at 3 s, and
  the web marks far rows stale when it misses. Three of five consecutive
  runs over 3 s reverses it. Plain `gglib model list` is not the reading: it
  makes no far request.

## Out of scope

- **Named remotes** (`--remote <name>`). One pairing is stored, as
  ADR 0013 says; the second kill criterion above is what would change that.
- **A text form** for a model on another machine (`desk/3`, a UUID).
  Declined by decision 5. If one is ever wanted, it is command-line input
  only, and nothing on the wire changes.
- **Far profiles in the web.** The web lists a far machine's base models;
  `gglib chat qwen3:coding --remote` is how a far profile is chosen.
- **The far proxy's dashboard and cache-clear from the web.** The CLI
  reaches them (ADR 0013 decision 3); the web reaches the far machine's
  chats, runs and models and can stop its daemon, but not the far proxy's
  dashboard or cache-clear.
- **Changing a far model.** `Manage` is never allowed on the paired machine.
- **Admission keyed by id.** Same-named models on one machine:
  [#1228](https://github.com/mmogr/gglib/issues/1228).
- **Retiring the runtime's `u32` model ids** (`RunningTarget`, the admission
  types). They are the same ids, narrowed, and nothing here needs them wider.
- **Far model events.** The far list is read when the connection comes up or
  comes back and when the page is focused; the paired machine's model events
  are not relayed.
