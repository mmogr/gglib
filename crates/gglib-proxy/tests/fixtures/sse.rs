//! Static SSE byte fixtures used by the proxy round-trip integration tests.
//!
//! Each fixture represents a complete upstream response — the bytes that
//! `llama-server` (or a model-specific dialect impostor) would send back to
//! the proxy.  Tests feed these to a mock upstream HTTP server and then
//! assert the bytes the *external client* receives from the proxy.
//!
//! All fixtures terminate with `data: [DONE]\n\n` exactly as `llama-server`
//! emits.

/// Standard OpenAI streaming: three text-content deltas followed by a
/// terminator.  No reasoning, no tool calls.
pub const BASIC_TEXT: &[u8] = b"\
data: {\"id\":\"u-1\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"u-1\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\", \"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"u-1\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"world\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"u-1\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n";

/// Reasoning model emitting `reasoning_content` (DeepSeek R1 / QwQ style)
/// followed by answer text.  The pipeline must surface both as separate
/// `reasoning_content` and `content` deltas in the re-emitted frames.
pub const REASONING_DEEPSEEK: &[u8] = b"\
data: {\"id\":\"u-2\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"Let me think.\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"u-2\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"42\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"u-2\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n";

/// Reasoning model that never leaves the thinking channel: `reasoning_content`
/// only, `finish_reason: "stop"`, and no `content` at all.  This is what a
/// model that fails to close its `<think>` block looks like on the wire, and it
/// renders as an empty response in clients that collapse reasoning.  The proxy
/// must promote the stranded text into the content channel.
pub const REASONING_ONLY: &[u8] = b"\
data: {\"id\":\"u-9\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"The answer \"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"u-9\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"is 42.\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"u-9\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n";

/// Qwen-family model emitting an XML-wrapped tool call inside the text
/// channel.  With `format:qwen-xml` tags the pipeline must rewrite this into
/// strict OpenAI `tool_calls` deltas — the external client should never see
/// the `<tool_call>` markers.
pub const QWEN_XML_TOOL_CALL: &[u8] = b"\
data: {\"id\":\"u-3\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Looking it up. \"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"u-3\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"<tool_call>{\\\"name\\\":\\\"get_weather\\\",\\\"arguments\\\":{\\\"city\\\":\\\"Paris\\\"}}</tool_call>\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"u-3\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n\
data: [DONE]\n\n";

/// Qwen3 + `--jinja` inner-XML tool call — the Hermes-style
/// `<function=NAME><parameter=KEY>VALUE</parameter></function>` body inside
/// the same `<tool_call>` wrapper, as opposed to [`QWEN_XML_TOOL_CALL`]'s
/// JSON body.  Exercises `qwen_xml`'s second dialect through the full
/// pipeline, not just the parser's own unit tests.
pub const QWEN_FUNCTION_XML_TOOL_CALL: &[u8] = b"\
data: {\"id\":\"u-10\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Looking it up. \"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"u-10\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"<tool_call><function=get_weather><parameter=city>Paris</parameter></function></tool_call>\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"u-10\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n\
data: [DONE]\n\n";

/// A template-derived dialect: custom multibyte `«TC»`/`«/TC»` markers with
/// a JSON body, and the CLOSE MARKER SPLIT ACROSS TWO SSE FRAMES — the
/// end-to-end proof that a spec nobody hardcoded parses chunk-safely
/// through the whole proxy pipeline.  Byte-string literals cannot hold
/// non-ASCII, hence `str::as_bytes`.
pub const DERIVED_MARKER_TOOL_CALL: &[u8] =
    "data: {\"id\":\"u-11\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Sure. \"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"u-11\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"«TC»{\\\"name\\\":\\\"get_weather\\\",\\\"arguments\\\":{\\\"city\\\":\\\"Paris\\\"}}«/T\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"u-11\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"C» done.\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"u-11\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n\
data: [DONE]\n\n"
        .as_bytes();

/// Standard OpenAI tool call (already strict / no dialect rewriting).  The
/// proxy must round-trip this preserving `id`, `type:"function"`, `name`,
/// `arguments`, and the `index`.
pub const STANDARD_OPENAI_TOOL_CALL: &[u8] = b"\
data: {\"id\":\"u-4\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_abc\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"u-4\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"city\\\":\\\"Paris\\\"}\"}}]},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"u-4\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n\
data: [DONE]\n\n";

/// A malformed `data:` payload (broken JSON) sandwiched between two valid
/// frames.  The pipeline cannot recover frame boundaries after a JSON parse
/// failure, so it must surface the pre-error content, emit a structured
/// `error` data frame, and terminate the stream with `[DONE]`.
pub const MALFORMED_JSON_RECOVERY: &[u8] = b"\
data: {\"id\":\"u-5\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"before\"},\"finish_reason\":null}]}\n\n\
data: {not valid json at all\n\n\
data: {\"id\":\"u-5\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"after\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"u-5\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n";

/// Same logical payload as [`BASIC_TEXT`] but split across deliberately
/// awkward byte boundaries — half a `data: …` frame here, the rest plus the
/// next frame's prefix in the following chunk.  Exercises the
/// `SseStreamDecoder` buffer carry-over.
pub fn basic_text_split_chunks() -> Vec<&'static [u8]> {
    // Hand-crafted split points that bisect:
    //   - the JSON body of the first frame
    //   - between the second and third frame
    //   - inside the `data: [DONE]` terminator
    vec![
        b"data: {\"id\":\"u-6\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"split-",
        b"frame-1\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"u-6\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"-and-2\"},\"finish_reason\":null}]}\n\n",
        b"data: {\"id\":\"u-6\",\"object\":\"chat.completion.chunk\",\"created\":1729000000,\"model\":\"upstream\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DO",
        b"NE]\n\n",
    ]
}
