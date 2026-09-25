#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
# SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
# SPDX-License-Identifier: GPL-3.0-or-later
"""Apply the model-compatibility rule to the live Remote Settings records.

Phase 1 throwaway (see docs/model-compatibility.md): lists, per language
pair, the newest complete record set a given engine pin accepts, so the
result can be compared with what Firefox desktop release offers. Stdlib only.

Usage: scripts/model-compat-check.py [--json] [--all-channels]
"""

import argparse
import json
import re
import sys
import urllib.request

SERVER = "https://firefox.settings.services.mozilla.com/v1"
MODELS_COLLECTION = "translations-models-v2"
# The supported model major version window is a property of the vendored
# engine pin; see docs/model-compatibility.md ("Engine version <-> source
# commit"). These mirror LANGUAGE_MODEL_MAJOR_VERSION_MIN/MAX in Firefox's
# TranslationsParent.sys.mjs.
MODEL_MAJOR_MIN = 3
MODEL_MAJOR_MAX = 3

# The environment we evaluate filter_expression as.
ENV = {"channel": "release", "os": "Linux"}


def fetch(url):
    with urllib.request.urlopen(url, timeout=30) as r:
        return json.load(r)


def moz_version_key(version):
    """Sort key implementing Mozilla toolkit version ordering for the shapes
    that occur in these collections: '3.0' > '3.0a2' > '3.0a1' > '3.0a'."""
    parts = []
    for part in version.split("."):
        m = re.fullmatch(r"(\d+)([a-z]*)(\d*)", part)
        if not m:
            raise ValueError(f"unexpected version part {part!r} in {version!r}")
        num, alpha, pre = m.groups()
        # An absent letter part sorts after a present one ('3.0' > '3.0a1').
        parts.append((int(num), 1 if not alpha else 0, alpha, int(pre or 0)))
    return parts


def version_in_window(version):
    lo = moz_version_key(f"{MODEL_MAJOR_MIN}.0a")
    hi = moz_version_key(f"{MODEL_MAJOR_MAX + 1}.0a")
    return lo <= moz_version_key(version) < hi


# filter_expression is JEXL; we only evaluate the handful of comparison
# patterns Mozilla actually publishes, and fail closed on anything else,
# matching the daemon's planned behavior.
_TERM = re.compile(
    r"^\s*env\.(channel|appinfo\.OS)\s*(==|!=)\s*'([^']*)'\s*$"
)


def filter_matches(expression):
    """True/False if we understand the expression, None if we don't."""
    if not expression or not expression.strip():
        return True
    result = False
    for clause in expression.split("||"):
        clause_result = True
        for term in clause.split("&&"):
            m = _TERM.match(term)
            if not m:
                return None
            field, op, value = m.groups()
            actual = ENV["channel"] if field == "channel" else ENV["os"]
            ok = (actual == value) if op == "==" else (actual != value)
            clause_result = clause_result and ok
        result = result or clause_result
    return result


def accepted_sets(records, all_channels=False):
    """Group records into complete per-version sets and pick the newest
    accepted one per (pair, variant). Returns {pair_key: set_dict}."""
    groups = {}
    skipped_filters = set()
    for r in records:
        matches = filter_matches(r.get("filter_expression"))
        if matches is None:
            skipped_filters.add(r["filter_expression"])
            continue
        if not matches and not all_channels:
            continue
        if not version_in_window(r["version"]):
            continue
        key = (
            r["sourceLanguage"],
            r["targetLanguage"],
            r.get("variant") or "",
            r["version"],
        )
        groups.setdefault(key, {})[r["fileType"]] = r

    best = {}
    for (src, trg, variant, version), files in groups.items():
        if "model" not in files:
            continue
        if "vocab" not in files and not ("srcvocab" in files and "trgvocab" in files):
            continue
        pair = (src, trg, variant)
        if pair not in best or moz_version_key(version) > moz_version_key(
            best[pair]["version"]
        ):
            best[pair] = {
                "source": src,
                "target": trg,
                "variant": variant,
                "version": version,
                "architecture": files["model"]["architecture"],
                "files": {
                    ft: {
                        "name": r["name"],
                        "download_size": r["attachment"]["size"],
                        "decompressed_size": r["decompressedSize"],
                    }
                    for ft, r in sorted(files.items())
                },
            }
    for expr in sorted(skipped_filters):
        print(f"note: skipped unrecognized filter_expression: {expr!r}", file=sys.stderr)
    return best


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", action="store_true", help="machine-readable output")
    ap.add_argument(
        "--all-channels",
        action="store_true",
        help="ignore channel/OS gating (show nightly-only records too)",
    )
    args = ap.parse_args()

    records = fetch(f"{SERVER}/buckets/main/collections/{MODELS_COLLECTION}/records")[
        "data"
    ]
    best = accepted_sets(records, all_channels=args.all_channels)
    rows = sorted(best.values(), key=lambda s: (s["source"], s["target"]))

    if args.json:
        json.dump(rows, sys.stdout, indent=1)
        print()
        return

    for s in rows:
        # lex is optional and not downloaded by default; flag its presence.
        extras = ",".join(ft for ft in s["files"] if ft not in ("model",))
        dl = sum(f["download_size"] for f in s["files"].values())
        variant = f" ({s['variant']})" if s["variant"] else ""
        print(
            f"{s['source']:>7} -> {s['target']:<7} {s['version']:<6} "
            f"{s['architecture']:<12} {dl/1e6:7.1f} MB  [{extras}]{variant}"
        )
    print(f"\n{len(rows)} accepted pairs from {len(records)} records "
          f"({MODELS_COLLECTION}, window [{MODEL_MAJOR_MIN}.0a, {MODEL_MAJOR_MAX + 1}.0a))")


if __name__ == "__main__":
    main()
