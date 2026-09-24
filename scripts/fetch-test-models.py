#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Download Mozilla translation models for local testing.

Fetches the accepted record set (see docs/model-compatibility.md) for the
given pairs from Remote Settings, verifies the sha256 of both the compressed
download and the decompressed file, and stores the decompressed files under
test-models/<src>-<trg>/<version>/. Needs the zstd CLI. Not the real model
store (that is dragoman-models, Phase 4): just enough for engine tests.

Usage: scripts/fetch-test-models.py [pair ...]     # default: bg-en en-bg
"""

import hashlib
import json
import pathlib
import subprocess
import sys
import urllib.request

sys.path.insert(0, str(pathlib.Path(__file__).parent))
compat = __import__("model-compat-check")

DEST = pathlib.Path(__file__).parent.parent / "test-models"
SERVER = compat.SERVER


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(1 << 20):
            h.update(chunk)
    return h.hexdigest()


def main():
    pairs = sys.argv[1:] or ["bg-en", "en-bg"]

    records = compat.fetch(
        f"{SERVER}/buckets/main/collections/{compat.MODELS_COLLECTION}/records"
    )["data"]
    base_url = compat.fetch(f"{SERVER}/")["capabilities"]["attachments"]["base_url"]
    by_key = {
        (r["sourceLanguage"], r["targetLanguage"], r.get("variant") or "", r["version"], r["fileType"]): r
        for r in records
    }
    accepted = compat.accepted_sets(records)

    for pair in pairs:
        src, trg = pair.split("-")
        chosen = accepted.get((src, trg, ""))
        if not chosen:
            sys.exit(f"no accepted record set for {pair}")
        out_dir = DEST / pair / chosen["version"]
        out_dir.mkdir(parents=True, exist_ok=True)
        for file_type in chosen["files"]:
            record = by_key[(src, trg, "", chosen["version"], file_type)]
            att = record["attachment"]
            target = out_dir / record["name"]
            if target.exists() and sha256(target) == record["decompressedHash"]:
                print(f"{pair}: {record['name']} already present")
                continue
            compressed = target.with_suffix(target.suffix + ".zst")
            print(f"{pair}: downloading {record['name']} ({att['size']} bytes)")
            urllib.request.urlretrieve(base_url + att["location"], compressed)
            if compressed.stat().st_size != att["size"] or sha256(compressed) != att["hash"]:
                sys.exit(f"{compressed}: size or sha256 mismatch on download")
            subprocess.run(["zstd", "-q", "-d", "-f", str(compressed), "-o", str(target)], check=True)
            compressed.unlink()
            if target.stat().st_size != record["decompressedSize"] or sha256(target) != record["decompressedHash"]:
                sys.exit(f"{target}: size or sha256 mismatch after decompression")
        manifest = {
            "pair": pair,
            "version": chosen["version"],
            "architecture": chosen["architecture"],
            "files": {ft: by_key[(src, trg, "", chosen["version"], ft)]["name"] for ft in chosen["files"]},
        }
        (out_dir / "test-manifest.json").write_text(json.dumps(manifest, indent=1) + "\n")
        print(f"{pair}: {chosen['version']} ({chosen['architecture']}) ready in {out_dir}")


if __name__ == "__main__":
    main()
