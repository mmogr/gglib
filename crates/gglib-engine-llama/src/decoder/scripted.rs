//! A decoder that replays a script, for tests that need a model's answers
//! without a model.

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use gglib_gguf::VocabTokens;

use super::{
    Abort, Decoder, DecoderError, FinishReason, Finished, LoadParams, Loaded, PromptProgress,
    Sampling, Step, Token,
};

/// What a [`ScriptedDecoder`] answers with.
#[derive(Debug, Clone)]
pub struct Script {
    /// The pieces every generation samples, in order. A generation that
    /// samples them all ends as if at an end-of-generation token.
    pub pieces: Vec<String>,
    /// How many prompt tokens one prompt-reading step reads.
    pub batch: u32,
    /// How long each step takes.
    pub step_delay: Duration,
    /// The model's `n_ctx_train`.
    pub n_ctx_train: u32,
    /// The model's chat template; empty is llama-server's `ChatML`.
    pub chat_template: String,
    /// The model's special tokens.
    pub vocab: VocabTokens,
}

impl Script {
    /// `pieces`, read 512 prompt tokens to a step, with no delay, from a
    /// model trained to 4096 tokens with no template and a BOS of `<s>` it
    /// adds itself.
    pub fn new<S: Into<String>>(pieces: impl IntoIterator<Item = S>) -> Self {
        Self {
            pieces: pieces.into_iter().map(Into::into).collect(),
            batch: 512,
            step_delay: Duration::ZERO,
            n_ctx_train: 4096,
            chat_template: String::new(),
            vocab: VocabTokens {
                bos: "<s>".to_owned(),
                eos: "</s>".to_owned(),
                add_bos: true,
                add_eos: false,
            },
        }
    }
}

/// What a [`ScriptedDecoder`] was asked and did, for a test to read.
#[derive(Debug, Clone, Default)]
pub struct ScriptRecord {
    /// Each load's path and parameters.
    pub loads: Vec<(String, LoadParams)>,
    /// The text of each generation's prompt, the BOS included.
    pub prompts: Vec<String>,
    /// The sampling each generation ran under.
    pub samplings: Vec<Sampling>,
    /// How each generation ended.
    pub finished: Vec<Finished>,
    /// Pieces handed out while the abort was already raised, over every
    /// generation: a decoder that checks before each step hands out at most
    /// one per abort.
    pub pieces_after_abort: u32,
    /// Pieces handed out, over every generation.
    pub pieces: u32,
}

/// A [`Decoder`] that tokenizes a character to a token and samples the
/// pieces of its [`Script`].
///
/// The KV cache is modelled: it holds the last prompt and the pieces
/// sampled after it, and a prompt that starts as they did reuses that
/// prefix, as llama-server's `cache_prompt` does, leaving at least its last
/// token to read. Generation stops at `n_predict` and when the context is
/// full, as a real decoder does.
#[derive(Debug)]
pub struct ScriptedDecoder {
    script: Script,
    record: Arc<Mutex<ScriptRecord>>,
    n_ctx: Option<u32>,
    cache: Vec<Token>,
}

impl ScriptedDecoder {
    /// A decoder replaying `script`, and the record it keeps.
    pub fn new(script: Script) -> (Self, Arc<Mutex<ScriptRecord>>) {
        let record = Arc::new(Mutex::new(ScriptRecord::default()));
        let decoder = Self {
            script,
            record: Arc::clone(&record),
            n_ctx: None,
            cache: Vec::new(),
        };
        (decoder, record)
    }

    fn note(&self, f: impl FnOnce(&mut ScriptRecord)) {
        f(&mut self.record.lock().unwrap_or_else(PoisonError::into_inner));
    }
}

impl Decoder for ScriptedDecoder {
    fn load(&mut self, path: &Path, params: &LoadParams) -> Result<Loaded, DecoderError> {
        self.note(|r| r.loads.push((path.display().to_string(), *params)));
        self.n_ctx = Some(params.n_ctx);
        self.cache.clear();
        Ok(Loaded {
            n_ctx_train: self.script.n_ctx_train,
            n_ctx: params.n_ctx,
            chat_template: self.script.chat_template.clone(),
            vocab: self.script.vocab.clone(),
        })
    }

    fn tokenize(&self, text: &str) -> Result<Vec<Token>, DecoderError> {
        if self.n_ctx.is_none() {
            return Err(DecoderError::NotLoaded);
        }
        let vocab = &self.script.vocab;
        let bos = vocab.add_bos.then_some(vocab.bos.as_str());
        Ok(bos
            .into_iter()
            .chain([text])
            .flat_map(str::chars)
            .map(char_token)
            .collect())
    }

    fn generate(
        &mut self,
        prompt: &[Token],
        sampling: &Sampling,
        abort: &Abort,
        sink: &mut dyn FnMut(Step),
    ) -> Result<Finished, DecoderError> {
        let n_ctx = self.n_ctx.ok_or(DecoderError::NotLoaded)?;
        let text: String = prompt.iter().filter_map(|&t| token_char(t)).collect();
        self.note(|r| {
            r.prompts.push(text);
            r.samplings.push(sampling.clone());
        });
        let total = len32(prompt.len());
        let common = self
            .cache
            .iter()
            .zip(prompt)
            .take_while(|(a, b)| a == b)
            .count();
        let cache = len32(common.min(prompt.len().saturating_sub(1)));
        self.cache = prompt.to_vec();
        let mut finished = Finished {
            reason: FinishReason::Eos,
            cached_tokens: cache,
            completion_tokens: 0,
        };
        let mut processed = cache;
        sink(Step::Progress(PromptProgress {
            total,
            cache,
            processed,
        }));
        while processed < total {
            if abort.is_raised() {
                finished.reason = FinishReason::Aborted;
                return Ok(self.finish(finished));
            }
            std::thread::sleep(self.script.step_delay);
            processed = total.min(processed + self.script.batch.max(1));
            sink(Step::Progress(PromptProgress {
                total,
                cache,
                processed,
            }));
        }
        let limit = u32::try_from(sampling.n_predict).ok();
        for piece in &self.script.pieces {
            if abort.is_raised() {
                finished.reason = FinishReason::Aborted;
                return Ok(self.finish(finished));
            }
            if limit.is_some_and(|n| finished.completion_tokens >= n)
                || total + finished.completion_tokens >= n_ctx
            {
                finished.reason = FinishReason::Length;
                return Ok(self.finish(finished));
            }
            std::thread::sleep(self.script.step_delay);
            finished.completion_tokens += 1;
            self.cache.extend(piece.chars().map(char_token));
            let late = abort.is_raised();
            self.note(|r| {
                r.pieces += 1;
                r.pieces_after_abort += u32::from(late);
            });
            sink(Step::Piece(piece.clone()));
        }
        // The end-of-generation token the script ends at is sampled too.
        finished.completion_tokens += 1;
        Ok(self.finish(finished))
    }

    fn unload(&mut self) {
        self.n_ctx = None;
        self.cache.clear();
    }
}

impl ScriptedDecoder {
    fn finish(&self, finished: Finished) -> Finished {
        self.note(|r| r.finished.push(finished));
        finished
    }
}

/// A character's token: its code point.
fn char_token(c: char) -> Token {
    Token::try_from(u32::from(c)).unwrap_or(Token::MAX)
}

/// The character a token stands for.
fn token_char(t: Token) -> Option<char> {
    u32::try_from(t).ok().and_then(char::from_u32)
}

fn len32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}
