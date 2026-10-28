//! What the recorder asks the pinned llama-server, and which ADR each
//! answer is a reading for.
//!
//! One pass starts every launch fresh: each dateless template on stories260K
//! with `/slots` debug on, rendering one fixed chat with thinking on and off,
//! the first of them also for the endpoints gglib reads; stories260K again
//! with debug off, for the `cache_prompt` pair and the `/slots` gglib's
//! poller sees; and tinygemma3 without and with its projector.

use serde_json::{Value, json};

use super::launch::{CAT, CONTEXT, GEMMA, GEMMA_PROJECTOR, Inputs, Launch, STORIES, Server};
use super::project::{Criterion, Recording, as_f32, help_default, redact_stream};
use crate::pinned_engines_tests::help_options;

/// The templates without a date in them, tried on stories260K.
const TEMPLATES: [&str; 3] = ["Qwen3.5-4B", "Qwen-Qwen3-0.6B", "ibm-granite-granite-4.0"];
const CHAT: &str = "/v1/chat/completions";

static CONSTRAIN: Criterion = Criterion {
    adr: "0002",
    quote: "upstream constrains dialect tool calls under both `tool_choice: \"required\"` and `\"auto\"`, with arguments conforming to the tool's own JSON Schema rather than merely being well-formed JSON",
    how_read: "stories260K under the template, one tool, tool_choice as named; /slots with LLAMA_SERVER_SLOTS_DEBUG=1 once idle: grammar non-empty, grammar_lazy, grammar_triggers non-empty",
    if_moved: "the decision on constrain.rs is re-taken",
};
static SAMPLER: Criterion = Criterion {
    adr: "0003",
    quote: "If a pin bump moves an upstream default that gglib now defers to, the readback flags the divergence and this decision is re-taken for that parameter.",
    how_read: "/props default_generation_settings.params, bare launch, as the f32 llama.cpp holds",
    if_moved: "UPSTREAM_DEFAULTS in gglib-proxy's props.rs moves with it, and the deferral is re-taken",
};
static TRANSPORT: Criterion = Criterion {
    adr: "0012",
    quote: "If `llama-server` — or another dependency gglib fundamentally relies on — ships a first-party remote transport, this defers to it and the tunnel is deleted rather than carried beside it.",
    how_read: "the number of options llama-server --help lists; read help.txt's diff for a transport",
    if_moved: "read the help.txt diff for a remote transport",
};
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
static CACHE_PROMPT: Criterion = Criterion {
    adr: "0022",
    quote: "A pin bump that changes either default named in the context (`server-task.h:54`, `common/common.h:635`) away from `true`",
    how_read: "stories260K, one request sent twice with no cache_prompt: the second's prompt_tokens_details.cached_tokens > 0",
    if_moved: "decision 1 is reversed: gglib sends cache_prompt itself",
};

/// A one-line question, with the weather tool under `choice` if there is one.
fn question(choice: Option<&str>) -> Value {
    let mut body = json!({
        "messages": [{"role": "user", "content": "What is the weather in Paris?"}],
        "max_tokens": 4, "ignore_eos": true,
        "temperature": 0,
        "seed": 1,
    });
    if let Some(choice) = choice {
        body["tools"] = json!([{"type": "function", "function": {
            "name": "get_weather",
            "description": "Get the weather",
            "parameters": {"type": "object", "properties": {"city": {"type": "string"}}, "required": ["city"]},
        }}]);
        body["tool_choice"] = json!(choice);
    }
    body
}

/// The chat every template renders through `/apply-template`: a system
/// message, a user turn, the assistant's reply and a second user turn, with
/// the turn the model would answer opened, and thinking on or off.
fn fixed_chat(thinking: bool) -> Value {
    json!({
        "messages": [
            {"role": "system", "content": "You answer in one short sentence."},
            {"role": "user", "content": "What is the capital of France?"},
            {"role": "assistant", "content": "The capital of France is Paris."},
            {"role": "user", "content": "And of Italy?"},
        ],
        "add_generation_prompt": true,
        "chat_template_kwargs": {"enable_thinking": thinking},
    })
}

/// A 200 body's `usage.prompt_tokens`.
fn prompt_tokens((status, body): (u16, String)) -> u64 {
    assert_eq!(status, 200, "{body}");
    let body: Value = serde_json::from_str(&body).expect("JSON");
    body["usage"]["prompt_tokens"]
        .as_u64()
        .expect("prompt_tokens")
}

fn json_of(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|e| panic!("{e}: {body}"))
}

/// `--help` and `--version` of both servers.
fn engines_said(inputs: &Inputs, r: &mut Recording) {
    let (help, _) = inputs.run(&inputs.llama_server, "--help");
    let options = help_options(&help).len();
    assert!(options > 100, "llama-server --help lists {options} options");
    r.named("help_diff.options", options, &TRANSPORT);
    for (name, flag) in [
        ("batch", "--batch-size"),
        ("fit", "--fit"),
        ("parallel", "--parallel"),
        ("jinja", "--jinja"),
    ] {
        let default = help_default(&help, flag).unwrap_or_else(|| panic!("no default for {flag}"));
        r.plain(format!("help_defaults.{name}"), default);
    }
    r.texts.insert("llama/help.txt", help);
    let (out, err) = inputs.run(&inputs.llama_server, "--version");
    let all = format!("{out}{err}");
    let line = all.lines().find(|l| l.starts_with("version:"));
    r.texts
        .insert("llama/version.txt", format!("{}\n", line.expect(&all)));
    let (help, _) = inputs.run(&inputs.sd_server, "--help");
    assert!(help.contains("--listen-port"), "{help}");
    r.texts.insert("sd/help.txt", help);
    let (out, err) = inputs.run(&inputs.sd_server, "--version");
    let all = format!("{out}{err}");
    let line = all.lines().find(|l| !l.trim().is_empty());
    r.texts.insert(
        "sd/version.txt",
        format!("{}\n", line.expect("sd-server --version")),
    );
}

/// One template's `/props`, what it does with a tool under each choice, and
/// the prompt it renders for [`fixed_chat`].
async fn read_template(server: &Server, template: &str, r: &mut Recording) {
    let props = server.get_json("/props").await;
    r.shape_of("/props", &props);
    r.body(&format!("props.{template}.json"), &props);
    assert_eq!(
        props["default_generation_settings"]["n_ctx"],
        json!(CONTEXT)
    );
    for (key, value) in props["chat_template_caps"].as_object().expect("caps") {
        r.plain(format!("caps.{template}.{key}"), value.clone());
    }
    for choice in ["auto", "required", "none"] {
        let (status, body) = server.post(CHAT, &question(Some(choice))).await;
        if status == 200 {
            r.shape_of("chat completion", &json_of(&body));
        }
        let (slots, _) = server.idle_slots().await;
        let params = &slots[0]["params"];
        let grammar = params["grammar"].as_str().is_some_and(|g| !g.is_empty());
        let lazy = params["grammar_lazy"] == json!(true);
        let triggers = params["grammar_triggers"]
            .as_array()
            .is_some_and(|t| !t.is_empty());
        let name = format!("{choice}_grammar.{template}");
        if choice == "none" {
            r.plain(format!("{name}.grammar"), grammar);
            continue;
        }
        r.named(format!("{name}.grammar"), grammar, &CONSTRAIN);
        r.named(format!("{name}.lazy"), lazy, &CONSTRAIN);
        r.named(format!("{name}.triggers_present"), triggers, &CONSTRAIN);
        if choice == "auto" {
            r.shape_of("/slots (debug)", &slots);
            r.plain(
                format!("slots_chat_format.{template}"),
                params["chat_format"].clone(),
            );
        }
    }
    let with_tools = prompt_tokens(server.post(CHAT, &question(Some("none"))).await);
    let without = prompt_tokens(server.post(CHAT, &question(None)).await);
    r.plain(
        format!("tools_rendered_under_none.{template}"),
        with_tools > without,
    );
    for (variant, thinking) in [("thinking", true), ("no-thinking", false)] {
        let (status, body) = server.post("/apply-template", &fixed_chat(thinking)).await;
        assert_eq!(status, 200, "{body}");
        let prompt = json_of(&body)["prompt"].as_str().map(str::to_owned);
        r.prompts.insert(
            format!("apply-template.{template}.{variant}.txt"),
            prompt.unwrap_or_else(|| panic!("no prompt: {body}")),
        );
    }
}

/// The first template's launch also answers for the endpoints gglib reads.
async fn read_endpoints(server: &Server, r: &mut Recording) {
    let props = server.get_json("/props").await;
    let params = &props["default_generation_settings"]["params"];
    for field in [
        "temperature",
        "top_p",
        "top_k",
        "repeat_penalty",
        "presence_penalty",
        "min_p",
        "dry_multiplier",
    ] {
        r.named(
            format!("sampler_defaults.{field}"),
            as_f32(&params[field]),
            &SAMPLER,
        );
    }
    r.shape_of("/health", &server.get_json("/health").await);
    let mut stream = question(None);
    stream["stream"] = json!(true);
    stream["return_progress"] = json!(true);
    stream["stream_options"] = json!({"include_usage": true});
    let (status, body) = server.post(CHAT, &stream).await;
    assert_eq!(status, 200, "{body}");
    for data in body.lines().filter_map(|l| l.strip_prefix("data: ")) {
        if data.starts_with('{') {
            r.shape_of("stream chunk", &json_of(data));
        }
    }
    r.bodies
        .insert("stream.sse".to_owned(), redact_stream(&body));
    let mut long = question(None);
    long["messages"][0]["content"] = json!("tell me a story ".repeat(1000));
    let (status, body) = server.post(CHAT, &long).await;
    r.shape_of("error (over context)", &json_of(&body));
    r.plain("over_context.status", status);
    let mut effort = question(None);
    effort["reasoning_effort"] = json!("banana");
    r.plain(
        "effort_validated",
        server.post(CHAT, &effort).await.0 == 400,
    );
    let messages = json!({"messages": [{"role": "user", "content": "hi"}]});
    for (name, path, request) in [
        ("tokenize", "/tokenize", json!({"content": "hi"})),
        ("apply-template", "/apply-template", messages.clone()),
        (
            "input_tokens",
            "/v1/chat/completions/input_tokens",
            messages,
        ),
    ] {
        let (status, body) = server.post(path, &request).await;
        r.plain(format!("endpoints_present.{name}"), status == 200);
        if status == 200 {
            r.shape_of(path, &json_of(&body));
        }
    }
}

/// One whole pass, every launch fresh.
pub(super) async fn record(inputs: &Inputs) -> Recording {
    let mut r = Recording::default();
    engines_said(inputs, &mut r);
    let stories = |template, slots_debug| Launch {
        model: STORIES,
        tags: &[],
        template: Some(template),
        projector: None,
        slots_debug,
    };
    for (n, template) in TEMPLATES.into_iter().enumerate() {
        let server = Server::start(inputs, &stories(template, true), n).await;
        read_template(&server, template, &mut r).await;
        if n == 0 {
            read_endpoints(&server, &mut r).await;
        }
    }
    // The cache pair, and `/slots` as gglib's poller reads it: no debug.
    let server = Server::start(inputs, &stories(TEMPLATES[0], false), 3).await;
    let twice = question(None);
    server.post(CHAT, &twice).await;
    let (_, second) = server.post(CHAT, &twice).await;
    let cached = &json_of(&second)["usage"]["prompt_tokens_details"]["cached_tokens"];
    r.named(
        "cache_prompt_default",
        cached.as_u64().expect(&second) > 0,
        &CACHE_PROMPT,
    );
    let (slots, _) = server.idle_slots().await;
    assert_eq!(slots[0]["n_ctx"], json!(CONTEXT));
    r.shape_of("/slots", &slots);
    r.body("slots.json", &slots);
    drop(server);
    for projector in [false, true] {
        let gemma = Launch {
            model: GEMMA,
            tags: &[],
            template: None,
            projector: projector.then_some(GEMMA_PROJECTOR),
            slots_debug: false,
        };
        let server = Server::start(inputs, &gemma, 4 + usize::from(projector)).await;
        let props = server.get_json("/props").await;
        r.shape_of("/props", &props);
        let vision = props["modalities"]["vision"].clone();
        if !projector {
            r.named("vision_without_mmproj", vision, &VISION);
            continue;
        }
        r.plain("vision_with_mmproj", vision);
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
    r
}

fn base64_of(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
