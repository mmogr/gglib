//! The sampler settings one generation runs under, read from a chat request
//! the way llama-server reads them.

use serde_json::{Map, Value, json};

/// llama.cpp's random seed, `LLAMA_DEFAULT_SEED`: a seed drawn per request.
const RANDOM_SEED: u32 = u32::MAX;

/// The sampler chain's settings for one generation.
///
/// [`Sampling::default`] is llama-server's own table at the pin, as its
/// `/props` reported it (`contracts/upstream/llama/bodies/props.*.json`), so
/// a key a request leaves out samples as it would there (ADR 0003). The
/// field names are the request's keys.
#[derive(Debug, Clone, PartialEq)]
pub struct Sampling {
    /// `temperature`.
    pub temperature: f32,
    /// `top_k`; 0 or less turns it off.
    pub top_k: i32,
    /// `top_p`.
    pub top_p: f32,
    /// `min_p`.
    pub min_p: f32,
    /// `typical_p`.
    pub typical_p: f32,
    /// `top_n_sigma`; negative turns it off.
    pub top_n_sigma: f32,
    /// `dynatemp_range`.
    pub dynatemp_range: f32,
    /// `dynatemp_exponent`.
    pub dynatemp_exponent: f32,
    /// `repeat_penalty`.
    pub repeat_penalty: f32,
    /// `repeat_last_n`.
    pub repeat_last_n: i32,
    /// `presence_penalty`.
    pub presence_penalty: f32,
    /// `frequency_penalty`.
    pub frequency_penalty: f32,
    /// `dry_multiplier`; 0 turns DRY off.
    pub dry_multiplier: f32,
    /// `dry_base`.
    pub dry_base: f32,
    /// `dry_allowed_length`.
    pub dry_allowed_length: i32,
    /// `dry_penalty_last_n`.
    pub dry_penalty_last_n: i32,
    /// `xtc_probability`.
    pub xtc_probability: f32,
    /// `xtc_threshold`.
    pub xtc_threshold: f32,
    /// `mirostat`: 0 off, 1 or 2 the version.
    pub mirostat: i32,
    /// `mirostat_tau`.
    pub mirostat_tau: f32,
    /// `mirostat_eta`.
    pub mirostat_eta: f32,
    /// `min_keep`.
    pub min_keep: i32,
    /// `seed`; `u32::MAX` draws one.
    pub seed: u32,
    /// `n_predict`, else `max_tokens`: the most tokens to sample, -1 for
    /// no limit but the context.
    pub n_predict: i32,
    /// `ignore_eos`: sample past an end-of-generation token.
    pub ignore_eos: bool,
}

impl Default for Sampling {
    fn default() -> Self {
        Self {
            temperature: 0.8,
            top_k: 40,
            top_p: 0.95,
            min_p: 0.05,
            typical_p: 1.0,
            top_n_sigma: -1.0,
            dynatemp_range: 0.0,
            dynatemp_exponent: 1.0,
            repeat_penalty: 1.0,
            repeat_last_n: 64,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
            dry_multiplier: 0.0,
            dry_base: 1.75,
            dry_allowed_length: 2,
            dry_penalty_last_n: 64,
            xtc_probability: 0.0,
            xtc_threshold: 0.1,
            mirostat: 0,
            mirostat_tau: 5.0,
            mirostat_eta: 0.1,
            min_keep: 0,
            seed: RANDOM_SEED,
            n_predict: -1,
            ignore_eos: false,
        }
    }
}

impl Sampling {
    /// The settings a chat completion `body` asks for, over the defaults.
    ///
    /// As llama-server's `json_value` reads a request: a key that is
    /// absent, null or of the wrong type keeps its default, so a request is
    /// never refused for one. `n_predict` beats `max_tokens`.
    pub fn from_request(body: &Value) -> Self {
        let mut s = Self::default();
        let float = |key: &str, into: &mut f32| {
            if let Some(v) = body.get(key).and_then(Value::as_f64) {
                *into = narrow(v);
            }
        };
        float("temperature", &mut s.temperature);
        float("top_p", &mut s.top_p);
        float("min_p", &mut s.min_p);
        float("typical_p", &mut s.typical_p);
        float("top_n_sigma", &mut s.top_n_sigma);
        float("dynatemp_range", &mut s.dynatemp_range);
        float("dynatemp_exponent", &mut s.dynatemp_exponent);
        float("repeat_penalty", &mut s.repeat_penalty);
        float("presence_penalty", &mut s.presence_penalty);
        float("frequency_penalty", &mut s.frequency_penalty);
        float("dry_multiplier", &mut s.dry_multiplier);
        float("dry_base", &mut s.dry_base);
        float("xtc_probability", &mut s.xtc_probability);
        float("xtc_threshold", &mut s.xtc_threshold);
        float("mirostat_tau", &mut s.mirostat_tau);
        float("mirostat_eta", &mut s.mirostat_eta);
        let int = |key: &str, into: &mut i32| {
            if let Some(v) = body.get(key).and_then(integer) {
                *into = v;
            }
        };
        int("top_k", &mut s.top_k);
        int("repeat_last_n", &mut s.repeat_last_n);
        int("dry_allowed_length", &mut s.dry_allowed_length);
        int("dry_penalty_last_n", &mut s.dry_penalty_last_n);
        int("mirostat", &mut s.mirostat);
        int("min_keep", &mut s.min_keep);
        int("max_tokens", &mut s.n_predict);
        int("n_predict", &mut s.n_predict);
        if let Some(seed) = body.get("seed").and_then(seed) {
            s.seed = seed;
        }
        if let Some(on) = body.get("ignore_eos").and_then(Value::as_bool) {
            s.ignore_eos = on;
        }
        s
    }

    /// The settings under llama-server's `/props` names, for
    /// `default_generation_settings.params`.
    pub fn to_json(&self) -> Value {
        let mut params = Map::new();
        let mut put = |key: &str, value: Value| {
            params.insert(key.to_owned(), value);
        };
        put("temperature", json!(self.temperature));
        put("top_k", json!(self.top_k));
        put("top_p", json!(self.top_p));
        put("min_p", json!(self.min_p));
        put("typical_p", json!(self.typical_p));
        put("top_n_sigma", json!(self.top_n_sigma));
        put("dynatemp_range", json!(self.dynatemp_range));
        put("dynatemp_exponent", json!(self.dynatemp_exponent));
        put("repeat_penalty", json!(self.repeat_penalty));
        put("repeat_last_n", json!(self.repeat_last_n));
        put("presence_penalty", json!(self.presence_penalty));
        put("frequency_penalty", json!(self.frequency_penalty));
        put("dry_multiplier", json!(self.dry_multiplier));
        put("dry_base", json!(self.dry_base));
        put("dry_allowed_length", json!(self.dry_allowed_length));
        put("dry_penalty_last_n", json!(self.dry_penalty_last_n));
        put("xtc_probability", json!(self.xtc_probability));
        put("xtc_threshold", json!(self.xtc_threshold));
        put("mirostat", json!(self.mirostat));
        put("mirostat_tau", json!(self.mirostat_tau));
        put("mirostat_eta", json!(self.mirostat_eta));
        put("min_keep", json!(self.min_keep));
        put("seed", json!(self.seed));
        put("n_predict", json!(self.n_predict));
        put("max_tokens", json!(self.n_predict));
        put("ignore_eos", json!(self.ignore_eos));
        Value::Object(params)
    }
}

/// `v` as the `f32` llama.cpp holds a sampler value in.
#[expect(
    clippy::cast_possible_truncation,
    reason = "llama.cpp holds sampler values as f32; a request's f64 is narrowed as llama-server narrows it"
)]
fn narrow(v: f64) -> f32 {
    v as f32
}

/// An integer field as llama-server reads one into an `int32_t`: a number
/// in range, a float truncated.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a float given for an integer field is truncated, as nlohmann's get<int> truncates it"
)]
fn integer(v: &Value) -> Option<i32> {
    let whole = v.as_i64().or_else(|| {
        v.as_f64()
            .filter(|f| f.is_finite())
            .map(|f| f.trunc() as i64)
    });
    whole.and_then(|n| i32::try_from(n).ok())
}

/// A `seed`: a `u32`, or -1 for a seed drawn per request.
fn seed(v: &Value) -> Option<u32> {
    match v.as_i64() {
        Some(-1) => Some(RANDOM_SEED),
        Some(n) => u32::try_from(n).ok(),
        None => None,
    }
}

#[cfg(test)]
#[path = "sampling_tests.rs"]
mod sampling_tests;
