# Client Configuration Examples

The endpoint is `http://127.0.0.1:8080/v1`. No API key is required on
loopback — enter any placeholder if your client insists. Use a model name from
`gglib model list` (shown below as `qwen3.6`).

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

When this machine is connected to another with `gglib remote connect`
([Remote access](remote.md)), the other machine's proxy is at
`http://127.0.0.1:<port>/v1` here — the port `connect` printed, also shown
by `gglib remote status`. Every recipe above works against it with two
changes: that port instead of `8080`, and the *other* machine's API key
instead of a placeholder. The key is that machine's `proxy_api_key`
(`gglib config settings show` there); the port does not add it for you, on
purpose — see [Why the port does not inject the key](remote.md#why-the-port-does-not-inject-the-key).

`gglib q --remote` and `gglib chat --remote` need neither: they attach the
key themselves.

## Sampling profiles

Append `:coding` to a model name (e.g. `qwen3.6:coding`) to select a sampling
profile. See [Sampling → Inference profiles](sampling.md#inference-profiles-modelprofile).
