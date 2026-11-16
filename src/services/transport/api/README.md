# api

<!-- module-docs:start -->

HTTP API transport implementations for all backend domains. Wraps `fetch` with automatic bearer-token injection, retry logic on 401/network failures, and API session discovery in Tauri environments. A request `client.ts`'s JSON helpers cannot carry (a stream, raw bytes) goes through its `apiFetch`, the one authenticated `fetch`; a stream's events are read by `readSse` in `utils/sse.ts`. `createApiTransport()` spreads these modules into the object `getTransport()` returns — the models pair through `createModelsApi()`, the rest as namespaces.

## Request Flow

```
useChatConversations.ts: getTransport().listConversations()
       ▼
transport/api/chat.ts
       ▼
client.ts  ─── injects Authorization header
           ─── retries on 401 (re-fetches session token)
           ─── throws TransportError on failure
       ▼
timeLimit.ts ─ the one fetch, held to the request's time limit
       ▼
Axum backend (HTTP response) → typed result
```

## Key Files

| File | Role |
|------|------|
| `client.ts` | HTTP client with auth injection, retry, and error normalization; `apiFetch` for a stream or a body that is not JSON. Both make their `fetch` through `timeLimit.ts` |
| `timeLimit.ts` | The time limits and the one `fetch` every request to the daemon is made through (`fetchWithin`): 30 s on a read (through `get`, its body included; through `apiFetch`, until its headers come, since the caller reads the body); 1.5 s on the health probe; a stream (`stream: true`) until its headers come and never after; none on a mutation unless its caller sets `timeoutMs`. A read answered from the Hugging Face Hub gets 5 minutes (`HUB_READ_LIMIT_MS`); an image or a run's events from the paired machine 90 s to its headers (`FAR_STREAMED_LIMIT_MS`). One that runs out rejects with a `TIMEOUT` `TransportError`, "gglib did not answer in 30 seconds.", never an abort |
| `renew.ts` | A stream refused with a 401 renews the session's credential before it reconnects, and says it did, so the stream reconnects at once |
| `addressLookup.ts` | Where the desktop app's lookup of the daemon's address (`get_embedded_api_info`, which `client.ts` asks) stands: out, answered, or failed with the launch's own words; recorded, never asked or timed |
| `daemonToken.ts` | The daemon's token from the link `gglib web` prints: stripped from the address bar, kept until the daemon next starts |
| `chat.ts` | Conversations and messages, a conversation's model, tools and tool limits (`settings` on `PUT /api/conversations/{id}`, each one sent and no other), a chat's title asked of its model (`POST /api/chat`), and a picture kept beside the one it was made from (`POST /api/conversations/{id}/pictures`) |
| `servers.ts` | llama.cpp server lifecycle and the running list |
| `downloads.ts` | Download queue management |
| `mcp.ts` | MCP server config, lifecycle and config test |
| `settings.ts` | Application settings, and the starter-profile install the daemon runs |
| `secrets.ts` | The one reveal of a stored secret (`POST /api/secrets/reveal`): the proxy key or an MCP server's env value, for Show and Copy, and the proxy key `useProxyKey` hands the dashboard, the tray and Clear Cache; the pairing key is refused, and the answer is never logged |
| `tags.ts` | Model tags |
| `builtin.ts` | Built-in tool listing |
| `verification.ts` | Model verification |
| `proxy.ts` | OpenAI-compatible proxy management |
| `remote.ts` | The remote tunnel (ADR 0012): enable/disable/status here, join/disconnect/kill for another machine |
| `models/` | Local and HuggingFace model APIs |
| `setup.ts` | First-run setup and dependency probes; the llama.cpp and image runtime installs, status and removal |
| `sse.ts` | `streamSse`: a POST whose reply is an event stream (llama install and update), handed to callbacks frame by frame |
| `runs.ts` | Runs (`/api/runs`): start an agent run under a minted id, list, cancel, and read its events from any point |
| `farChats.ts` | The far machine's chats and runs (`/api/remote/chats`, `/api/remote/runs`), forwarded by this machine's daemon: list, open, add a turn of text and images (by the ids the far store answered) that says the chat's Thinking choice only when it changes it and `draw` only when sent with Draw pressed, with `image_model` beside it only when one was chosen, and read, list and cancel its runs |
| `images.ts` | Whether a message sent with Draw pressed can draw, and why not (`/api/images/drawing`, told whether the chat's model is the paired machine's and whether it calls tools; the far machine's answer at `/api/remote/images/drawing` for a far chat), with that machine's image models; either is asked about a chosen one with `image_model`. Also a picture drawn again from itself (`redrawPicture`): `/api/images/generations` sent the picture, a strength and a mask, read as the stream it answers |
| `attachments.ts` | The image stores (`/api/attachments`, and the far machine's at `/api/remote/attachments` for a far chat): upload an image as its raw bytes, and read one's bytes back with the page's credential |
| `farModels.ts` | The paired machine's models (`/api/remote/models`), read by this machine's daemon: the list with that machine and what may be done there, one model by its id there, and a load |
| `version.ts` | Which build of gglib the daemon is running |
| `health.ts` | `probeDaemon`: whether a daemon answers `/health` at all within 1.5 s (outside the token and key guards and the client's 401 handling); `tokenRefusal`: the daemon's sentence when it refuses this tab's token, read once through `apiFetch` |

<!-- module-docs:end -->
