//! Each input llama-server gives a template, one at a time.

use chrono::{DateTime, Local, TimeZone};
use serde_json::json;

use super::tojson::float_repr;
use super::{
    ChatKwargs, ChatMessage, ChatTemplate, RenderError, VocabTokens, env, render_at, render_chat,
};

/// `<s>` and `</s>`, neither added by the tokenizer.
fn vocab() -> VocabTokens {
    VocabTokens {
        bos: "<s>".to_owned(),
        eos: "</s>".to_owned(),
        add_bos: false,
        add_eos: false,
    }
}

fn template(source: &str) -> ChatTemplate {
    ChatTemplate::new(source, vocab()).expect("compiles")
}

fn kwargs(body: &serde_json::Value) -> ChatKwargs {
    ChatKwargs::from_request(body).expect("valid")
}

fn noon() -> DateTime<Local> {
    Local.with_ymd_and_hms(2026, 3, 7, 12, 0, 0).unwrap()
}

/// `source` over one user turn, at noon on 7 March 2026.
fn render(source: &str, add_generation_prompt: bool, body: &serde_json::Value) -> String {
    let user = [ChatMessage::new("user", "hi")];
    render_at(
        &template(source),
        &user,
        add_generation_prompt,
        &kwargs(body),
        noon(),
    )
    .unwrap()
}

/// A boolean prints as Python prints it, `True` or `False`, in llama's
/// engine (`common/jinja/value.h`, `value_bool_t::as_string`) as here.
#[test]
fn add_generation_prompt_is_given_only_when_true() {
    let t = "{{ add_generation_prompt is defined }}";
    assert_eq!(render(t, true, &json!({})), "True");
    assert_eq!(render(t, false, &json!({})), "False");
}

#[test]
fn no_tools_are_given() {
    assert_eq!(render("{{ tools is defined }}", true, &json!({})), "False");
}

#[test]
fn bos_and_eos_come_from_the_vocabulary_and_no_kwarg_replaces_them() {
    let body = json!({"chat_template_kwargs": {"bos_token": "X", "eos_token": "Y"}});
    assert_eq!(
        render("{{ bos_token }}|{{ eos_token }}", true, &body),
        "<s>|</s>"
    );
}

#[test]
fn a_bos_or_eos_the_tokenizer_adds_is_dropped_from_the_prompt() {
    let source = "{{ bos_token }}{{ messages[0].content }}{{ eos_token }}";
    let user = [ChatMessage::new("user", "hi")];
    let rendered = |vocab: VocabTokens| {
        let t = ChatTemplate::new(source, vocab).unwrap();
        render_chat(&t, &user, false, &ChatKwargs::default()).unwrap()
    };
    assert_eq!(rendered(vocab()), "<s>hi</s>");
    let bos_added = VocabTokens {
        add_bos: true,
        ..vocab()
    };
    assert_eq!(rendered(bos_added), "hi</s>");
    let eos_appended = VocabTokens {
        add_eos: true,
        ..vocab()
    };
    assert_eq!(rendered(eos_appended), "<s>hi");
}

#[test]
fn thinking_is_on_unless_the_request_turns_it_off() {
    let t = "{{ enable_thinking }}";
    assert_eq!(render(t, true, &json!({})), "True");
    let off = json!({"chat_template_kwargs": {"enable_thinking": false}});
    assert_eq!(render(t, true, &off), "False");
    assert_eq!(
        render(t, true, &json!({"reasoning_effort": "none"})),
        "False"
    );
}

#[test]
fn a_string_enable_thinking_is_refused_as_llama_server_refuses_it() {
    let body = json!({"chat_template_kwargs": {"enable_thinking": "false"}});
    assert!(matches!(
        ChatKwargs::from_request(&body),
        Err(RenderError::Request(_))
    ));
}

#[test]
fn a_budget_of_zero_turns_thinking_off_over_an_explicit_kwarg() {
    let body = json!({
        "chat_template_kwargs": {"enable_thinking": true},
        "reasoning_budget_tokens": 0,
    });
    assert_eq!(render("{{ enable_thinking }}", true, &body), "False");
    let alias = json!({"thinking_budget_tokens": 0});
    assert!(!kwargs(&alias).enable_thinking());
    let null = json!({"reasoning_budget_tokens": null, "thinking_budget_tokens": 0});
    assert!(!kwargs(&null).enable_thinking());
    let some = json!({"reasoning_budget_tokens": 512});
    assert!(kwargs(&some).enable_thinking());
}

#[test]
fn reasoning_effort_reaches_the_template_under_both_names() {
    let t = "{{ reasoning_effort }}/{{ reasoning_strength }}";
    assert_eq!(
        render(t, true, &json!({"reasoning_effort": "high"})),
        "high/high"
    );
    let none =
        json!({"reasoning_effort": "none", "chat_template_kwargs": {"reasoning_effort": "low"}});
    assert_eq!(
        render("{{ reasoning_effort is defined }}", true, &none),
        "False"
    );
}

#[test]
fn preserve_reasoning_is_on_by_default_and_sets_its_four_variables() {
    let t = "{{ preserve_reasoning }} {{ preserve_thinking }} {{ clear_thinking }} \
             {{ truncate_history_thinking }} {{ drop_thinking }}";
    assert_eq!(render(t, true, &json!({})), "True True False False False");
    let off = json!({"chat_template_kwargs": {"preserve_reasoning": false}});
    assert_eq!(render(t, true, &off), "False False True True True");
}

#[test]
fn a_kwarg_reaches_the_template_as_a_variable_of_its_name() {
    let body = json!({"chat_template_kwargs": {"custom": {"depth": 2}}});
    assert_eq!(render("{{ custom.depth }}", true, &body), "2");
}

#[test]
fn the_dates_are_the_local_time_of_the_render() {
    let t = "{{ datetime }}|{{ date_string }}|{{ strftime_now('%Y-%m-%d %H:%M') }}";
    assert_eq!(
        render(t, true, &json!({})),
        "Mar 07 2026|07 Mar 2026|2026-03-07 12:00"
    );
}

#[test]
fn strftime_now_fails_where_llamas_strftime_fails() {
    assert!(env::strftime(noon(), "").is_err());
    assert!(env::strftime(noon(), &"%Y".repeat(25)).is_err());
    assert_eq!(env::strftime(noon(), "%d %b").unwrap(), "07 Mar");
}

#[test]
fn raise_exception_fails_the_render_with_its_message() {
    let t = template("{{ raise_exception('No user query found in messages.') }}");
    let err = render_chat(&t, &[], true, &ChatKwargs::default()).unwrap_err();
    assert!(
        matches!(&err, RenderError::Template(why) if why.contains("No user query found")),
        "{err}"
    );
}

#[test]
fn messages_are_shaped_as_llama_server_shapes_them() {
    let t = template(
        "{% for m in messages %}{{ m.role }}:{{ m.content }}:{{ m.reasoning_content is defined }};{% endfor %}",
    );
    let messages = [
        ChatMessage::new("developer", "be brief"),
        ChatMessage {
            reasoning_content: Some(String::new()),
            ..ChatMessage::new("assistant", "")
        },
        ChatMessage {
            reasoning_content: Some("thought".to_owned()),
            ..ChatMessage::new("assistant", "said")
        },
    ];
    let rendered = render_chat(&t, &messages, false, &ChatKwargs::default()).unwrap();
    assert_eq!(
        rendered,
        "system:be brief:False;assistant::False;assistant:said:True;"
    );
    let channel = template("{# <|channel|> #}{{ messages[0].role }}");
    let rendered = render_chat(&channel, &messages[..1], false, &ChatKwargs::default()).unwrap();
    assert_eq!(rendered, "developer");
}

#[test]
fn the_templates_llama_server_trims_messages_for_get_them_trimmed() {
    let t = template(
        "{# You have access to the following functions in JSONSchema format #}[{{ messages[0].content }}]",
    );
    let messages = [ChatMessage::new("user", " \n hi \t")];
    let rendered = render_chat(&t, &messages, false, &ChatKwargs::default()).unwrap();
    assert_eq!(rendered, "[hi]");
}

#[test]
fn a_model_without_a_template_renders_chatml() {
    assert_eq!(
        render("", true, &json!({})),
        "<|im_start|>user\nhi<|im_end|>\n<|im_start|>assistant\n"
    );
    assert_eq!(template("chatml").source(), template("").source());
}

#[test]
fn the_gpt_oss_check_llama_server_patches_out_is_patched_out() {
    let source = "<|channel|>{%- if \"<|channel|>analysis<|message|>\" in message.content or \"<|channel|>final<|message|>\" in message.content %}x{%- endif %}";
    assert_eq!(
        template(source).source(),
        "<|channel|>{%- if false %}x{%- endif %}"
    );
}

#[test]
fn blocks_are_trimmed_and_left_stripped_as_in_llamas_engine() {
    let t = "a\n  {% if true %}\n  b\n  {% endif %}\nc\r\n";
    assert_eq!(render(t, true, &json!({})), "a\n  b\nc");
}

#[test]
fn an_attribute_of_an_undefined_value_is_undefined() {
    assert_eq!(render("[{{ nothing.at.all }}]", true, &json!({})), "[]");
}

#[test]
fn tojson_writes_what_pythons_json_dumps_writes() {
    let t = r#"{{ {"b": [1, 2.5, "x<y&'z"], "a": none, "é": true} | tojson }}"#;
    assert_eq!(
        render(t, true, &json!({})),
        r#"{"b": [1, 2.5, "x<y&'z"], "a": null, "é": true}"#
    );
    let t = r#"{{ {"k": [1, {}]} | tojson(indent=2) }}"#;
    assert_eq!(
        render(t, true, &json!({})),
        "{\n  \"k\": [\n    1,\n    {}\n  ]\n}"
    );
    let t = r#"{{ ["é\n😀"] | tojson(ensure_ascii=true) }}|{{ tojson([1, 2], separators=(",", ":")) }}"#;
    assert_eq!(
        render(t, true, &json!({})),
        "[\"\\u00e9\\n\\ud83d\\ude00\"]|[1,2]"
    );
    let sorted = template(r#"{{ {"a": 1} | tojson(sort_keys=true) }}"#);
    assert!(render_chat(&sorted, &[], true, &ChatKwargs::default()).is_err());
}

#[test]
fn a_float_is_written_as_pythons_repr() {
    let cases = [
        (0.0, "0.0"),
        (-0.0, "-0.0"),
        (1.0, "1.0"),
        (0.1, "0.1"),
        (2.5, "2.5"),
        (123.456, "123.456"),
        (0.0001, "0.0001"),
        (0.00001, "1e-05"),
        (1e15, "1000000000000000.0"),
        (1e16, "1e+16"),
        (1.5e300, "1.5e+300"),
        (-2.25e-7, "-2.25e-07"),
    ];
    for (value, repr) in cases {
        assert_eq!(float_repr(value), repr, "{value:e}");
    }
}

#[test]
fn a_template_that_does_not_compile_is_refused_up_front() {
    assert!(matches!(
        ChatTemplate::new("{% if %}", VocabTokens::default()),
        Err(RenderError::Template(_))
    ));
}
