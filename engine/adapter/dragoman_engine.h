/* SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info> */
/* SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech> */
/* SPDX-License-Identifier: GPL-3.0-or-later */
/*
 * dragoman_engine: a narrow C ABI over the Bergamot translation engine.
 *
 * Threading contract: a dg_engine and every dg_model loaded on it belong to
 * one thread at a time. The intended pattern is one engine per worker thread
 * (BlockingService is synchronous and not thread-safe). Handles from one
 * engine must not be mixed with another engine's.
 *
 * Error contract: every function that takes a dg_error **err either
 * succeeds, or returns NULL / nonzero and stores a heap-allocated error the
 * caller frees with dg_error_free(). Passing NULL for err discards the
 * message. No C++ exception ever crosses this boundary, and Marian aborts
 * are converted to errors (see dg_engine_create).
 */

#ifndef DRAGOMAN_ENGINE_H
#define DRAGOMAN_ENGINE_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct dg_engine dg_engine;
typedef struct dg_model dg_model;
typedef struct dg_result dg_result;
typedef struct dg_error dg_error;

typedef struct dg_engine_options {
    /* Translation-cache capacity in sentences; 0 disables caching. */
    size_t cache_size;
    /* Engine log level ("trace" … "critical"), or NULL for "off". */
    const char *log_level;
} dg_engine_options;

/* Model files for one direction, as filesystem paths (UTF-8, NUL-terminated,
 * preferably absolute). Either vocab_path is set (shared source/target
 * vocabulary), or both src_vocab_path and trg_vocab_path are. shortlist_path
 * is optional and may be NULL (Firefox runs without it by default). */
typedef struct dg_model_files {
    const char *model_path;
    const char *vocab_path;
    const char *src_vocab_path;
    const char *trg_vocab_path;
    const char *shortlist_path;
} dg_model_files;

/* A UTF-8 string that need not be NUL-terminated. */
typedef struct dg_text {
    const char *data;
    size_t len;
} dg_text;

typedef struct dg_translate_options {
    /* Nonzero: segments are HTML; markup is preserved in the output. */
    int html;
} dg_translate_options;

/* Creates an engine. Also switches Marian's ABORT handler to throwing
 * exceptions (process-wide, once), so a model failure cannot kill the
 * process. Returns NULL on failure. */
dg_engine *dg_engine_create(const dg_engine_options *options, dg_error **err);

/* Destroys the engine. All models loaded on it must be unloaded first. */
void dg_engine_destroy(dg_engine *engine);

/* Loads one translation model. config_yaml holds the Marian decoder options
 * *without* any file path keys (models/vocabs/shortlist are built from
 * files); see docs/model-compatibility.md for the configuration Firefox
 * uses per model. Returns NULL on failure. */
dg_model *dg_model_load(dg_engine *engine, const dg_model_files *files,
                        const char *config_yaml, dg_error **err);

void dg_model_unload(dg_model *model);

/* Translates n segments. With second == NULL, translates with first alone;
 * otherwise pivots (first: source→pivot, second: pivot→target). Both models
 * must be loaded on this engine. Returns 0 and stores a result with exactly
 * n entries, or nonzero on failure. */
int dg_translate_batch(dg_engine *engine, dg_model *first, dg_model *second,
                       const dg_text *segments, size_t n,
                       const dg_translate_options *options, dg_result **out,
                       dg_error **err);

size_t dg_result_len(const dg_result *result);
/* Valid until dg_result_free. index must be < dg_result_len(). */
dg_text dg_result_text(const dg_result *result, size_t index);
void dg_result_free(dg_result *result);

/* Message is valid until dg_error_free. Never NULL for a stored error. */
const char *dg_error_message(const dg_error *err);
void dg_error_free(dg_error *err);

/* ABI version of this header; bump on incompatible change. */
#define DG_ENGINE_ABI_VERSION 1
size_t dg_engine_abi_version(void);

#ifdef __cplusplus
}
#endif

#endif /* DRAGOMAN_ENGINE_H */
