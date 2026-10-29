//! What the recorder asks tinygemma3: whether the engine reads images
//! without its projector, the reply it streams to [`question`] at
//! temperature 0, and, from llama-server with the projector, what a picture
//! counts for in the prompt.
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
use super::project::{Criterion, Recording, redact_stream};
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
    drop(server);
    match upstream {
        Upstream::LlamaServer(inputs) => {
            let server = upstream.start(&gemma(Some(GEMMA_PROJECTOR)), 5).await;
            read_image(&server, inputs, r).await;
        }
        Upstream::InProcess(inputs) => inputs.read_own(&props, r),
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
