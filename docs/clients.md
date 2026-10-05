# Client Configuration Examples

The endpoint is `http://127.0.0.1:8080/v1`. No API key is required on
loopback — enter any placeholder if your client insists. Use a model name from
`gglib model list` (shown below as `qwen3.6`). In `/v1/models`, the model to
send is an entry's `id`; its `gglib_id` is gglib's own catalog id for it.

## Cline / Roo Code

Settings → API Provider: *OpenAI Compatible*, Base URL
`http://127.0.0.1:8080/v1`, API Key `gglib`, Model ID `qwen3.6`.

## Continue

`config.yaml`:

```yaml
models:
  - name: qwen3.6 (local)
    provider: openai
    model: qwen3.6
    apiBase: http://127.0.0.1:8080/v1
    apiKey: gglib
```

## Aider

```bash
OPENAI_API_BASE=http://127.0.0.1:8080/v1 OPENAI_API_KEY=gglib aider --model openai/qwen3.6
```

## Zed

`settings.json`:

```json
{
  "language_models": {
    "openai_compatible": {
      "gglib": {
        "api_url": "http://127.0.0.1:8080/v1",
        "available_models": [{ "name": "qwen3.6", "max_tokens": 32768 }]
      }
    }
  }
}
```

## From another machine

When this machine is connected to another with `gglib remote join`
([Remote access](remote.md)), the other machine's proxy is at
`http://127.0.0.1:<port>/v1` here — the port `join` printed, also shown
by `gglib remote status`. Every recipe above works against it with two
changes: that port instead of `8080`, and this machine's device key instead
of a placeholder. The device key is the one this machine was given when it
paired, not the other machine's `proxy_api_key`; `gglib remote key --show`
prints it. The port does not add it for you, on purpose — see
[Why the port does not inject the key](remote.md#why-the-port-does-not-inject-the-key).

`gglib q --remote` and `gglib chat --remote` need neither: they attach the
key themselves.

## Images

A client that sends images in the `OpenAI` shape, as GitHub Copilot does when
a screenshot is pasted, works with a model that is linked to a projector
(`gglib model update <model> --projector <path>`). The image is an `image_url`
part of a message's `content`, as a `data:image/png;base64,…` or
`data:image/jpeg;base64,…` URL:

```json
{"role": "user", "content": [
  {"type": "text", "text": "What is the error?"},
  {"type": "image_url", "image_url": {"url": "data:image/png;base64,iVBORw0…"}}
]}
```

Which models read images is in `/v1/models`: such a model's entry carries
`"capabilities": ["vision"]`.

- **What an image costs.** gglib counts an image against the model's context
  by its pixels, not by the length of its base64: about one token per 32x32
  pixels, at most 4,096 an image. A 2560x1440 screenshot is about 3,600
  tokens. An image whose size gglib cannot read from its header (a PNG or a
  JPEG can be read; an `http(s)` URL or another format cannot) is counted at
  4,096.
- **A model that cannot read images refuses them by name.** A request with an
  image anywhere in its messages, earlier turns included, for a model with no
  projector is answered with HTTP 400 before anything is loaded:

  ```json
  {"error": {"message": "Model 'qwen3.6' cannot read images: it has no projector linked. Link one with `gglib model update qwen3.6 --projector <path>`, or name a model that has one.",
             "type": "invalid_request_error", "code": "model_cannot_read_images"}}
  ```

- **The body limit is 32 MiB** on `POST /v1/chat/completions` and
  `PUT /v1/runs/{id}`. A larger body is answered with HTTP 413 and the code
  `request_too_large`.

## Thinking

Which models think is in `/v1/models`: the entry of a model tagged
`reasoning` lists `"reasoning"` in its `capabilities`, after `"vision"` when
it has both (`"capabilities": ["vision", "reasoning"]`). A gglib from before
this never lists it, so a client that offers a Thinking switch only for such
a model offers none there.

A client that keeps its own chats, as a phone app does for the chats gglib
never stores, turns thinking off for a request by sending
`"reasoning_budget_tokens": 0` in the body of `POST /v1/chat/completions`,
and leaves the key out otherwise. gglib honours that key from any client,
whatever `trust_client_sampling` says
([The two reasoning controls](sampling.md#the-two-reasoning-controls)). The
choice is the client's to keep: gglib remembers nothing of such a request.

gglib's own turn routes take the choice as a word and remember it on the
chat: `"thinking": "off"` or `"thinking": "default"` beside the message, on a
paired device's turn (`PUT /v1/runs/{id}?kind=agent`), on the chat page's
turn on a paired machine (`PUT /api/remote/chats/{id}/turns/{run_id}`) and on
this machine's own agent run (`PUT /api/runs/{id}?kind=agent`). Send it only
on the turn that changes it. `off` runs that turn and the chat's later ones
with a budget of `0`; `default` forgets that; a turn that says neither runs
as the chat remembers, which its `settings.thinking` says when the chat is
opened. [What the other machine can reach](remote.md#what-the-other-machine-can-reach)
has the rest.

gglib's chat page is such a client. Its composer has a Thinking switch only
for a model that thinks, showing what the chat remembers; it sends `thinking`
on the message after a click, and reads the chat's settings again when the
chat is opened and after each reply, so a chat switched off on a paired
device opens switched off there and the reverse. Once the message that
carries a click is accepted the page says nothing more of it: the switch
shows what the chat remembers when it is next read, so a change made since on
another device is shown, and the page's next message does not undo it. A far
chat's model is the one its settings name (`model_name`), else its last
reply's, looked up in that machine's model list, so a chat that has never run
shows no switch until its first reply.

## Context reading

A client that wants to show how full a model's context is gets the two facts
it cannot work out by itself from gglib, beside the token counts it is
already sent: the context the server that answered was launched with, and how
many earlier messages gglib shortened or left out so the request fit.

Send `"stream": true` and `"return_progress": true` in the request body. The
stream's usage frame, the one with an empty `choices`, then carries two more
keys inside `usage`:

```json
{"choices":[],"created":1729000000,"id":"chatcmpl-5b1e","model":"qwen3.6","object":"chat.completion.chunk",
 "usage":{"completion_tokens":96,"context_size":8192,"prompt_tokens":812,"total_tokens":908,"trimmed_messages":3}}
```

- `context_size` is the context, in tokens, that the server which answered
  this request was launched with.
- `trimmed_messages` is how many earlier messages were shortened to a
  placeholder so the request fit. It is left out when there were none: a
  missing key means none, and it is never `0` or `null`.

**The rule for a client.** The context used is `prompt_tokens +
completion_tokens` of the newest reply that finished, taken from that reply's
last model call and never summed over calls; a reply that was stopped before
any of its model calls finished leaves the reading of the one before it. Draw
it only when both counts and `context_size` are known, and draw nothing
otherwise. Never divide by the `context_window` of a `/v1/models` entry:
every figure there is advertised 8% low, to leave a client headroom, and only
the entry of the model loaded at that moment starts from the context its
server has. The rest start from the catalogue's.

The rule is worked through, case by case, in
[`contracts/context/readings.json`](../contracts/context/readings.json), and
where this page and that file differ the file is right. It says which reply
decides (one that did not finish is passed over only when it carries no
counts), when a reading is drawn, the percent as a whole number (a half
rounds up, and it is never over 100), the warning from 70 and the stronger
one from 90, and the sentences a client says. It is written by hand for every
client to replay; gglib's chat page replays every case in its tests.

`return_progress` is llama.cpp's own key, and it also asks for
`prompt_progress` frames, which have no `choices`. On
`POST /v1/chat/completions` a client that does not send it is sent neither:
its stream is byte for byte what it was before the reading existed. A chat
run (`PUT /v1/runs/{id}`) sets the key for every client, so a run's usage
frame always carries the reading. A reply that is not streamed carries no
reading.

An agent run says the same of each model call. Its `turn_usage` event carries
`context_size` and `trimmed_messages` flat, under the same two names, with
`finish_reason` beside them (`length` is a reply cut off at the model's
limit). There `trimmed_messages` counts the messages the run left out of the
request that call answered, over the whole run so far, and `context_size` is
left out when the run does not know it: on a paired machine's model, or a
model loaded in the second of this machine's two slots. A saved reply's row
keeps them in its `metadata` as `contextSize`, `trimmedMessages` and
`finishReason`, each only when the turn had it, so a chat opened later reads
the same figures as one watched live.

## Error codes

Every error code gglib's proxy writes, in a refusal, in a stream's error frame
or in a failed run, is listed with its type, HTTP status and meaning in
[error-codes.json](error-codes.json). Match on `code`. gglib's tests check
the file against the source.

## Sampling profiles

Append `:coding` to a model name (e.g. `qwen3.6:coding`) to select a sampling
profile. See [Sampling → Inference profiles](sampling.md#inference-profiles-modelprofile).
