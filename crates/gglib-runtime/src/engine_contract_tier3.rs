//! Tier 3: what a real small model does through the pinned llama-server.
//!
//! Record-only. On a llama.cpp bump the Engine pins workflow downloads
//! Qwen3.5-0.8B and its projector (`.github/actions/pinned-engines/
//! tier3-models.txt`) into `GGLIB_TIER3_MODEL_DIR`, with `0_cat.png` beside
//! them, and runs this test. It starts the model through gglib's own launch
//! path and writes one boolean per reading to
//! `contracts/upstream/llama/tier3/readings.json`, with a reason when one is
//! not true. No check reads that file and this test never fails: a probe
//! that cannot run reads `null`. The answers themselves go only to
//! `GGLIB_ENGINE_BODIES_DIR/tier3/` (the run's artifact): nothing the model
//! wrote is committed.

use std::path::{Path, PathBuf};
use std::time::Duration;

use gglib_core::domain::DialectSpec;
use gglib_core::domain::capability_tags::{AGENT, REASONING};
use gglib_core::normalize::normalize_chat_completion_body;
use gglib_core::request_pipeline::validate::{Verdict, validate_tool_calls};
use serde_json::{Value, json};

use super::launch::{CAT, Inputs, Launch, Server};
use super::project::write_under;

const QWEN: &str = "Qwen3.5-0.8B-Q8_0.gguf";
const QWEN_PROJECTOR: &str = "mmproj-F16.gguf";
/// The tags gglib's import reads from a Qwen3.5 template (it thinks, and calls
/// tools), which pick its launch flags.
const QWEN_TAGS: [&str; 2] = [AGENT, REASONING];
const CHAT: &str = "/v1/chat/completions";
/// Each request's limit; a CPU runner writes a few dozen tokens a second.
const PROBE_TIMEOUT: Duration = Duration::from_mins(3);

/// One probe's outcome: `Ok` is true, `Err` says why not.
type Probe = Result<(), String>;

fn tool(name: &str, description: &str, properties: &Value) -> Value {
    let required: Vec<&String> = properties
        .as_object()
        .map_or(vec![], |p| p.keys().collect());
    json!([{"type": "function", "function": {
        "name": name,
        "description": description,
        "parameters": {"type": "object", "properties": properties, "required": required},
    }}])
}

fn weather_tool() -> Value {
    tool(
        "get_weather",
        "Get the current weather in a city",
        &json!({"city": {"type": "string"}}),
    )
}

fn chat(messages: &Value, max_tokens: u64, thinking: bool) -> Value {
    json!({
        "messages": messages,
        "max_tokens": max_tokens,
        "temperature": 0,
        "seed": 1,
        "chat_template_kwargs": {"enable_thinking": thinking},
    })
}

fn message(body: &Value) -> &Value {
    &body["choices"][0]["message"]
}

/// llama-server returned the call as structured `tool_calls` itself, before
/// gglib's normaliser: the fact a decision to delete that normaliser needs.
fn tool_call_native(body: &Value) -> Probe {
    match message(body)["tool_calls"].as_array() {
        Some(calls) if !calls.is_empty() => Ok(()),
        _ => Err("no tool_calls in llama-server's answer".to_owned()),
    }
}

/// A `get_weather` call that gglib's parser reads and its validator accepts,
/// whether llama-server parsed it or left markup for gglib to. No reason
/// repeats what the model wrote.
fn tool_call_parsed(body: &Value, tools: &Value) -> Probe {
    let mut body = body.clone();
    let errors = normalize_chat_completion_body(&mut body, Some(&DialectSpec::qwen_xml()));
    let calls = &message(&body)["tool_calls"];
    let Some(first) = calls.as_array().and_then(|c| c.first()) else {
        let kept = errors.len();
        return Err(format!(
            "no tool call after normalizing ({kept} parser errors)"
        ));
    };
    if first["function"]["name"] != json!("get_weather") {
        return Err("the call names another function".to_owned());
    }
    match validate_tool_calls(Some(tools), Some(calls)) {
        Verdict::Valid => Ok(()),
        _ => Err("the arguments do not match the tool's schema".to_owned()),
    }
}

/// Thinking arrived in `reasoning_content`, and none of it in `content`.
fn reasoning_split(body: &Value) -> Probe {
    let message = message(body);
    let reasoning = message["reasoning_content"].as_str().unwrap_or_default();
    let content = message["content"].as_str().unwrap_or_default();
    if content.contains("<think>") || content.contains("</think>") {
        return Err("content carries a think tag".to_owned());
    }
    if reasoning.trim().is_empty() {
        return Err("reasoning_content is empty".to_owned());
    }
    Ok(())
}

/// The image in the tool message was read (more prompt tokens than the same
/// request without it) and the answer names what it shows, as a whole word.
fn image_answered(with: &Value, without: &Value) -> Probe {
    let tokens = |body: &Value| body["usage"]["prompt_tokens"].as_u64();
    let (Some(seen), Some(unseen)) = (tokens(with), tokens(without)) else {
        return Err("no usage.prompt_tokens".to_owned());
    };
    if seen <= unseen {
        return Err(format!(
            "{seen} prompt tokens with the image, {unseen} without"
        ));
    }
    let answer = message(with)["content"]
        .as_str()
        .unwrap_or_default()
        .to_lowercase();
    let named = answer
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| ["cat", "cats", "kitten", "kitty", "feline"].contains(&word));
    if !named {
        return Err("the answer does not name the cat".to_owned());
    }
    Ok(())
}

/// The readings file: each probe's value, `null` when it did not run, and a
/// reason beside anything not true.
fn readings(probes: &[(&str, Option<Probe>)], why_none: &str) -> Value {
    let mut out = json!({"model": QWEN, "projector": QWEN_PROJECTOR});
    for (name, probe) in probes {
        out[*name] = match probe {
            Some(Ok(())) => json!({"value": true}),
            Some(Err(reason)) => json!({"value": false, "reason": reason}),
            None => json!({"value": null, "reason": why_none}),
        };
    }
    out
}

const PROBES: [&str; 4] = [
    "tool_call_native",
    "tool_call_parsed",
    "reasoning_split",
    "image_in_tool_message_answered",
];

/// A 200 answer's JSON, kept in `bodies` as `name`.
async fn ask(server: &Server, body: &Value, name: &str, bodies: &Path) -> Result<Value, String> {
    let (status, text) = server
        .try_post(CHAT, body, PROBE_TIMEOUT)
        .await
        .map_err(|e| format!("no answer: {e}"))?;
    write_under(bodies, &format!("{name}.json"), &text);
    if status != 200 {
        return Err(format!("HTTP {status}"));
    }
    serde_json::from_str(&text).map_err(|e| format!("not JSON: {e}"))
}

/// The tool-call request, judged twice: as llama-server answered it, and
/// through gglib's parser.
async fn probe_tool(server: &Server, bodies: &Path) -> [Probe; 2] {
    let tools = weather_tool();
    let mut body = chat(
        &json!([{"role": "user", "content": "What is the weather in Paris right now?"}]),
        256,
        false,
    );
    body["tools"] = tools.clone();
    match ask(server, &body, "tool_call", bodies).await {
        Ok(answer) => [tool_call_native(&answer), tool_call_parsed(&answer, &tools)],
        Err(why) => [Err(why.clone()), Err(why)],
    }
}

async fn probe_reasoning(server: &Server, bodies: &Path) -> Probe {
    let question = json!([{"role": "user", "content": "What is 17 + 25? Answer with the number."}]);
    reasoning_split(&ask(server, &chat(&question, 1024, true), "reasoning", bodies).await?)
}

async fn probe_image(server: &Server, inputs: &Inputs, bodies: &Path) -> Probe {
    let png = std::fs::read(inputs.models.join(CAT)).map_err(|e| format!("{CAT}: {e}"))?;
    let url = format!("data:image/png;base64,{}", base64_of(&png));
    let conversation = |parts: Value| {
        let mut body = chat(
            &json!([
                {"role": "user", "content": "Call get_picture, then say in one sentence what the picture shows."},
                {"role": "assistant", "content": "", "tool_calls": [{"id": "call_1", "type": "function",
                    "function": {"name": "get_picture", "arguments": "{}"}}]},
                {"role": "tool", "tool_call_id": "call_1", "content": parts},
            ]),
            128,
            false,
        );
        body["tools"] = tool("get_picture", "Show the picture", &json!({}));
        body
    };
    let text = json!({"type": "text", "text": "Here is the picture."});
    let image = json!({"type": "image_url", "image_url": {"url": url}});
    let with = ask(
        server,
        &conversation(json!([text, image])),
        "image_in_tool",
        bodies,
    )
    .await?;
    let without = ask(
        server,
        &conversation(json!([text])),
        "image_in_tool_without",
        bodies,
    )
    .await?;
    image_answered(&with, &without)
}

/// Every probe on one launch. Panics only if llama-server does not start.
async fn run(inputs: Inputs, bodies: PathBuf) -> [Option<Probe>; 4] {
    let launch = Launch {
        model: QWEN,
        tags: &QWEN_TAGS,
        template: None,
        projector: Some(QWEN_PROJECTOR),
        slots_debug: false,
    };
    let server = Server::start(&inputs, &launch, 0).await;
    let [native, parsed] = probe_tool(&server, &bodies).await;
    [
        Some(native),
        Some(parsed),
        Some(probe_reasoning(&server, &bodies).await),
        Some(probe_image(&server, &inputs, &bodies).await),
    ]
}

fn base64_of(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// The first line of a panic's message, which is all a reason keeps.
fn panic_line(error: tokio::task::JoinError) -> String {
    let Ok(payload) = error.try_into_panic() else {
        return "the probes were cancelled".to_owned();
    };
    let text = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_default();
    let line = text
        .lines()
        .next()
        .unwrap_or_default()
        .trim_end_matches(':');
    let line = if line.is_empty() {
        "a probe panicked"
    } else {
        line
    };
    line.chars().take(200).collect()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts the pinned llama-server on Qwen3.5-0.8B; the Engine pins workflow sets GGLIB_TIER3_MODEL_DIR"]
async fn a_small_real_model_through_the_pinned_llama_server() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let Some(inputs) = Inputs::with_models("GGLIB_TIER3_MODEL_DIR", scratch.path()) else {
        eprintln!("GGLIB_TIER3_MODEL_DIR is unset: no tier-3 probes");
        return;
    };
    let bodies = std::env::var_os("GGLIB_ENGINE_BODIES_DIR")
        .map_or_else(|| inputs.scratch_path("bodies"), PathBuf::from)
        .join("tier3");
    let (results, why_none) = match tokio::spawn(run(inputs, bodies)).await {
        Ok(results) => (results, String::new()),
        Err(error) => ([None, None, None, None], panic_line(error)),
    };
    let probes: Vec<(&str, Option<Probe>)> = PROBES.into_iter().zip(results).collect();
    let text = serde_json::to_string_pretty(&readings(&probes, &why_none)).expect("JSON") + "\n";
    eprint!("{text}");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/upstream");
    write_under(&root, "llama/tier3/readings.json", &text);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(message: &Value) -> Value {
        json!({"choices": [{"message": message, "finish_reason": "stop"}],
               "usage": {"prompt_tokens": 40}})
    }

    #[test]
    fn a_native_tool_call_is_native_and_parsed() {
        let body = answer(
            &json!({"content": null, "tool_calls": [{"id": "1", "type": "function",
            "function": {"name": "get_weather", "arguments": "{\"city\":\"Paris\"}"}}]}),
        );
        assert_eq!(tool_call_native(&body), Ok(()));
        assert_eq!(tool_call_parsed(&body, &weather_tool()), Ok(()));
    }

    #[test]
    fn tool_call_markup_left_in_content_is_parsed_by_gglib_but_not_native() {
        let body = answer(&json!({"content":
            "<tool_call>\n{\"name\": \"get_weather\", \"arguments\": {\"city\": \"Paris\"}}\n</tool_call>"}));
        assert_eq!(
            tool_call_native(&body),
            Err("no tool_calls in llama-server's answer".to_owned())
        );
        assert_eq!(tool_call_parsed(&body, &weather_tool()), Ok(()));
        let empty = answer(&json!({"content": "Sunny.", "tool_calls": []}));
        assert!(tool_call_native(&empty).is_err());
    }

    #[test]
    fn plain_text_or_a_bad_call_is_not_a_parsed_tool_call() {
        let prose = answer(&json!({"content": "It is sunny in Paris."}));
        assert!(
            tool_call_parsed(&prose, &weather_tool())
                .unwrap_err()
                .starts_with("no tool call")
        );
        let wrong = answer(
            &json!({"content": null, "tool_calls": [{"id": "1", "type": "function",
            "function": {"name": "get_weather", "arguments": "{\"town\":\"Paris\"}"}}]}),
        );
        assert_eq!(
            tool_call_parsed(&wrong, &weather_tool()),
            Err("the arguments do not match the tool's schema".to_owned())
        );
        let other = answer(
            &json!({"content": null, "tool_calls": [{"id": "1", "type": "function",
            "function": {"name": "get_time", "arguments": "{}"}}]}),
        );
        assert_eq!(
            tool_call_parsed(&other, &weather_tool()),
            Err("the call names another function".to_owned())
        );
    }

    #[test]
    fn reasoning_is_split_only_when_it_is_in_its_own_field() {
        let split = answer(&json!({"reasoning_content": "17 and 25", "content": "42"}));
        assert_eq!(reasoning_split(&split), Ok(()));
        let inline = answer(&json!({"content": "<think>17 and 25</think>42"}));
        assert_eq!(
            reasoning_split(&inline),
            Err("content carries a think tag".to_owned())
        );
        let leaked = answer(&json!({"reasoning_content": "17", "content": "17 and 25</think>42"}));
        assert_eq!(
            reasoning_split(&leaked),
            Err("content carries a think tag".to_owned())
        );
        let opened =
            answer(&json!({"reasoning_content": "17", "content": "<think>17 and 25 is 42"}));
        assert_eq!(
            reasoning_split(&opened),
            Err("content carries a think tag".to_owned())
        );
        let blank = answer(&json!({"reasoning_content": " \n ", "content": "42"}));
        assert_eq!(
            reasoning_split(&blank),
            Err("reasoning_content is empty".to_owned())
        );
        let none = answer(&json!({"content": "42"}));
        assert_eq!(
            reasoning_split(&none),
            Err("reasoning_content is empty".to_owned())
        );
    }

    #[test]
    fn an_image_answer_needs_the_image_counted_and_named() {
        let with = |tokens: u64, text: &str| {
            json!({"choices": [{"message": {"content": text}}],
                                                     "usage": {"prompt_tokens": tokens}})
        };
        assert_eq!(
            image_answered(&with(300, "A Cat on a sofa."), &with(40, "?")),
            Ok(())
        );
        assert_eq!(
            image_answered(&with(40, "A cat."), &with(40, "?")),
            Err("40 prompt tokens with the image, 40 without".to_owned())
        );
        assert_eq!(
            image_answered(&with(300, "Two cats, asleep."), &with(40, "?")),
            Ok(())
        );
        for answer in ["A dog.", "The picture shows a location", "A catalogue."] {
            assert_eq!(
                image_answered(&with(300, answer), &with(40, "?")),
                Err("the answer does not name the cat".to_owned()),
                "{answer}"
            );
        }
        assert_eq!(
            image_answered(&json!({}), &with(40, "?")),
            Err("no usage.prompt_tokens".to_owned())
        );
    }

    #[test]
    fn readings_say_why_a_probe_is_not_true() {
        let probes = [
            ("a", Some(Ok(()))),
            ("b", Some(Err("HTTP 500".to_owned()))),
            ("c", None),
        ];
        let out = readings(&probes, "llama-server exited (exit status: 1):");
        assert_eq!(out["a"], json!({"value": true}));
        assert_eq!(out["b"], json!({"value": false, "reason": "HTTP 500"}));
        assert_eq!(
            out["c"],
            json!({"value": null, "reason": "llama-server exited (exit status: 1):"})
        );
        assert_eq!(out["model"], json!(QWEN));
    }
}
