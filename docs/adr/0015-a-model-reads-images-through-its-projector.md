# ADR 0015 — A model reads images through its projector: an image is counted by its pixels, refused by name, and stored once under the hash of its bytes

- **Status:** Accepted
- **Date:** 2026-10-04
- **Depends on:** [ADR 0001](0001-runtime-capability-tiers.md),
  [ADR 0012](0012-the-remote-tunnel.md),
  [ADR 0014](0014-a-model-is-named-by-its-machine.md)
- **Supersedes:** nothing
- **Superseded by:** nothing
- **Log:** [log-0015](log-0015.md)

## Context

A client pastes a screenshot. GitHub Copilot sends it to an OpenAI endpoint as
an `image_url` part of a message's `content`, a `data:` URL of base64
(`docs/clients.md`, *Images*). llama-server can read it, given a second file;
gglib had no place for that file, and its proxy measured the image as text.

- **llama-server reads an image once it is given the projector.** Started
  with `--mmproj <projector>`, it read a 980x460 terminal screenshot and a
  2560x1440 screenshot back exactly, at 480 and 3,646 prompt tokens and 2.7 s
  and 24 s of prefill. `GET /props` answered `modalities` as
  `{vision: true, video: true, audio: false}` with the projector and all
  three `false` without it. A projector from one repository worked with
  another repository's GGUF of the same base model, generation stayed at
  about 15 tokens per second, and `--image-max-tokens 8192` changed nothing
  ([log-0015, 2026-10-03][poc]).
- **Without the projector an image request fails after the model is loaded,
  and not by name.** llama-server answered HTTP 500 "image input is not
  supported" ([log-0015, 2026-10-03][poc]). Forwarded by the proxy, such a
  request would load the model, evicting whatever is serving, to collect
  that 500 (`crates/gglib-proxy/src/image_refusal.rs`).
- **The library had no place for a projector.** A model row named its
  weights, and no second file to load beside them. `CapabilityFlags::VISION`
  and a `vision` tag existed, and nothing set them; #1244 removed both. A
  repository listing grouped every `.gguf` by the quantization in its name,
  so `mmproj-F16.gguf` was listed as an `F16` quantization and
  `X.mmproj-Q8_0.gguf` joined the real `Q8_0` group as a second shard. Files
  sort by name and the first is the primary one, so a projector could become
  the file a model is launched from ([log-0015, #1249's entry][download]).
- **An image measured as text is refused.** History truncation measured a
  request by its serialized length, so a 1.5 MB screenshot was refused as
  `context_length_exceeded` under any context a local model has, and the
  learned chars-per-token ratio would have been taught by requests whose
  prompt tokens are mostly an image's ([log-0015, #1245's entry][estimate]).
- **Inline bytes are the wrong form for gglib's own messages.** gglib
  measures a request by its length in more than one place, and a megabyte of
  base64 is not a megabyte of prompt. A chat's whole history is sent again
  every turn, so an inline image would be stored and carried once a turn. A
  chat opened on a paired device would pull every image through the tunnel
  to list its rows ([log-0015, #1264's entry][reference]).

**The scope of the evidence.** Every number about what an image costs comes
from one session on 2026-10-03: one machine, a 128 GB Mac on Metal; one
llama.cpp build, `cb7934c`, which is not gglib's pin (`PINNED_LLAMA_RELEASE`
in `crates/gglib-runtime/src/llama/download/mod.rs` is `b10327`, which
[ADR 0002](0002-defer-tool-call-constraint-to-llama-cpp.md) records as
`69bf643`); one model, Qwen3.8-27B at Q8_0, with a 629 MB projector; two
images. It was taken without gglib's proxy, and no script for it is in the
repository, so its numbers cannot be re-derived from a checkout; the
reproducer is the shape of the session, llama-server with `--mmproj` and a
chat completion whose user message carries an `image_url` part. No other
machine, backend, model family or quantization was read. No later entry of
the log reads an image against a model through gglib: #1245's says the
estimate was not read through the proxy, and #1264's and the chat page's
that no image reached a model.

## Decision

Seven decisions. All are Tier B in
[ADR 0001](0001-runtime-capability-tiers.md)'s terms: llama-server serves one
model, holds no catalogue and keeps no chat, so which model can see, what a
request will cost before it is admitted, and where an image is kept are
gglib's questions. None gates on `RuntimeCapabilities`. The one input that is
not gglib's own is decision 3's `--port` branch, which reads llama-server's
`GET /props` `modalities.vision`: a runtime self-report used as a policy
input, the posture [ADR 0007](0007-ask-the-server-for-template-capabilities.md)
names and ADR 0001's amendment places outside the tiers; a server that does
not say is sent the image.

### 1. Whether a model can see is one fact: `models.projector_path`

A model of this machine reads images exactly when it is linked to a
projector, the second GGUF llama-server loads with `--mmproj`. The link is
the column `models.projector_path` (`crates/gglib-db/src/setup_models.rs`),
and `Model::image_input` (`crates/gglib-core/src/domain/model.rs`) is that
column being set. No tag and no capability flag says it.

- **One header check links it.** `ModelService::set_projector`
  (`crates/gglib-core/src/services/model_projector.rs`) links a file only when
  its GGUF header says it is a projector (`general.type = mmproj`, or
  `general.architecture = clip` for one converted before that key), and
  stores its canonical path. `gglib model update <model> --projector <path>`
  and `--no-projector`, and the inspector's Projector row through
  `PUT /api/models/{id}`, call it, and a download that brings a projector is
  linked by the same header check (`link_downloaded_projector`); a model the
  library already holds with a link keeps that link when it is registered
  again. `gglib model upgrade` fetches a projector and does not link it
  (#1252). A library from before the column links each model, by file name
  and without reading a header, to the first projector among its own files
  that is on disk, once, when the column is added
  (`crates/gglib-db/src/setup_model_files.rs`). Several models may name one
  file.
- **A download fetches one and links it.** A download of quantization Q from
  a repository that has projectors fetches one of them as one more file of
  the model's group, after the weights: the one whose name carries Q, else
  the `F16` one, else the first by name (`choose_projector`,
  `crates/gglib-core/src/download/projector_choice.rs`). In a listing and a
  download, a projector is never a quantization and never a shard (verify
  still labels it one, #1253).
- **The launch reads it.** The runtime passes `--mmproj <projector>` for a
  linked model (`crates/gglib-runtime/src/command.rs`,
  `crates/gglib-runtime/src/process/residency/launch.rs`), refuses by name a
  projector missing on disk (`launch_files.rs`), and recycles a resident
  launched with another projector than the request resolves to, at the
  model's next admission (a proxy request or a start), unless a run holds it
  (`resident_match.rs`). The projector's size counts in the model's resident
  bytes (`crates/gglib-runtime/src/ports_impl/model_shards.rs`).
- **Every surface reads it, and none restates it.** The proxy's refusal, and
  its `/v1/models` entry's `"capabilities": ["vision"]`
  (`crates/gglib-proxy/src/models_list.rs`); `/v1/models/{name}/detail`,
  which carries `image_input` and not the path; the daemon's run gates; the
  CLI's `model list` Images column, `model inspect` Projector line and
  `model explain`'s resident figure; and the web page's `canSee`
  (`src/utils/canSee.ts`), which reads this machine's `imageInput` or the
  paired machine's `vision` capability.

Two kinds of model are not judged by this machine's column. A llama-server
named with `--port` is asked through its `GET /props` (decision 3), whatever
row this machine holds under the name the session gives, since that server is
what answers the turn (#1255). A model of the paired machine is judged on that
machine, by that machine's column, and refused there by the same code. This
machine reads that column only as the far list's `vision` capability or the
far detail's `image_input`, to show it and to decide what the page offers.

### 2. An image costs `min(ceil(w/32) * ceil(h/32), 4096)` prompt tokens

`estimate_image_tokens` (`crates/gglib-core/src/request_pipeline/images.rs`)
charges one token per 32-pixel square an image touches (`IMAGE_TOKEN_PX`), at
least 1 and at most 4,096 (`MAX_IMAGE_TOKENS`, the model family's own cap on
one image). The size is read from a PNG's or a JPEG's header alone
(`image_size.rs`); an image whose size cannot be read, an `http(s)` URL,
another format or a header cut short, is charged the cap.

Against the 2026-10-03 readings, 2560x1440 is 80 x 45 = 3,600 tokens against
3,646 measured, and 980x460 is 31 x 15 = 465 against 480. Both measurements
include the request's text and the chat template.

Where gglib needs an image's cost before llama-server has read it:

- history truncation measures an image at its estimate times the
  chars-per-token ratio in use, in place of its URL's length
  (`crates/gglib-core/src/request_pipeline/measure.rs`);
- an upload answers it as `image_tokens`, which the chat page shows as
  `~N tokens` with its share of the context, and the CLI prints on the line
  that receipts an image;
- the agent loop's pruning charges every image the cap, `IMAGE_CHARGE_CHARS`
  (`crates/gglib-core/src/domain/agent/messages.rs`), without reading its
  size.

A request that carries an image does not update the learned chars-per-token
ratio (`crates/gglib-proxy/src/token_calibration.rs`), and the content-hash
session id covers the images of the first user message
(`crates/gglib-proxy/src/fallback_session.rs`).

### 3. An image for a model that cannot see is refused by name, before any load

One predicate, `refuse_unless_can_see`, and one code and sentence,
`CannotReadImages` (`crates/gglib-core/src/request_pipeline/images.rs`):
`model_cannot_read_images`, a message that names the model and
`gglib model update <model> --projector <path>`. A request carries an image
when any of its messages does, history included, since the whole history is
sent each turn. It is asked at each site before a model is loaded:

| Site | Asked of | Where |
|---|---|---|
| The proxy's `POST /v1/chat/completions` | the resolved model, before admission, beside the embedding refusal | `crates/gglib-proxy/src/image_refusal.rs` |
| A daemon agent run on a server already started | the model that server serves | `image_gate::served`, `crates/gglib-axum/src/handlers/agent/image_gate.rs` |
| A paired device's turn on a hub chat | the chat's model, once it is known | `image_gate::named`, from `hub_turn.rs` |
| The CLI's `q` and `chat`, before the agent loop is composed | the catalogue row; with `--port`, that server's `GET /props` | `crates/gglib-cli/src/handlers/agent_chat/sight.rs` |

The proxy and the daemon answer HTTP 400 with the code; the CLI prints the
sentence. On `--port`, `modalities.vision: false` is the same refusal, and a
server that does not answer within 2 s, or answers without saying, is sent
the image.

A model of the paired machine is not judged on this machine. That machine
refuses, and the completion adapter renders an upstream refusal as
`<status> <type> (<code>): <message>`, so the code reaches the user with the
sentence. The chat page does not ask the predicate. It decides by `canSee`
what it offers, the attach button, paste and drop, and a far chat always
offers them.

### 4. gglib's own messages name an image by the hash of its bytes, stored once

- **By reference.** A user message carries `images`, a list of ids, beside
  its text (`AgentMessage::User`), on every gglib surface: the CLI, the chat
  page and a paired device's turn. A saved row answers each image as id,
  type, width and height (`AttachmentInfo`), never bytes.
- **Stored once, as sent.** An image is a row of `attachments` keyed by its
  `AttachmentId`, the lowercase hex SHA-256 of its bytes, and
  `message_attachments` links a message to its images in order and goes with
  the message by cascade (`crates/gglib-db/src/setup_attachments.rs`). One
  function takes an image in, `AttachmentService::ingest`
  (`crates/gglib-core/src/services/attachments.rs`), behind
  `POST /api/attachments`, the proxy's `POST /v1/attachments` and the CLI's
  `--image` and `/image`. It reads the type from the first bytes (PNG or
  JPEG, never a name or a `Content-Type`) and the size from the image's own
  header, and stores the bytes exactly as they came: no decode, no resize, no
  re-encode. So an id is the hash of the file the client holds, and the same
  bytes sent twice are one row and the same answer. Any downscaling is the
  client's: the chat page redraws an image over 2560 px or 8 MiB before it
  uploads it (`src/hooks/useGglibRuntime/imagePrep.ts`), and the CLI does
  none.
- **An id becomes bytes at one place.** The completion adapter
  (`crates/gglib-runtime/src/ports_impl/llm_completion/images.rs`) reads each
  id a request names from the store and writes it as an `image_url` data URL
  just before the request is sent, for this machine's model and the paired
  machine's alike. A user message with no image is the bare string it was
  before.
- **An outside client's images are not stored.** Its inline `image_url`
  parts go through the proxy as it sent them.
- **Unlinked rows go after a day.** An image is stored before the message
  that names it is saved. Rows no message links to, last stored more than a
  day ago, are deleted when the daemon starts and at no other time
  (`sweep_unlinked_attachments`).

### 5. An image read across a pairing is sent `no-store`

`PAIRED_ATTACHMENT_CACHE_CONTROL`
(`crates/gglib-core/src/contracts/http/attachments.rs`) is `no-store`, at the
two doors where an image crosses a pairing: the proxy's
`GET /v1/attachments/{id}`, which a named device reads
([ADR 0012](0012-the-remote-tunnel.md)), and the joined machine's relay,
`GET /api/remote/attachments/{id}`, which shows the far machine's image on
this machine's page. Neither machine stores the other's chats or their
images: the relay keeps nothing, and `no-store` asks whatever reads an image
across a pairing, a device or this machine's browser, not to keep it. The
relay types what it serves:
`image/png` or `image/jpeg` only when the far machine said so,
`application/octet-stream` otherwise, with `nosniff`
(`crates/gglib-axum/src/handlers/remote/attachments.rs`).

This machine's own image, read at `GET /api/attachments/{id}`, is
`private, max-age=31536000, immutable`: what an id names never changes. A
chat's images are kept by the machine that stores the chat. A far chat's
images are uploaded to the far machine's store. A chat stored here on the
paired machine's model ([ADR 0014](0014-a-model-is-named-by-its-machine.md)
decision 8) keeps its images here, and sends their bytes to the far proxy in
the completion request itself.

### 6. Three caps: 8 MiB an image, 16 MiB of images a request, a 32 MiB body

| Cap | Value | Enforced | Refusal |
|---|---|---|---|
| One image (`MAX_IMAGE_BYTES`) | 8 MiB | `AttachmentService::ingest`, and the body limit of `POST /api/attachments`, `POST /v1/attachments` and `POST /api/remote/attachments`, so an oversized body is refused while it arrives | 413 `image_too_large` |
| The images of one request (`MAX_REQUEST_IMAGE_BYTES`) | 16 MiB of raw bytes, history included; an image named twice is counted twice | the check before a run (decision 7), and the completion adapter as it reads the bytes | 400 `request_images_too_large` before a run |
| One request body (`MAX_BODY_BYTES`) | 32 MiB | the proxy's `POST /v1/chat/completions` and `PUT /v1/runs/{id}` | 413 `request_too_large` |

The values are in `crates/gglib-core/src/request_pipeline/images.rs` and
`crates/gglib-core/src/contracts/http/mod.rs`. 16 MiB of raw bytes is under
22 MiB as base64, which with a chat's text has to fit under the 32 MiB body
limit of a far proxy.

### 7. A run's images are checked before the run starts

`AttachmentService::check_request` (#1257) reads the size of every image a
run's messages name, history included, from the store (`length(data)`, never
the bytes). An id not stored is 400 `attachment_not_found`, and images over
16 MiB together are 400 `request_images_too_large`. It runs at an agent run,
`PUT /api/runs/{id}?kind=agent`, before a slot is taken, for this machine's
model and the paired machine's (`crates/gglib-axum/src/handlers/agent/run.rs`),
and at a paired device's turn on a hub chat before its model is loaded
(`hub_turn.rs`). The completion adapter's own check stays, as the backstop
for a request that reaches it another way: a CLI turn, or
`POST /api/agent/chat`, which has the image gate for a model here but not
this check.

## Consequences

**What a person notices:**

- A screenshot pasted in Copilot works with a model linked to a projector,
  and is counted at about 3,600 tokens at 2560x1440 rather than refused as
  too long ([docs/clients.md](../clients.md#images)).
- `gglib q --image shot.png "..."`, `gglib chat --image` and `/image <path>`
  in a chat attach a PNG or a JPEG, with one line per image saying its size
  and estimated tokens. The chat page takes an image by paste, drop or pick,
  shows `~N tokens` and its share of the context, and shows a turn's images
  when the chat is opened again.
- A model downloaded from a repository that has projectors reads images with
  no extra step. `gglib model download <repo> --list-quants` lists the
  projectors apart from the quantizations, with their size and which
  download fetches each, and the page's preview shows the projector the
  selected quantization's download fetches, with its size. `gglib model
  update <model> --no-projector` unlinks one.
- `gglib model list` has an Images column, `gglib model inspect` a Projector
  line, and the library a vision chip on each model that reads images, the
  paired machine's included.
- An image sent to a model that cannot read one is refused with the model's
  name and the command that links a projector, before anything is loaded or
  evicted.
- A changed projector link takes effect the next time the model is
  admitted, by a request through the proxy or a start, which recycles its
  running server unless a run holds it. A chat page run, or a device's turn,
  on a server already started is sent to that server as it is, with the
  projector it was started with.

**What it makes possible:** a paired device, the one client the proxy's
chats answer, sends an image once as `POST /v1/attachments` and names its id
in the turn that carries it. The hub keeps the chat's history, so no later
turn names it again, and the file crosses the tunnel once, not as base64 in
every turn's history. The device reads an image back with
`GET /v1/attachments/{id}`, which is sent `no-store`, so each showing reads
it again.

**Costs, accepted:**

- **A linked model loads its projector on every launch,** image or not. Its
  file size is counted in memory; the proof of concept's process was 29.2 GB
  resident. And llama-server at `cb7934c` switches off context shift and
  `--cache-reuse` when a projector is loaded, and makes no context checkpoint
  after an image chunk (llama.cpp's `tools/server/server-context.cpp:1241-1249`
  and `:3819` at that commit; reuse of a prompt's common prefix is not gated
  on the projector, `:3376-3378`). That is a reading of the source; no cache
  behaviour was measured ([log-0015, #1249's entry][download]).
- **An image is slow to read.** A 2560x1440 screenshot was 24 s of prefill
  on a 27B model on that Mac, and there is no setting to trade its detail for
  speed (#1263).
- **Every turn sends every image again.** The whole history is sent each
  turn, so its images are read from the store, encoded and sent each turn: up
  to 16 MiB of image bytes a request, across the tunnel for the paired
  machine's model.
- **The estimate is one family's.** Its constants rest on two images of one
  model family. A family that tokenizes images differently is charged
  wrongly, and nothing notices until a request is refused or truncated: the
  dashboards do not show what a request's images cost (#1261). The agent's
  pruning charges every image the cap, so a small image is overcharged there.
- **Images live in the database.** `attachments.data` holds every image a
  message names for as long as the message exists, and an image uploaded for
  a turn never sent stays until the first daemon start a day later.
- **Broken wire types.** A message's wire types gained `images`, and the
  page's unused base64 image, audio and file parts are gone (#1264);
  `CapabilityFlags::VISION` and the `vision` tag are gone (#1244).
- **Known gaps.** assistant-ui sends an edit only when its text changed, so
  an edit that adds or removes only an image does nothing. A projector cannot
  be downloaded on its own, and one linked from another repository is not
  checked for updates. Three groups in the listing of
  `unsloth/Qwen3.8-27B-GGUF` are still wrong, none of them a projector
  ([log-0015, #1249's entry][download]). A link changed while the model is
  served takes effect for a chat page run or a device's turn only once that
  server is restarted, or recycled when the model is next admitted. A link
  added that way lets the page offer images and the daemon's gate pass them,
  and llama-server, started without `--mmproj`, answers HTTP 500
  ([log-0015, 2026-10-03][poc]).
  `gglib model upgrade` fetches a projector and does not link it (#1252).
  `gglib model verify` calls a projector a shard (#1253). With `--port`, the
  server's `/props` decides even when the library knows the model, and a
  server that does not say is sent the image (#1255).

## Kill criteria

- **If the estimate is more than a tenth off what llama-server counts,
  decision 2 is reopened.** The reading is one pair of requests, taken by
  hand until #1261 puts it on a dashboard: an upload's `image_tokens` against
  what the image adds to a completion's `usage.prompt_tokens`. Upload a
  2560x1440 PNG to `POST /api/attachments`, whose answer's `image_tokens` is
  the estimate, 3,600 (the chat page shows it beside the image as
  `~3,600 tokens`). Then send a one-line question with the same file as a
  `data:` URL through the proxy's `POST /v1/chat/completions` to a model
  linked to a projector, and the same request again without the image. The
  measurement is the `usage.prompt_tokens` of the request with the image,
  less that of the same request sent without it, so neither a template's
  system prompt nor the question can cross the threshold. The one reading so
  far is the proof of concept's, taken without the proxy on one model family
  and llama.cpp `cb7934c` ([log-0015, 2026-10-03][poc]): 3,646 prompt tokens
  with the question and the chat template included, against 3,600. The log
  records no request without the image, and puts the 46 tokens between them
  down to the question and the template. A reading through gglib's
  proxy is appended to log-0015.md, with its model, its build and its
  machine. A measurement above 3,960 or below 3,240, a tenth either side of
  3,600, on any model reverses it: one formula for every family is then the
  wrong shape.
- **If llama-server reads images without a projector, decision 1 is
  reopened.** The reading is taken at each llama.cpp pin bump, the moment
  [ADR 0012](0012-the-remote-tunnel.md)'s first criterion already uses:
  `PINNED_LLAMA_RELEASE` in `crates/gglib-runtime/src/llama/download/mod.rs`
  moves, and `gglib config llama status` prints the binary and the commit
  installed. Start that `llama-server` on the weights of a model that has a
  projector, without `--mmproj`, and read `GET /props`. The proof of concept
  read `modalities` as all `false` there, at `cb7934c` and not at the pin
  ([log-0015, 2026-10-03][poc]). `vision: true` reverses it: the column is
  then not the one fact, and the server's own answer is read, as the CLI
  reads it already on `--port`.

## Out of scope

- **Audio and video.** gglib's own surfaces attach PNG and JPEG images
  alone, and nothing here measures, refuses or stores an audio or a video
  part, though `/props` reported `video: true` with the projector:
  [#1259](https://github.com/mmogr/gglib/issues/1259).
- **Pasting from the clipboard in the CLI.** `--image` and `/image` read a
  file: [#1260](https://github.com/mmogr/gglib/issues/1260).
- **A dashboard row for what a request's images cost,** estimated and
  measured: [#1261](https://github.com/mmogr/gglib/issues/1261).
- **Images in a chat export, and in `gglib run start`:**
  [#1262](https://github.com/mmogr/gglib/issues/1262).
- **`--image-max-tokens`,** a cap on an image's cost below the family's
  4,096: [#1263](https://github.com/mmogr/gglib/issues/1263).
- **The phone's camera and share extension,** which are the phone app's:
  [ggchat #166](https://github.com/mmogr/ggchat/issues/166).
- **One list of a model's files.** A downloaded model's files stay recorded
  in four columns, in three forms. Image input does not need one list of
  them, since the projector has its own column (decision 1):
  [#1250](https://github.com/mmogr/gglib/issues/1250).

[poc]: log-0015.md#2026-10-03-llama-server-reads-an-image-when-it-is-started-with-the-models-projector
[estimate]: log-0015.md#2026-10-04-an-image-is-counted-by-its-pixels-and-refused-by-name-where-it-cannot-be-read
[download]: log-0015.md#2026-10-04-a-projector-is-fetched-with-its-model-and-chosen-by-one-rule
[reference]: log-0015.md#2026-10-04-gglibs-own-surfaces-name-an-image-by-its-id-and-the-bytes-are-stored-once
