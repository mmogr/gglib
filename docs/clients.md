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

## Error codes

Every error code gglib's proxy writes, in a refusal, in a stream's error frame
or in a failed run, is listed with its type, HTTP status and meaning in
[error-codes.json](error-codes.json). Match on `code`. gglib's tests check
the file against the source.

## Sampling profiles

Append `:coding` to a model name (e.g. `qwen3.6:coding`) to select a sampling
profile. See [Sampling → Inference profiles](sampling.md#inference-profiles-modelprofile).
