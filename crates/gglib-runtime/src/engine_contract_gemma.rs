//! What the recorder asks tinygemma3: whether the engine reads images
//! without its projector, the reply it streams to [`question`] at
//! temperature 0, what llama-server does with a tool on tinygemma3's own
//! template, which calls none, and, from llama-server with the projector,
//! what a picture counts for in the prompt.
//!
//! The greedy reply is the one generated text either recording keeps, and
//! the two engines must stream it alike: the same visible pieces, as many
//! tokens, ending for the same reason. A token with no bytes streams no
//! piece, and neither engine's answer names it, so the count is what holds
//! it. The reply is asked without `ignore_eos`, so it ends where the model
//! ends it, or at the cap.

use serde_json::{Value, json};

use super::inproc::Upstream;
use super::launch::{CAT, GEMMA, GEMMA_PROJECTOR, Inputs, Launch, Server};
use super::project::{Criterion, Engine, Recording, redact_stream};
use super::readings::{CHAT, json_of, prompt_tokens, question};

/// The most tokens tinygemma3 streams for [`question`] at temperature 0.
const GREEDY_TOKENS: u64 = 16;

/// The readings of tinygemma3's greedy reply, each under
/// `greedy.tinygemma3.`, that the in-process engine must give as
/// llama-server gives them.
pub(super) const GREEDY: [&str; 4] = [
    "prompt_tokens",
    "visible_pieces",
    "completion_tokens",
    "finish_reason",
];

/// The redacted greedy stream, kept beside the recording's `stream.sse`: a
/// whole stream whose deltas carry `content`.
const CONTENT_STREAM: &str = "stream-content.sse";

static VISION: Criterion = Criterion {
    adr: "0015",
    quote: "If llama-server reads images without a projector, decision 1 is reopened.",
    how_read: "tinygemma3 started without --mmproj: /props modalities.vision",
    if_moved: "the server's own /props answer is read in place of the projector column",
};
static TOOLLESS: Criterion = Criterion {
    adr: "0007",
    quote: "If llama-server starts rendering `tools` to, or parsing tool calls from, a template whose caps say it calls none, the drop of `tools` for a model whose tool flag is off starts changing what the model sees, and is re-taken.",
    how_read: "tinygemma3 on its own template: /props chat_template_caps; one tool under tool_choice auto and required: the HTTP status, whether the message carries tool_calls, and whether prompt_tokens differs from the question without tools",
    if_moved: "the drop for a flag-off model now changes what the model sees: read the tool branch of common/chat-auto-parser-generator.cpp at the new pin and re-take it",
};
static IMAGE_TOKENS: Criterion = Criterion {
    adr: "0015",
    quote: "If the estimate is more than a tenth off what llama-server counts, decision 2 is reopened.",
    how_read: "tinygemma3 with its projector: usage.prompt_tokens with 0_cat.png less the same request without it",
    if_moved: "compare gglib's image_tokens estimate with the new count",
};

/// tinygemma3's launches, the pass's fifth and sixth: without its
/// projector, and with it on llama-server. The in-process engine reads no
/// projector, and says instead what it ran and how it matched.
pub(super) async fn read(upstream: Upstream<'_>, r: &mut Recording) {
    let gemma = |projector| Launch {
        model: GEMMA,
        tags: &[],
        template: None,
        projector,
        slots_debug: false,
    };
    let server = upstream.start(&gemma(None), 4).await;
    let props = server.get_json("/props").await;
    r.shape_of("/props", &props);
    let vision = props["modalities"]["vision"].clone();
    r.named("vision_without_mmproj", vision, &VISION);
    read_greedy(&server, r).await;
    // The in-process engine refuses a tool with a 400.
    if r.engine == Engine::LlamaServer {
        read_toolless(&server, &props, r).await;
    }
    drop(server);
    match upstream {
        Upstream::LlamaServer(inputs) => {
            let server = upstream.start(&gemma(Some(GEMMA_PROJECTOR)), 5).await;
            read_image(&server, inputs, r).await;
        }
        Upstream::InProcess(inputs) => inputs.read_own(&props, r),
    }
}

/// llama-server on a template that calls no tools, asked with one under
/// `auto` and `required`: its caps say so, and the tools reach the template,
/// which does not read them, and the reply is read as plain text. Asked
/// after the greedy reply, which the cache must not touch.
async fn read_toolless(server: &Server, props: &Value, r: &mut Recording) {
    let caps = &props["chat_template_caps"];
    for key in ["supports_tool_calls", "supports_tools"] {
        r.named(
            format!("caps.tinygemma3.{key}"),
            caps[key].clone(),
            &TOOLLESS,
        );
    }
    let without = prompt_tokens(server.post(CHAT, &question(None)).await);
    for choice in ["auto", "required"] {
        let (status, body) = server.post(CHAT, &question(Some(choice))).await;
        let name = |reading| format!("toolless.tinygemma3.{choice}.{reading}");
        r.named(name("status"), status, &TOOLLESS);
        if status != 200 {
            r.shape_of("error (tools on a tool-less template)", &json_of(&body));
            continue;
        }
        let answer = json_of(&body);
        r.shape_of("chat completion", &answer);
        let calls = &answer["choices"][0]["message"]["tool_calls"];
        r.named(name("tool_calls"), !calls.is_null(), &TOOLLESS);
        let tokens = answer["usage"]["prompt_tokens"].as_u64().expect(&body);
        r.named(name("tools_rendered"), tokens != without, &TOOLLESS);
    }
}

/// The reply to [`question`] at temperature 0, at most [`GREEDY_TOKENS`]:
/// its non-empty pieces, the prompt's and the reply's tokens, and its
/// `finish_reason`, as [`GREEDY`] names them. Its chunks carry the answer
/// as `delta.content`, and are projected with the others.
async fn read_greedy(server: &Server, r: &mut Recording) {
    let mut greedy = question(None);
    greedy["max_tokens"] = json!(GREEDY_TOKENS);
    greedy["ignore_eos"] = json!(false);
    greedy["stream"] = json!(true);
    greedy["stream_options"] = json!({"include_usage": true});
    let (status, body) = server.post(CHAT, &greedy).await;
    assert_eq!(status, 200, "{body}");
    let mut pieces = Vec::new();
    let (mut finish_reason, mut usage) = (Value::Null, Value::Null);
    for data in body.lines().filter_map(|l| l.strip_prefix("data: ")) {
        if !data.starts_with('{') {
            continue;
        }
        let frame = json_of(data);
        r.shape_of("stream chunk", &frame);
        let choice = &frame["choices"][0];
        let piece = choice["delta"]["content"].as_str();
        pieces.extend(piece.filter(|p| !p.is_empty()).map(str::to_owned));
        if choice["finish_reason"].is_string() {
            finish_reason = choice["finish_reason"].clone();
        }
        if frame["usage"].is_object() {
            usage = frame["usage"].clone();
        }
    }
    assert!(
        finish_reason.is_string() && usage["completion_tokens"].is_u64(),
        "no finish_reason or usage: {body}"
    );
    let name = |reading| format!("greedy.tinygemma3.{reading}");
    r.plain(name("visible_pieces"), pieces);
    r.plain(name("prompt_tokens"), usage["prompt_tokens"].clone());
    r.plain(
        name("completion_tokens"),
        usage["completion_tokens"].clone(),
    );
    r.plain(name("finish_reason"), finish_reason);
    r.bodies
        .insert(CONTENT_STREAM.to_owned(), redact_stream(&body));
}

/// tinygemma3 with its projector: llama-server reads the picture, and counts
/// it in the prompt.
async fn read_image(server: &Server, inputs: &Inputs, r: &mut Recording) {
    let props = server.get_json("/props").await;
    r.shape_of("/props", &props);
    r.plain("vision_with_mmproj", props["modalities"]["vision"].clone());
    let png = std::fs::read(inputs.models.join(CAT)).expect("the cat picture");
    let url = format!("data:image/png;base64,{}", base64_of(&png));
    let mut seen = question(None);
    seen["messages"][0]["content"] = json!([
        {"type": "text", "text": "What is this?"},
        {"type": "image_url", "image_url": {"url": url}},
    ]);
    let mut unseen = question(None);
    unseen["messages"][0]["content"] = json!([{"type": "text", "text": "What is this?"}]);
    let with = prompt_tokens(server.post(CHAT, &seen).await);
    let without = prompt_tokens(server.post(CHAT, &unseen).await);
    r.named("image_token_delta", with - without, &IMAGE_TOKENS);
}

fn base64_of(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
