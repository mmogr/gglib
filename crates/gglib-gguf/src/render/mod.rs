#![doc = include_str!("README.md")]

mod env;
mod kwargs;
mod messages;
mod tojson;

#[cfg(test)]
mod parity_tests;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt;

use chrono::{DateTime, Local};
use minijinja::Value;

pub use kwargs::ChatKwargs;

/// The template llama-server renders when the model carries none, or the
/// template given is `chatml` (`common/chat.cpp:687-693, 752-758`).
const CHATML: &str = "{%- for message in messages -%}\n  {{- '<|im_start|>' + message.role + '\n' + message.content + '<|im_end|>\n' -}}\n{%- endfor -%}\n{%- if add_generation_prompt -%}\n  {{- '<|im_start|>assistant\n' -}}\n{%- endif -%}";

/// What the model's vocabulary gives its chat template.
///
/// llama-server reads these from the loaded model
/// (`common_chat_templates_init`, `common/chat.cpp:783-807`): the pieces
/// `llama_token_to_piece` writes for the BOS and EOS tokens with special
/// tokens shown (empty when the vocabulary has none), and whether llama
/// adds BOS or EOS itself when it tokenizes the prompt.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VocabTokens {
    /// The BOS token's piece, the template's `bos_token`.
    pub bos: String,
    /// The EOS token's piece, the template's `eos_token`.
    pub eos: String,
    /// The tokenizer adds BOS, so a rendered prompt's leading BOS is dropped.
    pub add_bos: bool,
    /// The tokenizer adds EOS, so a rendered prompt's trailing EOS is dropped.
    pub add_eos: bool,
}

/// A chat template as the pinned llama-server holds it: the source it
/// renders and the vocabulary's special tokens.
#[derive(Debug, Clone)]
pub struct ChatTemplate {
    source: String,
    vocab: VocabTokens,
}

impl ChatTemplate {
    /// The model's `tokenizer.chat_template`, compiled the way llama-server
    /// compiles it.
    ///
    /// An empty source, or `chatml`, is llama-server's own `ChatML`
    /// template; the GGUF's `tokenizer.chat_template.tool_use`, which
    /// llama-server tries first, is not read. Two upstream templates are
    /// patched as llama-server patches them (`common/chat.cpp:760-781`). A
    /// source that does not compile is an error here, as llama-server
    /// refuses to start on one.
    pub fn new(source: &str, vocab: VocabTokens) -> Result<Self, RenderError> {
        let source = served_source(source);
        env::environment(&source, Local::now())?;
        Ok(Self { source, vocab })
    }

    /// The source rendered, after the fallback and patches.
    pub fn source(&self) -> &str {
        &self.source
    }
}

/// One text message of a chat, as llama-server hands it to a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    /// `system`, `user`, `assistant`, or another role the template knows;
    /// `developer` reaches most templates as `system`.
    pub role: String,
    /// The message's text; a message with none has `""`.
    pub content: String,
    /// An assistant turn's reasoning, shown to the template only when not
    /// empty.
    pub reasoning_content: Option<String>,
}

impl ChatMessage {
    /// A message with text and no reasoning.
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            reasoning_content: None,
        }
    }
}

/// A template failed to compile or render, or a request asked for
/// something llama-server refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// The template did not compile, or failed while rendering: a
    /// `raise_exception`, or an operation on the wrong type.
    Template(String),
    /// The request asked for something llama-server answers with a 400.
    Request(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Template(why) => write!(f, "chat template: {why}"),
            Self::Request(why) => write!(f, "chat request: {why}"),
        }
    }
}

impl std::error::Error for RenderError {}

impl From<minijinja::Error> for RenderError {
    fn from(err: minijinja::Error) -> Self {
        Self::Template(err.to_string())
    }
}

/// Render `messages` through `template` as the pinned llama-server's
/// `POST /apply-template` and chat completions render them.
///
/// The template is given what `common_chat_template_direct_apply_impl`
/// gives it (`common/chat.cpp:877-951`): the messages, `bos_token` and
/// `eos_token`, `enable_thinking`, `datetime` and `date_string`, every
/// template kwarg, and `add_generation_prompt` only when it is true. No
/// `tools` are given. A leading BOS or trailing EOS the tokenizer adds
/// itself is dropped from the result.
pub fn render_chat(
    template: &ChatTemplate,
    messages: &[ChatMessage],
    add_generation_prompt: bool,
    kwargs: &ChatKwargs,
) -> Result<String, RenderError> {
    render_at(
        template,
        messages,
        add_generation_prompt,
        kwargs,
        Local::now(),
    )
}

/// [`render_chat`] at a given time, which the dates are taken from.
fn render_at(
    template: &ChatTemplate,
    messages: &[ChatMessage],
    add_generation_prompt: bool,
    kwargs: &ChatKwargs,
    now: DateTime<Local>,
) -> Result<String, RenderError> {
    let env = env::environment(&template.source, now)?;
    let context = context(template, messages, add_generation_prompt, kwargs, now);
    let mut prompt = env.get_template(env::NAME)?.render(context)?;
    let vocab = &template.vocab;
    if vocab.add_bos && prompt.starts_with(&vocab.bos) {
        prompt.drain(..vocab.bos.len());
    }
    if vocab.add_eos && prompt.ends_with(&vocab.eos) {
        prompt.truncate(prompt.len() - vocab.eos.len());
    }
    Ok(prompt)
}

/// The template's variables, assembled in llama-server's order, a later
/// key overwriting an earlier one (`common/chat.cpp:886-927`).
fn context(
    template: &ChatTemplate,
    messages: &[ChatMessage],
    add_generation_prompt: bool,
    kwargs: &ChatKwargs,
    now: DateTime<Local>,
) -> BTreeMap<String, Value> {
    let bos = Value::from(template.vocab.bos.as_str());
    let eos = Value::from(template.vocab.eos.as_str());
    let mut given = BTreeMap::new();
    given.insert(
        "messages".to_owned(),
        messages::to_value(&template.source, messages),
    );
    given.insert("bos_token".to_owned(), bos.clone());
    given.insert("eos_token".to_owned(), eos.clone());
    given.insert(
        "enable_thinking".to_owned(),
        Value::from(kwargs.enable_thinking()),
    );
    // `common_chat_extra_context`, then the kwargs over it (chat.cpp:1065-1073, 1354-1357).
    given.insert(
        "datetime".to_owned(),
        Value::from(now.format("%b %d %Y").to_string()),
    );
    given.insert(
        "date_string".to_owned(),
        Value::from(now.format("%d %b %Y").to_string()),
    );
    for (key, value) in kwargs.kwargs() {
        given.insert(key.clone(), Value::from_serialize(value));
    }
    if add_generation_prompt {
        given.insert("add_generation_prompt".to_owned(), Value::from(true));
    }
    let mut context = kwargs::reasoning_variables(kwargs);
    context.extend(given);
    // bos and eos come from the model, never a kwarg (chat.cpp:923-926).
    context.insert("bos_token".to_owned(), bos);
    context.insert("eos_token".to_owned(), eos);
    context
}

/// The source llama-server renders for `source` (`common/chat.cpp:752-781`).
fn served_source(source: &str) -> String {
    if source.is_empty() || source == "chatml" {
        return CHATML.to_owned();
    }
    let mut source = source.to_owned();
    if source.contains("<|channel|>") && source.contains("in message.content or") {
        source = source.replace(
            "{%- if \"<|channel|>analysis<|message|>\" in message.content or \"<|channel|>final<|message|>\" in message.content %}",
            "{%- if false %}",
        );
    }
    if source.contains("[TOOL_CALLS]") && source.contains("if (message['content'] is none or") {
        source = source.replace(
            "{%- if (message['content'] is none or message['content'] == '' or message['content']|length == 0) and (message['tool_calls'] is not defined or message['tool_calls'] is none or message['tool_calls']|length == 0) %}",
            "{%- if false %}",
        );
    }
    source
}
