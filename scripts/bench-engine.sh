#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
# SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Benchmarks a dg-smoke build: model load time, translation throughput
# (single worker thread, cpu-threads 0) and peak RSS, for bg->en and en->bg.
# Results go into docs/benchmarks.md by hand.
#
# Usage: scripts/bench-engine.sh <dg-smoke binary> <bg-corpus> <en-corpus>

set -euo pipefail

smoke="${1:?usage: bench-engine.sh <dg-smoke> <bg-corpus> <en-corpus>}"
bg_corpus="${2:?}"
en_corpus="${3:?}"

root="$(cd -- "$(dirname -- "$0")/.." && pwd)"
bg_en="$root/test-models/bg-en/3.0"
en_bg="$root/test-models/en-bg/3.0"

# Serialize BLAS to the calling thread: the daemon runs one worker per pair.
export OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1

# Prints "<wall-seconds> <max-rss-kb>" for a run over the given stdin.
# (python instead of GNU time: /usr/bin/time is not everywhere.)
measure() { # <stdin-file> <smoke-args...>
    local stdin_file="$1"
    shift
    python3 - "$stdin_file" "$smoke" "$@" <<'PY'
import resource, subprocess, sys, time
stdin_file, cmd = sys.argv[1], sys.argv[2:]
t0 = time.monotonic()
with open(stdin_file, "rb") as f:
    subprocess.run(cmd, stdin=f, stdout=subprocess.DEVNULL, check=True)
wall = time.monotonic() - t0
rss_kb = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
print(f"{wall:.2f} {rss_kb}")
PY
}

best_of() { # <n> <stdin-file> <smoke-args...>  -> best wall time, max rss
    local n="$1" best="" rss=0
    shift
    for _ in $(seq "$n"); do
        read -r t m <<<"$(measure "$@")"
        if [ -z "$best" ] || awk "BEGIN{exit !($t < $best)}"; then best="$t"; fi
        if [ "$m" -gt "$rss" ]; then rss="$m"; fi
    done
    echo "$best $rss"
}

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
: >"$tmp/empty"

lines() { wc -l <"$1"; }

run_direction() { # <label> <corpus> <model> <vocab>
    local label="$1" corpus="$2" model="$3" vocab="$4"
    read -r t_load rss_load <<<"$(best_of 3 "$tmp/empty" "$model" "$vocab")"
    read -r t_full _ <<<"$(best_of 3 "$corpus" "$model" "$vocab")"
    local n
    n="$(lines "$corpus")"
    awk -v l="$label" -v tl="$t_load" -v tf="$t_full" -v n="$n" -v rss="$rss_load" 'BEGIN {
        tt = tf - tl
        if (tt <= 0) tt = 0.001
        printf "%-8s load %5.2fs   translate %d lines in %5.2fs = %6.1f lines/s   peak RSS %d MB\n",
               l, tl, n, tt, n / tt, rss / 1024
    }'
}

echo "binary: $smoke"
ldd "$smoke" | grep -iE "blas|lapack" | sed 's/^/  /'
run_direction "bg->en" "$bg_corpus" "$bg_en/model.bgen.intgemm.alphas.bin" "$bg_en/vocab.bgen.spm"
run_direction "en->bg" "$en_corpus" "$en_bg/model.enbg.intgemm.alphas.bin" "$en_bg/vocab.enbg.spm"

# Two models loaded at once (pivot): peak RSS for the memory budget.
read -r t_pivot rss_pivot <<<"$(best_of 1 "$bg_corpus" --pivot \
    "$bg_en/model.bgen.intgemm.alphas.bin" "$bg_en/vocab.bgen.spm" \
    "$en_bg/model.enbg.intgemm.alphas.bin" "$en_bg/vocab.enbg.spm")"
awk -v t="$t_pivot" -v rss="$rss_pivot" -v n="$(lines "$bg_corpus")" 'BEGIN {
    printf "pivot    bg->en->bg %d lines in %5.2fs total   peak RSS (2 models) %d MB\n", n, t, rss / 1024
}'
