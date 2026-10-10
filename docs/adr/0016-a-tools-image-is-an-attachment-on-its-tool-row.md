# ADR 0016 — A tool's image is an attachment on its tool row: stored through the one intake, shown to the user, never sent to the model

- **Status:** Accepted
- **Date:** 2026-10-09
- **Depends on:** [ADR 0015](0015-a-model-reads-images-through-its-projector.md)
  [decision 4](0015-a-model-reads-images-through-its-projector.md#4-gglibs-own-messages-name-an-image-by-the-hash-of-its-bytes-stored-once)
- **Supersedes:** nothing
- **Superseded by:** nothing
- **Log:** none

## Context

An MCP tool can return an image. A `tools/call` result is a `content` list
of items, and an image item carries its bytes as base64 `data` beside a
`mimeType`. At origin/main `c75e2c8d`, gglib dropped such an image or leaked
its bytes:

- **With a text item, the image was dropped.** `extract_mcp_content_text`
  (`crates/gglib-mcp/src/tool_executor.rs:173-193` at `c75e2c8d`) joined the
  `text` of the items that had one and skipped every other item
  (`:180-187`).
- **Without one, the bytes went everywhere.** A list with no text item fell
  through to `v.to_string()` (`:192`), so the whole list, base64 included,
  became the `ToolResult`'s `content`: the text the model was sent as the
  tool's message, the `result` of the run's `tool_call_complete` frame, and
  the content of the saved tool row.
- **The `/mcp` gateway sent the list as one text.** `invoke_tool`
  pretty-printed the server's whole content into one text item
  (`crates/gglib-proxy/src/mcp/handlers.rs:321-327` at `c75e2c8d`), so an
  `/mcp` client got an image as base64 inside a string.
- **The stdio client could not time out a slow server.** It read the
  server's stdout with a blocking `read_line` inside tokio's 30 s timeout
  (`crates/gglib-mcp/src/client.rs:366-411` at `c75e2c8d`), so the timer
  could not fire while a read blocked, and it took the first JSON-RPC line
  among at most ten as the reply, whatever its id.

**What motivates it.** The arc this starts is a model that draws, whose
picture would come back through a tool.

## Decision

Seven decisions. They carry [ADR 0015](0015-a-model-reads-images-through-its-projector.md)
decision 4, under which a user's image is stored once under the hash of its
bytes and named by id, over to an image a tool returns.

### 1. A tool's image goes through the one intake

`split_content` (`crates/gglib-mcp/src/tool_images.rs`) splits an MCP tool's
`content`. Text items join with `\n`, as before. An `image` item's `data` is
decoded from base64 and passed to `AttachmentService::ingest`, the function
that takes a user's image in, so a tool's image is held to the same rules: a
PNG or a JPEG by its first bytes, at most 8 MiB (ADR 0015 decision 6), stored
once under the SHA-256 of its bytes, exactly as they came. The item's
`mimeType` is not read.

`CombinedToolExecutor::new` and `with_sandbox`
(`crates/gglib-mcp/src/combined.rs`) take the `AttachmentService`, and
`compose_agent_loop_inner` (`crates/gglib-runtime/src/compose.rs`) builds it
from the attachment store it is given, the one the completion adapter reads
images from. The daemon's runs (`compose_agent_loop`) and the CLI's chat
(`compose_agent_loop_with_sampling`) both compose their loop through it.
Only an MCP tool's result is split: the builtin tools return text.

### 2. `ToolResult` carries `AttachmentInfo`, not ids

`ToolResult.images` (`crates/gglib-core/src/domain/agent/tool_types.rs`)
lists the images the tool stored, in the order it returned them, each as id,
type, width and height. A client knows an image's type and size from these
without asking the store: the chat page labels a tile `1024 × 1024`, and the
CLI's marker prints the size and the start of the id. The field is left out
of the JSON when empty and read as empty when absent, so a result without
images is the frame it was (`a_result_without_images_is_the_frame_it_always_was`).
The web's `AgentToolResult` is the binding generated from it
(`src/types/generated/ToolResult.ts`).

### 3. The image is linked to the tool row where it was made

`replay::tool_row` (`crates/gglib-core/src/domain/agent/replay.rs`) puts a
result's image ids on its tool row's `images`, and the row is saved with
them as a user row is: `message_attachments` links an image to a row of any
role, and reads it back as facts (`crates/gglib-db/src/repositories/message_rows.rs`).
The daemon and the CLI save a reply through `rows_from_timed_frames`
(`crates/gglib-app-services/src/transcript.rs`), so both link it
(`a_tool_row_carries_the_ids_of_the_images_its_result_made`). No other row
gains the image, and the schema does not change.

### 4. No image item's bytes are in a result's text

The result's text, which the model reads, the `tool_call_complete` frame
carries and the tool row saves, names each image in its place:
`[image 1024x1024 PNG stored]`. A result with neither text nor an image
names each item by its kind (`describe_item`: `[resource <uri>]`,
`[resource link <uri>]`, `[audio content not shown]`) and is never dumped as
JSON: a number, a boolean or `null` is its JSON text, a string in the list is
`[content not shown]`, and an empty list is `[no content]`. A result that is
a plain string, a text item and an error's message pass as they came.

The stdio client logs a line that is not JSON only by its first 120
characters and its length, and a reply to another request only by its id
(`crates/gglib-mcp/src/client.rs`).

### 5. A tool's image is never sent to the model

`AgentMessage::Tool` is unchanged, a tool call id and the text, and has no
field for an image. A resumed tool row drops its images
(`a_tool_message_resumes_without_its_images`), and the completion body for
a saved tool row with images, with their bytes in the store, is the body
without them (`a_saved_tool_rows_images_never_reach_the_model`,
`crates/gglib-runtime/src/ports_impl/llm_completion/images_tests.rs`). The
model reads the sentence that names the image.

### 6. `/mcp` answers protocol image items

`from_upstream` (`crates/gglib-proxy/src/mcp/call_result.rs`) passes a
server's content on as MCP items: a text item as text, an image item with
its base64 `data` and `mimeType` as they came, and any other item, an image
item missing either of those among them, as a text item naming it in
`describe_item`'s words. An `/mcp` client speaks MCP and reads an image item
itself, so the gateway stores nothing. A failed call is one text item with
the error's message and `isError: true`. `search_tools` and
`get_tool_schema` answer as before, one text item of JSON.

### 7. A refused image becomes a sentence

An image `ingest` refuses, another format or over 8 MiB, becomes
`[image not stored: <why>]` in the text, in the refusal's own words, and
nothing is stored. Data that is missing or not base64 is
`[image not stored: The data is not base64.]`. A store fault gets the same
sentence and is also logged as a warning, without the bytes. The result is
still a success.

## Consequences

**What a person notices:**

- **`gglib chat`.** A tool's line ends with ` [image WxH 3f9a2c1e]` for each
  image it made: the size and the first 8 characters of the id. A user's
  image gets the same marker. `gglib chat --continue` shows the images the
  last turn's tools made on a line of their own. `gglib attachment save <id>
  [path]` writes one to a file as stored, from the whole id or its first 8 or
  more characters ([docs/cli.md](../cli.md#attachment-save)).
- **The chat page** draws the images of an assistant message's tool calls as
  one strip in that message's body, under its text and outside "How this was
  made", in the order the calls come: live from `tool_call_complete`, and
  from the saved tool rows when the chat is opened again. Each is read from
  the chat's store by its id and enlarged on a click. The folded tool rows
  show their text only.
- **A paired device** is sent `result.images` on a run's
  `tool_call_complete` frames and `images` on an opened chat's tool rows
  (`tool_reply` in `contracts/runs/recorded.json` and
  `contracts/chats/recorded.json`), and can read each image's bytes by id at
  `GET /v1/attachments/{id}`.
- **An `/mcp` client** gets a tool's items as items, where it got one text
  item of pretty-printed JSON.

**Costs, accepted:**

- **An MCP tool call has 30 s.** The client's reply timeout
  (`REPLY_TIMEOUT`, `crates/gglib-mcp/src/client.rs`) is 30 s from writing a
  request to reading its reply, and now fires. The agent loop's tool timeout
  defaults to 30 s too (`tool_timeout_ms`,
  `crates/gglib-core/src/domain/agent/config.rs`). A third-party image server
  slower than that is cut: the call fails, the model reads a failed result,
  and nothing of its image is stored.
- **One request at a time per server.** A server's calls queue on its
  pipes' lock. The wait is not counted in the client's 30 s, and is counted
  in the loop's.
- **The model does not see what a tool made.** It reads the sentence
  (decision 5).
- **Only PNG and JPEG are kept.** A WebP or a GIF from a tool is a sentence
  (decision 7).
- **A tool's images live in the database,** as a user's do: in
  `attachments.data`, for as long as a row links them. One stored for a reply
  whose rows are never saved, and linked by no other row, is deleted at the
  first daemon start more than a day after it was stored
  (`sweep_unlinked_attachments`, ADR 0015 decision 4).
- **Every store finds an image by the start of its id.**
  `AttachmentStore::ids_starting_with` is required of every store, test
  doubles included; a store that holds its ids in memory answers with
  `ids_starting_with` beside the trait
  (`crates/gglib-core/src/ports/attachment_store.rs`).
- **Broken wire shapes.** `/mcp`'s `tools/call` answers items, not one text
  item of JSON. The CLI's image marker gained the start of the id, for a
  user's image too.

## Kill criteria

- **If the model needs to see a tool's image, and llama-server can be sent
  one in a tool message, decision 5 is reopened.** Two readings, neither
  taken. The first is a recorded run in which the model's answer depends on
  what a tool's image shows and is wrong because it read only the sentence
  naming it. The second is taken at a llama.cpp pin bump, when
  `PINNED_LLAMA_RELEASE` in `crates/gglib-runtime/src/llama/download/mod.rs`
  moves: start that llama-server with a projector, and send a chat
  completion whose `role: "tool"` message carries an `image_url` part. An
  answer that describes the image is the reading; whether llama-server reads
  an image there is unverified. Both together reverse decision 5; either
  alone does not.

## Out of scope

- **Drawing.** This carries an image a tool returns; it adds no tool that
  draws.
- **Builtin tools.** They return text, and `/mcp` does not reach them; both
  are unchanged.
- **Items other than text and images,** such as audio, resources and
  resource links: named in the text, never stored.
- **What ggchat shows** of these frames and rows, which is ggchat's.
- **A tool's own deadline,** longer than the 30 s every MCP call has.
