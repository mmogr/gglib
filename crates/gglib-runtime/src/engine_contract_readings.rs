//! What the recorder asks an engine, and which ADR each answer is a reading
//! for when the engine is the pinned llama-server.
//!
//! One pass starts every launch fresh: each dateless template on stories260K
//! with `/slots` debug on, rendering one fixed chat with thinking on and off,
//! the first of them also for the endpoints gglib reads; stories260K again
//! with debug off, for the `cache_prompt` pair and the `/slots` gglib's
//! poller sees; and tinygemma3 without its projector, streaming a reply at
//! temperature 0, then with it (`engine_contract_gemma.rs`). The in-process
//! engine is asked the same, less what it refuses or does not serve: tools,
//! `/slots` (a 501, read as such), a projector, and `--help`.

use serde_json::{Value, json};

use super::gemma;
use super::inproc::Upstream;
use super::launch::{CONTEXT, Inputs, Launch, STORIES, Server};
use super::project::{Criterion, Engine, Recording, as_f32, help_default, redact_stream};
use crate::pinned_engines_tests::help_options;

/// The templates without a date in them, tried on stories260K.
pub(super) const TEMPLATES: [&str; 3] =
    ["Qwen3.5-4B", "Qwen-Qwen3-0.6B", "ibm-granite-granite-4.0"];
/// Each template renders [`fixed_chat`] with thinking on and off.
pub(super) const RENDERS: [(&str, bool); 2] = [("thinking", true), ("no-thinking", false)];
/// Where every question is asked.
pub(super) const CHAT: &str = "/v1/chat/completions";

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
static CACHE_PROMPT: Criterion = Criterion {
    adr: "0022",
    quote: "A pin bump that changes either default named in the context (`server-task.h:54`, `common/common.h:635`) away from `true`",
    how_read: "stories260K, one request sent twice with no cache_prompt: the second's prompt_tokens_details.cached_tokens > 0",
    if_moved: "decision 1 is reversed: gglib sends cache_prompt itself",
};

/// A one-line question, with the weather tool under `choice` if there is one.
pub(super) fn question(choice: Option<&str>) -> Value {
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
pub(super) fn prompt_tokens((status, body): (u16, String)) -> u64 {
    assert_eq!(status, 200, "{body}");
    let body: Value = serde_json::from_str(&body).expect("JSON");
    body["usage"]["prompt_tokens"]
        .as_u64()
        .expect("prompt_tokens")
}

/// `body` as JSON, which it must be.
pub(super) fn json_of(body: &str) -> Value {
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
    r.texts.insert("llama/help.txt".into(), help);
    let (out, err) = inputs.run(&inputs.llama_server, "--version");
    let all = format!("{out}{err}");
    let line = all.lines().find(|l| l.starts_with("version:"));
    r.texts.insert(
        "llama/version.txt".into(),
        format!("{}\n", line.expect(&all)),
    );
    let (help, _) = inputs.run(&inputs.sd_server, "--help");
    assert!(help.contains("--listen-port"), "{help}");
    r.texts.insert("sd/help.txt".into(), help);
    let (out, err) = inputs.run(&inputs.sd_server, "--version");
    let all = format!("{out}{err}");
    let line = all.lines().find(|l| !l.trim().is_empty());
    r.texts.insert(
        "sd/version.txt".into(),
        format!("{}\n", line.expect("sd-server --version")),
    );
}

/// One template's `/props`, the answer to a plain question, what
/// llama-server does with a tool under each choice, and the prompt it renders
/// for [`fixed_chat`].
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
    let (status, body) = server.post(CHAT, &question(None)).await;
    assert_eq!(status, 200, "{body}");
    let answer = json_of(&body);
    r.shape_of("chat completion", &answer);
    // The in-process engine refuses a tool with a 400, and has no `/slots`.
    if r.engine == Engine::LlamaServer {
        read_tools(server, template, r).await;
        let without = answer["usage"]["prompt_tokens"].as_u64().expect(&body);
        let with_tools = prompt_tokens(server.post(CHAT, &question(Some("none"))).await);
        r.plain(
            format!("tools_rendered_under_none.{template}"),
            with_tools > without,
        );
    }
    for (variant, thinking) in RENDERS {
        let (status, body) = server.post("/apply-template", &fixed_chat(thinking)).await;
        assert_eq!(status, 200, "{body}");
        let prompt = json_of(&body)["prompt"].as_str().map(str::to_owned);
        r.prompts.insert(
            format!("apply-template.{template}.{variant}.txt"),
            prompt.unwrap_or_else(|| panic!("no prompt: {body}")),
        );
    }
}

/// What llama-server does with a tool under each choice, as `/slots` shows
/// the task it last ran.
async fn read_tools(server: &Server, template: &str, r: &mut Recording) {
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

/// `/slots` as gglib's poller reads it: llama-server's one slot, or the
/// in-process engine's 501, which stops the poller for good.
async fn read_slots(server: &Server, r: &mut Recording) {
    if r.engine == Engine::InProcess {
        r.plain("slots_status", server.get("/slots").await.0);
        return;
    }
    let (slots, _) = server.idle_slots().await;
    assert_eq!(slots[0]["n_ctx"], json!(CONTEXT));
    r.shape_of("/slots", &slots);
    r.body("slots.json", &slots);
}

/// One whole pass of `upstream`, every launch fresh.
pub(super) async fn record(upstream: Upstream<'_>) -> Recording {
    let mut r = Recording::of(upstream.engine());
    if let Upstream::LlamaServer(inputs) = upstream {
        engines_said(inputs, &mut r);
    }
    let stories = |template, slots_debug| Launch {
        model: STORIES,
        tags: &[],
        template: Some(template),
        projector: None,
        slots_debug,
    };
    for (n, template) in TEMPLATES.into_iter().enumerate() {
        let server = upstream.start(&stories(template, true), n).await;
        read_template(&server, template, &mut r).await;
        if n == 0 {
            read_endpoints(&server, &mut r).await;
        }
    }
    // The cache pair, and `/slots` as gglib's poller reads it: no debug.
    let server = upstream.start(&stories(TEMPLATES[0], false), 3).await;
    let twice = question(None);
    server.post(CHAT, &twice).await;
    let (_, second) = server.post(CHAT, &twice).await;
    let cached = &json_of(&second)["usage"]["prompt_tokens_details"]["cached_tokens"];
    r.named(
        "cache_prompt_default",
        cached.as_u64().expect(&second) > 0,
        &CACHE_PROMPT,
    );
    read_slots(&server, &mut r).await;
    drop(server);
    gemma::read(upstream, &mut r).await;
    r
}
