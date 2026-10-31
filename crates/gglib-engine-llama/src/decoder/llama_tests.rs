//! The decoder over llama.h, on the pinned test models and the Mac's slice
//! of llama.framework.
//!
//! The models come from `GGLIB_ENGINE_MODELS_DIR`, which CI fills from
//! `.github/actions/pinned-engines/models.txt`. A test that does not find
//! its model fails under CI, which sets `CI`, and is skipped elsewhere with
//! the reason printed. The tests take turns: the count of llama objects
//! alive is the process's, and one decode at a time keeps them quick.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use gglib_core::cache_config::KvCacheType;
use gglib_gguf::{GgufParser, GgufParserPort as _, VocabTokens};
use serde_json::{Value, json};

use super::LlamaDecoder;
use crate::decoder::{
    Abort, Decoder, DecoderError, FinishReason, Finished, LoadParams, Loaded, PromptProgress,
    Sampling, Step,
};
use crate::engine::Engine;
use crate::sys::{self, Context, DecodeStop, Model};

const STORIES: &str = "stories260K-f32.gguf";
const GEMMA: &str = "tinygemma3-Q8_0.gguf";
const ONCE: &str = "Once upon a time";

static TURN: Mutex<()> = Mutex::new(());

/// This test's turn with llama.cpp, until the guard drops.
fn turn() -> MutexGuard<'static, ()> {
    TURN.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The test model `name`, or why it is missing: a failure under CI.
fn model(name: &str) -> Option<PathBuf> {
    let dir = std::env::var_os("GGLIB_ENGINE_MODELS_DIR").map(PathBuf::from);
    let path = dir.as_ref().map(|dir| dir.join(name));
    if path.as_ref().is_some_and(|p| p.is_file()) {
        return path;
    }
    let why = dir.map_or_else(
        || format!("GGLIB_ENGINE_MODELS_DIR is unset, so there is no {name}"),
        |dir| {
            format!(
                "{name} is not in GGLIB_ENGINE_MODELS_DIR ({})",
                dir.display()
            )
        },
    );
    assert!(
        std::env::var_os("CI").is_none(),
        "{why}: CI fetches the test models, so this is a failure"
    );
    eprintln!("skipped: {why}");
    None
}

/// `n_ctx` tokens of `f16` cache. stories260K's heads are 8 wide, which a
/// `q8_0` cache's blocks of 32 do not divide, so gglib gives it `f16` too.
fn f16(n_ctx: u32) -> LoadParams {
    LoadParams {
        n_ctx,
        cache_type_k: KvCacheType::F16,
        cache_type_v: KvCacheType::F16,
    }
}

/// A decoder holding the model at `path`, and what it loaded.
fn loaded(path: &Path, params: LoadParams) -> (LlamaDecoder, Loaded) {
    let mut decoder = LlamaDecoder::new();
    let loaded = decoder
        .load(path, &params, &Abort::new(), &mut |_| {})
        .expect("the model loads");
    (decoder, loaded)
}

/// Greedy sampling of at most `n` tokens (-1: no limit), past any
/// end-of-generation token.
fn greedy(n: i32) -> Sampling {
    Sampling {
        temperature: 0.0,
        n_predict: n,
        ignore_eos: true,
        ..Sampling::default()
    }
}

/// What a generation from `prompt` handed out, and its counts.
fn generate(
    decoder: &mut LlamaDecoder,
    prompt: &str,
    sampling: &Sampling,
) -> (Vec<Step>, Finished) {
    let tokens = decoder.tokenize(prompt).expect("the prompt tokenizes");
    let mut steps = Vec::new();
    let finished = decoder
        .generate(&tokens, sampling, &Abort::new(), &mut |step| {
            steps.push(step);
        })
        .expect("the generation runs");
    (steps, finished)
}

fn pieces(steps: &[Step]) -> Vec<&str> {
    steps
        .iter()
        .filter_map(|step| match step {
            Step::Piece(piece) => Some(piece.as_str()),
            Step::Progress(_) => None,
        })
        .collect()
}

fn progress(steps: &[Step]) -> Vec<PromptProgress> {
    steps
        .iter()
        .filter_map(|step| match step {
            Step::Progress(p) => Some(*p),
            Step::Piece(_) => None,
        })
        .collect()
}

#[test]
fn stories_loads_at_the_context_asked_reporting_its_progress_to_the_end() {
    let Some(path) = model(STORIES) else { return };
    let _turn = turn();
    let mut decoder = LlamaDecoder::new();
    let mut reported = Vec::new();

    let loaded = decoder
        .load(&path, &f16(512), &Abort::new(), &mut |f| reported.push(f))
        .expect("the model loads");

    assert_eq!(loaded.n_ctx_train, 2048);
    assert_eq!(loaded.n_ctx, 512);
    let vocab = VocabTokens {
        bos: "<s>".to_owned(),
        eos: "</s>".to_owned(),
        add_bos: true,
        add_eos: false,
    };
    assert_eq!(
        loaded.vocab, vocab,
        "as the pinned llama-server's /props names them"
    );
    assert!(reported.windows(2).all(|w| w[0] <= w[1]), "{reported:?}");
    assert_eq!(reported.last(), Some(&1.0), "{reported:?}");
}

#[test]
fn a_greedy_reply_streams_sixteen_pieces_and_counts_the_prompt_and_the_reply() {
    let Some(path) = model(STORIES) else { return };
    let _turn = turn();
    let (mut decoder, _) = loaded(&path, f16(512));
    let n_prompt = u32::try_from(decoder.tokenize(ONCE).unwrap().len()).unwrap();

    let (steps, finished) = generate(&mut decoder, ONCE, &greedy(16));

    let expected = Finished {
        reason: FinishReason::Length,
        cached_tokens: 0,
        completion_tokens: 16,
    };
    assert_eq!(finished, expected);
    assert_eq!(pieces(&steps).len(), 16, "a piece a token: {steps:?}");
    let progress = progress(&steps);
    let first = progress.first().map(|p| (p.total, p.processed));
    assert_eq!(first, Some((n_prompt, 0)));
    assert_eq!(progress.last().map(|p| p.processed), Some(n_prompt));
    let reply = pieces(&steps).concat();
    assert!(!reply.trim().is_empty(), "{reply:?}");

    // The same prompt again: the KV cache holds all but its last token, and
    // a greedy reply read from there is the same reply.
    let (again, finished) = generate(&mut decoder, ONCE, &greedy(16));
    assert_eq!(finished.cached_tokens, n_prompt - 1);
    assert_eq!(pieces(&again).concat(), reply);
}

#[test]
fn an_abort_raised_mid_reply_ends_it_before_another_piece() {
    let Some(path) = model(STORIES) else { return };
    let _turn = turn();
    let (mut decoder, _) = loaded(&path, f16(512));
    let prompt = decoder.tokenize(ONCE).unwrap();
    let abort = Abort::new();
    let mut handed_out = 0;

    let finished = decoder
        .generate(&prompt, &greedy(-1), &abort, &mut |step| {
            if matches!(step, Step::Piece(_)) {
                handed_out += 1;
                if handed_out == 4 {
                    abort.raise();
                }
            }
        })
        .expect("the generation ends");

    assert_eq!(finished.reason, FinishReason::Aborted);
    assert_eq!(handed_out, 4, "no piece after the abort");
    assert_eq!(finished.completion_tokens, 4);
    // The KV cache is left as the decoder records it: the next reply runs.
    let (steps, finished) = generate(&mut decoder, ONCE, &greedy(4));
    assert_eq!(finished.reason, FinishReason::Length);
    assert_eq!(pieces(&steps).len(), 4);
}

#[test]
fn a_decode_whose_abort_is_raised_is_stopped_inside_llama() {
    let Some(path) = model(STORIES) else { return };
    let _turn = turn();
    let _backend = sys::Backend::acquire();
    let model = Arc::new(Model::load(&path, 0, &Abort::new(), &mut |_| {}).unwrap());
    let mut ctx = Context::new(Arc::clone(&model), 256, 512, None, "f16", "f16").unwrap();
    let tokens = model.tokenize(ONCE).unwrap();
    let abort = Abort::new();
    ctx.watch(&abort);
    abort.raise();

    assert!(matches!(ctx.decode(&tokens), Err(DecodeStop::Aborted)));

    ctx.watch(&Abort::new());
    assert!(ctx.keep(0));
    assert!(ctx.decode(&tokens).is_ok(), "a fresh abort lets it decode");
}

#[test]
fn an_empty_prompt_is_refused_and_the_cached_prefix_kept() {
    let Some(path) = model(STORIES) else { return };
    let _turn = turn();
    let (mut decoder, _) = loaded(&path, f16(512));
    let n_prompt = u32::try_from(decoder.tokenize(ONCE).unwrap().len()).unwrap();
    generate(&mut decoder, ONCE, &greedy(4));
    let mut handed_out = Vec::new();

    let refused = decoder.generate(&[], &greedy(4), &Abort::new(), &mut |step| {
        handed_out.push(step);
    });

    assert_eq!(refused, Err(DecoderError::EmptyPrompt));
    assert_eq!(handed_out, Vec::new(), "nothing was sampled");
    let (_, again) = generate(&mut decoder, ONCE, &greedy(4));
    assert_eq!(again.cached_tokens, n_prompt - 1, "the KV cache kept it");
}

#[test]
fn a_load_stopped_from_its_progress_leaves_no_handle_and_the_next_load_runs() {
    let Some(path) = model(STORIES) else { return };
    let _turn = turn();
    let mut decoder = LlamaDecoder::new();
    let before = sys::live_objects();
    let abort = Abort::new();

    let stopped = decoder.load(&path, &f16(512), &abort, &mut |_| abort.raise());

    assert_eq!(stopped, Err(DecoderError::LoadAborted));
    // The count is of the shim's own handles: what llama.cpp's loader read
    // before it stopped is its own to free.
    assert_eq!(sys::live_objects(), before, "no model or context handle");
    assert_eq!(decoder.tokenize(ONCE), Err(DecoderError::NotLoaded));
    decoder
        .load(&path, &f16(512), &Abort::new(), &mut |_| {})
        .expect("the next load runs");
    assert_eq!(sys::live_objects(), before + 2, "a model and its context");
    drop(decoder);
    assert_eq!(sys::live_objects(), before);
}

#[test]
fn a_context_llama_cannot_make_fails_the_load_in_its_words() {
    let Some(path) = model(STORIES) else { return };
    let _turn = turn();
    let mut decoder = LlamaDecoder::new();
    let before = sys::live_objects();
    let q8 = LoadParams {
        cache_type_k: KvCacheType::Q8_0,
        cache_type_v: KvCacheType::Q8_0,
        ..f16(512)
    };

    let refused = decoder.load(&path, &q8, &Abort::new(), &mut |_| {});

    let Err(DecoderError::Load(why)) = refused else {
        panic!("{refused:?}");
    };
    assert!(why.contains("does not divide n_embd_head_k"), "{why}");
    assert_eq!(sys::live_objects(), before, "the model read is freed");
}

#[test]
fn tinygemma3_carries_its_template_and_special_tokens_as_llama_server_reads_them() {
    let Some(path) = model(GEMMA) else { return };
    let _turn = turn();

    let (_decoder, loaded) = loaded(&path, f16(256));

    let gguf = GgufParser.parse(&path).expect("gglib-gguf reads the file");
    let template = gguf.metadata.get("tokenizer.chat_template");
    assert_eq!(Some(&loaded.chat_template), template);
    assert!(loaded.chat_template.starts_with("{{ bos_token }}"));
    // What the pinned llama-server's /props answers for this model.
    let vocab = VocabTokens {
        bos: "<bos>".to_owned(),
        eos: "<eos>".to_owned(),
        add_bos: true,
        add_eos: false,
    };
    assert_eq!(loaded.vocab, vocab);
}

#[test]
fn the_shim_streams_a_real_reply_whose_usage_counts_the_prompt_and_the_reply() {
    let Some(path) = model(STORIES) else { return };
    let _turn = turn();
    let engine = Engine::new(LlamaDecoder::new());
    engine
        .load(1, "stories", &path, &f16(512), &Abort::new())
        .expect("the model loads");
    let body = json!({
        "model": "stories",
        "messages": [{"role": "user", "content": ONCE}],
        "stream": true,
        "return_progress": true,
        "stream_options": {"include_usage": true},
        "max_tokens": 16,
        "temperature": 0,
        "ignore_eos": true,
    });

    let answer = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let shim = crate::shim::start(Arc::clone(&engine)).await.unwrap();
            let client = reqwest::Client::builder().no_proxy().build().unwrap();
            let url = format!("{}/v1/chat/completions", shim.base_url());
            let answer = client.post(url).json(&body).send().await.unwrap();
            assert_eq!(answer.status(), 200);
            let text = answer.text().await.unwrap();
            shim.shutdown().await.unwrap();
            text
        });

    let frames: Vec<Value> = answer
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter(|data| *data != "[DONE]")
        .map(|data| serde_json::from_str(data).expect("a JSON frame"))
        .collect();
    let read = frames
        .iter()
        .filter_map(|f| f["prompt_progress"]["total"].as_u64())
        .next_back()
        .expect("prompt progress was reported");
    let usage = frames
        .iter()
        .find(|f| f["usage"].is_object())
        .map(|f| f["usage"].clone())
        .expect("a usage frame");
    let content: String = frames
        .iter()
        .filter_map(|f| f["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert_eq!(usage["prompt_tokens"], read, "{usage}");
    assert_eq!(usage["completion_tokens"], 16, "{usage}");
    assert!(!content.trim().is_empty(), "{answer}");
}
