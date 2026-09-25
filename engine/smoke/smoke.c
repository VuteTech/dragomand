/* SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info> */
/* SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech> */
/* SPDX-License-Identifier: GPL-3.0-or-later */
/*
 * dg-smoke: exercises the dragoman_engine C ABI without any Rust.
 *
 * Usage:
 *   dg-smoke MODEL VOCAB [SHORTLIST] < input.txt
 *   dg-smoke --pivot MODEL1 VOCAB1 MODEL2 VOCAB2 < input.txt
 *
 * Reads stdin (whole input as one segment per line), translates, prints one
 * line per input line. The Marian options match what Firefox passes; see
 * docs/model-compatibility.md.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "dragoman_engine.h"

/* Every current Mozilla model is *.intgemm.alphas.bin => int8shiftAlphaAll.
 * Models named *.intgemm8.bin would need int8shiftAll instead. */
static const char *config_yaml =
    "beam-size: 1\n"
    "normalize: 1.0\n"
    "word-penalty: 0\n"
    "max-length-break: 128\n"
    "mini-batch-words: 1024\n"
    "workspace: 128\n"
    "max-length-factor: 2.0\n"
    "skip-cost: true\n"
    "cpu-threads: 0\n"
    "quiet: true\n"
    "quiet-translation: true\n"
    "gemm-precision: int8shiftAlphaAll\n"
    "alignment: soft\n";

static void die(const char *what, dg_error *err) {
    fprintf(stderr, "dg-smoke: %s: %s\n", what,
            err != NULL ? dg_error_message(err) : "(no message)");
    dg_error_free(err);
    exit(1);
}

static dg_model *load(dg_engine *engine, const char *model_path,
                      const char *vocab_path, const char *shortlist_path) {
    dg_error *err = NULL;
    dg_model_files files = {0};
    files.model_path = model_path;
    files.vocab_path = vocab_path;
    files.shortlist_path = shortlist_path;
    dg_model *model = dg_model_load(engine, &files, config_yaml, &err);
    if (model == NULL) {
        die("model load failed", err);
    }
    return model;
}

int main(int argc, char **argv) {
    const char *usage =
        "usage: dg-smoke MODEL VOCAB [SHORTLIST] < input\n"
        "       dg-smoke --pivot MODEL1 VOCAB1 MODEL2 VOCAB2 < input\n";
    int pivot = argc > 1 && strcmp(argv[1], "--pivot") == 0;
    if ((!pivot && (argc < 3 || argc > 4)) || (pivot && argc != 6)) {
        fputs(usage, stderr);
        return 2;
    }

    dg_error *err = NULL;
    dg_engine_options engine_options = {0};
    dg_engine *engine = dg_engine_create(&engine_options, &err);
    if (engine == NULL) {
        die("engine create failed", err);
    }

    dg_model *first = NULL;
    dg_model *second = NULL;
    if (pivot) {
        first = load(engine, argv[2], argv[3], NULL);
        second = load(engine, argv[4], argv[5], NULL);
    } else {
        first = load(engine, argv[1], argv[2], argc == 4 ? argv[3] : NULL);
    }

    /* One segment per stdin line. */
    char **lines = NULL;
    size_t n = 0, cap = 0;
    char *line = NULL;
    size_t linecap = 0;
    ssize_t len;
    while ((len = getline(&line, &linecap, stdin)) > 0) {
        if (line[len - 1] == '\n') {
            line[--len] = '\0';
        }
        if (n == cap) {
            cap = cap == 0 ? 16 : cap * 2;
            lines = realloc(lines, cap * sizeof(*lines));
            if (lines == NULL) {
                perror("realloc");
                return 1;
            }
        }
        lines[n++] = strdup(line);
    }
    free(line);

    dg_text *segments = calloc(n, sizeof(*segments));
    for (size_t i = 0; i < n; i++) {
        segments[i].data = lines[i];
        segments[i].len = strlen(lines[i]);
    }

    dg_result *result = NULL;
    dg_translate_options translate_options = {0};
    if (dg_translate_batch(engine, first, second, segments, n,
                           &translate_options, &result, &err) != 0) {
        die("translate failed", err);
    }
    for (size_t i = 0; i < dg_result_len(result); i++) {
        dg_text text = dg_result_text(result, i);
        fwrite(text.data, 1, text.len, stdout);
        fputc('\n', stdout);
    }

    dg_result_free(result);
    if (second != NULL) {
        dg_model_unload(second);
    }
    dg_model_unload(first);
    dg_engine_destroy(engine);
    for (size_t i = 0; i < n; i++) {
        free(lines[i]);
    }
    free(lines);
    free(segments);
    return 0;
}
