/*
 * gglib_llama over llama.h at the pin. Each function says which llama.h
 * calls it makes; the sampler chain follows common_sampler_init in
 * llama.cpp's common/sampling.cpp, which llama-server builds its chain with.
 */
#include "gglib_llama.h"

#include <math.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "llama.h"

struct gglib_model {
    struct llama_model *model;
    const struct llama_vocab *vocab;
};

struct gglib_context {
    struct llama_context *ctx;
};

struct gglib_sampler {
    struct llama_sampler *chain;
};

static atomic_int live_objects;

/* The errors logged on this thread since a call here began, joined. */
static _Thread_local char last_error[1024];
/* The level a GGML_LOG_LEVEL_CONT line continues. */
static _Thread_local enum ggml_log_level last_level = GGML_LOG_LEVEL_NONE;

static void forget_error(void) { last_error[0] = '\0'; }

static void note_error(const char *text, bool continued) {
    size_t used = strlen(last_error);
    const char *joint = (used > 0 && !continued) ? " " : "";
    snprintf(last_error + used, sizeof last_error - used, "%s%s", joint, text);
}

/* llama.cpp's log: errors kept for gglib_llama_last_error, and warnings and
 * errors written to stderr as llama.cpp writes them; the rest dropped. */
static void on_log(enum ggml_log_level level, const char *text, void *data) {
    (void)data;
    bool continued = level == GGML_LOG_LEVEL_CONT;
    if (continued) {
        level = last_level;
    } else {
        last_level = level;
    }
    if (level == GGML_LOG_LEVEL_ERROR) {
        note_error(text, continued);
    }
    if (level == GGML_LOG_LEVEL_WARN || level == GGML_LOG_LEVEL_ERROR) {
        fputs(text, stderr);
    }
}

void gglib_llama_backend_init(void) {
    llama_log_set(on_log, NULL);
    llama_backend_init();
}

void gglib_llama_backend_free(void) { llama_backend_free(); }

bool gglib_llama_supports_gpu_offload(void) { return llama_supports_gpu_offload(); }

int32_t gglib_llama_live_objects(void) { return atomic_load(&live_objects); }

const char *gglib_llama_last_error(void) { return last_error; }

gglib_model *gglib_llama_model_load(const char *path, int32_t n_gpu_layers, bool use_mmap,
                                    gglib_progress_fn progress, void *data) {
    forget_error();
    struct llama_model_params params = llama_model_default_params();
    params.n_gpu_layers = n_gpu_layers;
    params.load_mode = use_mmap ? LLAMA_LOAD_MODE_MMAP : LLAMA_LOAD_MODE_NONE;
    params.progress_callback = progress;
    params.progress_callback_user_data = data;
    struct llama_model *loaded = llama_model_load_from_file(path, params);
    if (loaded == NULL) {
        return NULL;
    }
    gglib_model *model = malloc(sizeof *model);
    if (model == NULL) {
        llama_model_free(loaded);
        note_error("out of memory", false);
        return NULL;
    }
    model->model = loaded;
    model->vocab = llama_model_get_vocab(loaded);
    atomic_fetch_add(&live_objects, 1);
    return model;
}

void gglib_llama_model_free(gglib_model *model) {
    if (model == NULL) {
        return;
    }
    llama_model_free(model->model);
    free(model);
    atomic_fetch_sub(&live_objects, 1);
}

int32_t gglib_llama_model_n_ctx_train(const gglib_model *model) {
    return llama_model_n_ctx_train(model->model);
}

int32_t gglib_llama_model_n_swa(const gglib_model *model) {
    return llama_model_n_swa(model->model);
}

const char *gglib_llama_model_chat_template(const gglib_model *model) {
    return llama_model_chat_template(model->model, NULL);
}

int32_t gglib_llama_vocab_bos(const gglib_model *model) { return llama_vocab_bos(model->vocab); }

int32_t gglib_llama_vocab_eos(const gglib_model *model) { return llama_vocab_eos(model->vocab); }

bool gglib_llama_vocab_add_bos(const gglib_model *model) {
    return llama_vocab_get_add_bos(model->vocab);
}

bool gglib_llama_vocab_add_eos(const gglib_model *model) {
    return llama_vocab_get_add_eos(model->vocab);
}

bool gglib_llama_vocab_is_eog(const gglib_model *model, int32_t token) {
    return llama_vocab_is_eog(model->vocab, token);
}

int32_t gglib_llama_tokenize(const gglib_model *model, const char *text, int32_t text_len,
                             int32_t *tokens, int32_t n_max, bool add_special,
                             bool parse_special) {
    return llama_tokenize(model->vocab, text, text_len, tokens, n_max, add_special,
                          parse_special);
}

int32_t gglib_llama_token_to_piece(const gglib_model *model, int32_t token, char *buf,
                                   int32_t len, bool special) {
    return llama_token_to_piece(model->vocab, token, buf, len, 0, special);
}

/* The ggml type llama.cpp's --cache-type-k/-v name, as gglib-core's
 * KvCacheType::as_llama_arg spells it. */
static bool kv_type(const char *name, enum ggml_type *type) {
    static const struct {
        const char *name;
        enum ggml_type type;
    } known[] = {
        {"f32", GGML_TYPE_F32},   {"f16", GGML_TYPE_F16},   {"bf16", GGML_TYPE_BF16},
        {"q8_0", GGML_TYPE_Q8_0}, {"q5_1", GGML_TYPE_Q5_1}, {"q5_0", GGML_TYPE_Q5_0},
        {"q4_1", GGML_TYPE_Q4_1}, {"q4_0", GGML_TYPE_Q4_0},
    };
    for (size_t i = 0; i < sizeof known / sizeof known[0]; i++) {
        if (strcmp(name, known[i].name) == 0) {
            *type = known[i].type;
            return true;
        }
    }
    return false;
}

gglib_context *gglib_llama_context_new(gglib_model *model, uint32_t n_ctx, uint32_t n_batch,
                                       int32_t flash_attn, const char *type_k,
                                       const char *type_v) {
    forget_error();
    struct llama_context_params params = llama_context_default_params();
    if (!kv_type(type_k, &params.type_k) || !kv_type(type_v, &params.type_v)) {
        note_error("a KV cache type llama.cpp does not name", false);
        return NULL;
    }
    params.n_ctx = n_ctx;
    params.n_batch = n_batch;
    params.n_ubatch = n_batch;
    params.n_seq_max = 1;
    params.flash_attn_type = flash_attn < 0    ? LLAMA_FLASH_ATTN_TYPE_AUTO
                             : flash_attn == 0 ? LLAMA_FLASH_ATTN_TYPE_DISABLED
                                               : LLAMA_FLASH_ATTN_TYPE_ENABLED;
    params.no_perf = true;
    struct llama_context *made = llama_init_from_model(model->model, params);
    if (made == NULL) {
        return NULL;
    }
    gglib_context *ctx = malloc(sizeof *ctx);
    if (ctx == NULL) {
        llama_free(made);
        note_error("out of memory", false);
        return NULL;
    }
    ctx->ctx = made;
    atomic_fetch_add(&live_objects, 1);
    return ctx;
}

void gglib_llama_context_free(gglib_context *ctx) {
    if (ctx == NULL) {
        return;
    }
    llama_free(ctx->ctx);
    free(ctx);
    atomic_fetch_sub(&live_objects, 1);
}

uint32_t gglib_llama_n_ctx(const gglib_context *ctx) { return llama_n_ctx(ctx->ctx); }

void gglib_llama_set_abort(gglib_context *ctx, gglib_abort_fn abort, void *data) {
    llama_set_abort_callback(ctx->ctx, abort, data);
}

int32_t gglib_llama_decode(gglib_context *ctx, const int32_t *tokens, int32_t n_tokens) {
    forget_error();
    /* llama_batch_get_one only reads the tokens. */
    struct llama_batch batch = llama_batch_get_one((llama_token *)tokens, n_tokens);
    return llama_decode(ctx->ctx, batch);
}

bool gglib_llama_memory_keep(gglib_context *ctx, int32_t n) {
    return llama_memory_seq_rm(llama_get_memory(ctx->ctx), 0, n, -1);
}

int32_t gglib_llama_memory_pos_min(const gglib_context *ctx) {
    return llama_memory_seq_pos_min(llama_get_memory(ctx->ctx), 0);
}

/* DRY's sequence breakers when a request names none, common_params_sampling's
 * dry_sequence_breakers. */
static const char *dry_breakers[] = {"\n", ":", "\"", "*"};

/* The model's suppressed tokens, and with ignore_eos every token that ends
 * a generation, each biased to -inf, as common_sampler_init and
 * llama-server's ignore_eos add them. False when memory runs out. */
static bool add_bans(struct llama_sampler *chain, const struct llama_vocab *vocab,
                     bool ignore_eos) {
    const int32_t n_vocab = llama_vocab_n_tokens(vocab);
    int32_t n_suppress = 0;
    const llama_token *suppress = llama_vocab_get_suppress_tokens(vocab, &n_suppress);
    size_t room = (size_t)n_suppress + (ignore_eos ? (size_t)n_vocab : 0);
    if (room == 0) {
        return true;
    }
    llama_logit_bias *bans = malloc(room * sizeof *bans);
    if (bans == NULL) {
        return false;
    }
    int32_t n = 0;
    for (int32_t i = 0; i < n_suppress; i++) {
        bans[n++] = (llama_logit_bias){suppress[i], -INFINITY};
    }
    for (int32_t token = 0; ignore_eos && token < n_vocab; token++) {
        if (llama_vocab_is_eog(vocab, token)) {
            bans[n++] = (llama_logit_bias){token, -INFINITY};
        }
    }
    if (n > 0) {
        llama_sampler_chain_add(chain, llama_sampler_init_logit_bias(n_vocab, n, bans));
    }
    free(bans);
    return true;
}

gglib_sampler *gglib_llama_sampler_new(
    const gglib_model *model, float temperature, int32_t top_k, float top_p, float min_p,
    float typical_p, float top_n_sigma, float dynatemp_range, float dynatemp_exponent,
    float repeat_penalty, int32_t repeat_last_n, float presence_penalty,
    float frequency_penalty, float dry_multiplier, float dry_base, int32_t dry_allowed_length,
    int32_t dry_penalty_last_n, float xtc_probability, float xtc_threshold, int32_t mirostat,
    float mirostat_tau, float mirostat_eta, int32_t min_keep, uint32_t seed, bool ignore_eos) {
    forget_error();
    /* common_sampler_init refuses these, and llama-server answers with its
     * words. */
    if (!isfinite(repeat_penalty) || repeat_penalty <= 0.0f || !isfinite(1.0f / repeat_penalty)) {
        note_error("penalty_repeat must be finite and greater than 0", false);
        return NULL;
    }
    if (!isfinite(frequency_penalty)) {
        note_error("penalty_freq must be finite", false);
        return NULL;
    }
    if (!isfinite(presence_penalty)) {
        note_error("penalty_present must be finite", false);
        return NULL;
    }
    const struct llama_vocab *vocab = model->vocab;
    const int32_t n_vocab = llama_vocab_n_tokens(vocab);
    /* llama-server holds mirostat to 0..2; a negative min_keep, which it
     * refuses, would wrap as a size_t. */
    mirostat = mirostat < 0 ? 0 : mirostat > 2 ? 2 : mirostat;
    const size_t keep = min_keep > 0 ? (size_t)min_keep : 0;

    struct llama_sampler_chain_params chain_params = llama_sampler_chain_default_params();
    chain_params.no_perf = true;
    struct llama_sampler *chain = llama_sampler_chain_init(chain_params);
    if (!add_bans(chain, vocab, ignore_eos)) {
        llama_sampler_free(chain);
        note_error("out of memory", false);
        return NULL;
    }
    if (mirostat == 0) {
        llama_sampler_chain_add(chain, llama_sampler_init_penalties(n_vocab, repeat_last_n,
                                                                     repeat_penalty,
                                                                     frequency_penalty,
                                                                     presence_penalty));
        llama_sampler_chain_add(
            chain, llama_sampler_init_dry(vocab, dry_multiplier, dry_base, dry_allowed_length,
                                          dry_penalty_last_n, dry_breakers,
                                          sizeof dry_breakers / sizeof dry_breakers[0]));
        llama_sampler_chain_add(chain, llama_sampler_init_top_n_sigma(top_n_sigma));
        llama_sampler_chain_add(chain, llama_sampler_init_top_k(top_k));
        llama_sampler_chain_add(chain, llama_sampler_init_typical(typical_p, keep));
        llama_sampler_chain_add(chain, llama_sampler_init_top_p(top_p, keep));
        llama_sampler_chain_add(chain, llama_sampler_init_min_p(min_p, keep));
        llama_sampler_chain_add(chain,
                                llama_sampler_init_xtc(xtc_probability, xtc_threshold, keep, seed));
        llama_sampler_chain_add(
            chain, llama_sampler_init_temp_ext(temperature, dynatemp_range, dynatemp_exponent));
        /* The chain ends at temperature, so 0 with no dynamic range is
         * greedy, as in common_sampler_init with no probabilities asked. */
        const bool greedy = temperature == 0.0f && dynatemp_range == 0.0f;
        llama_sampler_chain_add(chain, greedy ? llama_sampler_init_greedy()
                                              : llama_sampler_init_dist(seed));
    } else {
        llama_sampler_chain_add(chain, llama_sampler_init_temp(temperature));
        llama_sampler_chain_add(
            chain, mirostat == 1
                       ? llama_sampler_init_mirostat(n_vocab, seed, mirostat_tau, mirostat_eta, 100)
                       : llama_sampler_init_mirostat_v2(seed, mirostat_tau, mirostat_eta));
    }
    gglib_sampler *sampler = malloc(sizeof *sampler);
    if (sampler == NULL) {
        llama_sampler_free(chain);
        note_error("out of memory", false);
        return NULL;
    }
    sampler->chain = chain;
    atomic_fetch_add(&live_objects, 1);
    return sampler;
}

void gglib_llama_sampler_accept(gglib_sampler *sampler, int32_t token) {
    llama_sampler_accept(sampler->chain, token);
}

int32_t gglib_llama_sampler_sample(gglib_sampler *sampler, gglib_context *ctx) {
    return llama_sampler_sample(sampler->chain, ctx->ctx, -1);
}

void gglib_llama_sampler_free(gglib_sampler *sampler) {
    if (sampler == NULL) {
        return;
    }
    llama_sampler_free(sampler->chain);
    free(sampler);
    atomic_fetch_sub(&live_objects, 1);
}
