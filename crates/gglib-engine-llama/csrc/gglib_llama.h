/*
 * gglib_llama: the part of llama.h the phone engine uses, as flat functions
 * whose arguments are scalars, strings, buffers with their lengths and
 * opaque handles, so no llama.h struct layout crosses into Rust. Rust
 * declares these in src/sys/mod.rs; build.rs compiles gglib_llama.c against
 * the llama.h of the pinned llama.framework.
 *
 * A function that can fail returns NULL or a negative number, and
 * gglib_llama_last_error() then says why in llama.cpp's own words. Handles
 * are freed with their _free function; a context must be freed before the
 * model it was made from, and a sampler before its model.
 */
#ifndef GGLIB_LLAMA_H
#define GGLIB_LLAMA_H

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct gglib_model gglib_model;
typedef struct gglib_context gglib_context;
typedef struct gglib_sampler gglib_sampler;

/* Told the fraction of a load read so far; returning false stops the load. */
typedef bool (*gglib_progress_fn)(float fraction, void *data);
/* Asked during a decode whether to stop it; true stops it. */
typedef bool (*gglib_abort_fn)(void *data);

/* llama's backends, and its log routed through this shim. Once before any
 * model is loaded; gglib_llama_backend_free once no model is held. */
void gglib_llama_backend_init(void);
void gglib_llama_backend_free(void);
/* Whether this build of llama.cpp can offload layers to a GPU. */
bool gglib_llama_supports_gpu_offload(void);
/* Models, contexts and samplers made and not yet freed, in this process. */
int32_t gglib_llama_live_objects(void);
/* The errors llama.cpp logged on this thread during its last call to a
 * function here that can fail; empty when it logged none. */
const char *gglib_llama_last_error(void);

/* The model at path, n_gpu_layers of its layers offloaded (-1: all), its
 * weights memory-mapped when use_mmap. NULL when it cannot be loaded, or
 * when progress returned false. */
gglib_model *gglib_llama_model_load(const char *path, int32_t n_gpu_layers, bool use_mmap,
                                    gglib_progress_fn progress, void *data);
void gglib_llama_model_free(gglib_model *model);
int32_t gglib_llama_model_n_ctx_train(const gglib_model *model);
/* The sliding window of the model's attention, 0 when it has none. */
int32_t gglib_llama_model_n_swa(const gglib_model *model);
/* tokenizer.chat_template, owned by the model; NULL when it has none. */
const char *gglib_llama_model_chat_template(const gglib_model *model);
/* The BOS and EOS tokens, -1 when the vocabulary has none, and whether
 * tokenizing with add_special adds them. */
int32_t gglib_llama_vocab_bos(const gglib_model *model);
int32_t gglib_llama_vocab_eos(const gglib_model *model);
bool gglib_llama_vocab_add_bos(const gglib_model *model);
bool gglib_llama_vocab_add_eos(const gglib_model *model);
/* Whether token ends a generation (EOS, EOT and their kind). */
bool gglib_llama_vocab_is_eog(const gglib_model *model, int32_t token);
/* text as at most n_max tokens: their count, or minus the count needed. */
int32_t gglib_llama_tokenize(const gglib_model *model, const char *text, int32_t text_len,
                             int32_t *tokens, int32_t n_max, bool add_special,
                             bool parse_special);
/* token's bytes into buf: their count, or minus the count needed. */
int32_t gglib_llama_token_to_piece(const gglib_model *model, int32_t token, char *buf,
                                   int32_t len, bool special);

/* A context of n_ctx tokens for one sequence, reading n_batch tokens per
 * decode step, flash attention auto (-1), off (0) or on (1), and its KV
 * cache in the types llama.cpp names type_k and type_v (f16, q8_0, ...). */
gglib_context *gglib_llama_context_new(gglib_model *model, uint32_t n_ctx, uint32_t n_batch,
                                       int32_t flash_attn, const char *type_k,
                                       const char *type_v);
void gglib_llama_context_free(gglib_context *ctx);
uint32_t gglib_llama_n_ctx(const gglib_context *ctx);
/* abort is asked during each decode, with data; NULL asks nothing. */
void gglib_llama_set_abort(gglib_context *ctx, gglib_abort_fn abort, void *data);
/* n_tokens tokens decoded after what the KV cache holds: 0, 2 when the
 * abort stopped it, any other value when it failed. */
int32_t gglib_llama_decode(gglib_context *ctx, const int32_t *tokens, int32_t n_tokens);
/* The KV cache cut to its first n positions; 0 empties it, which never
 * fails. False when this cache cannot be cut there. */
bool gglib_llama_memory_keep(gglib_context *ctx, int32_t n);
/* The first position the KV cache holds, -1 when it is empty. */
int32_t gglib_llama_memory_pos_min(const gglib_context *ctx);

/* llama-server's sampler chain at the pin, from a request's sampling keys:
 * the model's suppressed tokens (and, with ignore_eos, every
 * end-of-generation token) banned, then penalties, DRY, top-n-sigma,
 * top-k, typical, top-p, min-p, XTC and temperature, then greedy at
 * temperature 0 or a draw from seed; mirostat 1 or 2 in their place. NULL
 * when a value llama-server refuses is given. */
gglib_sampler *gglib_llama_sampler_new(
    const gglib_model *model, float temperature, int32_t top_k, float top_p, float min_p,
    float typical_p, float top_n_sigma, float dynatemp_range, float dynatemp_exponent,
    float repeat_penalty, int32_t repeat_last_n, float presence_penalty,
    float frequency_penalty, float dry_multiplier, float dry_base, int32_t dry_allowed_length,
    int32_t dry_penalty_last_n, float xtc_probability, float xtc_threshold, int32_t mirostat,
    float mirostat_tau, float mirostat_eta, int32_t min_keep, uint32_t seed, bool ignore_eos);
/* token counted in the chain's history, as a prompt's tokens are. */
void gglib_llama_sampler_accept(gglib_sampler *sampler, int32_t token);
/* The next token, from the logits of the last token decoded, and counted
 * in the chain's history. */
int32_t gglib_llama_sampler_sample(gglib_sampler *sampler, gglib_context *ctx);
void gglib_llama_sampler_free(gglib_sampler *sampler);

#ifdef __cplusplus
}
#endif

#endif
