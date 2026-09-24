#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Vendors Mozilla's translation engine into engine/vendor/.
#
# Copies inference/ from mozilla/translations at the given commit, plus the
# nested submodules the native build needs. Dropped entirely: emsdk (wasm
# toolchain), nccl (GPU), simple-websocket-server (marian-server), fbgemm
# (we build with USE_FBGEMM=OFF; Mozilla ships intgemm models). A plain git
# submodule won't do here, because the submodule definitions live in the
# repo's top-level .gitmodules and would drag in the training pipeline.
#
# Usage: engine/scripts/update-vendor.sh <mozilla/translations commit>

set -euo pipefail

commit="${1:?usage: update-vendor.sh <mozilla/translations commit>}"
repo_url="https://github.com/mozilla/translations.git"

engine_dir="$(cd -- "$(dirname -- "$0")/.." && pwd)"
vendor_dir="$engine_dir/vendor"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# Submodule paths (relative to the mozilla/translations root) that the
# native library build requires.
submodules=(
    inference/3rd_party/ssplit-cpp
    inference/marian-fork/src/3rd_party/sentencepiece
    inference/marian-fork/src/3rd_party/intgemm
    inference/marian-fork/src/3rd_party/ruy
    inference/marian-fork/src/3rd_party/simd_utils
    inference/marian-fork/src/3rd_party/onnxjs
)

echo "Fetching mozilla/translations @ $commit ..."
git init -q "$tmp/repo"
git -C "$tmp/repo" remote add origin "$repo_url"
git -C "$tmp/repo" fetch -q --depth 1 origin "$commit"
git -C "$tmp/repo" sparse-checkout set inference
git -C "$tmp/repo" checkout -q FETCH_HEAD

echo "Fetching submodules ..."
git -C "$tmp/repo" submodule update -q --init --depth 1 "${submodules[@]}"

echo "Writing $vendor_dir ..."
rm -rf "$vendor_dir"
mkdir -p "$vendor_dir"
rsync -a \
    --exclude='.git' \
    --exclude='.git*' \
    --exclude='/3rd_party/emsdk/' \
    --exclude='/marian-fork/src/3rd_party/nccl/' \
    --exclude='/marian-fork/src/3rd_party/fbgemm/' \
    --exclude='/marian-fork/src/3rd_party/simple-websocket-server/' \
    "$tmp/repo/inference/" "$vendor_dir/"

# Record every commit hash: the main repo and each submodule under
# inference/, vendored or not, so the provenance is complete.
{
    echo "# Written by engine/scripts/update-vendor.sh on $(date -u +%Y-%m-%d)."
    echo "# Vendored from $repo_url"
    echo "mozilla/translations $commit"
    git -C "$tmp/repo" ls-tree -r FETCH_HEAD | awk '$2 == "commit" && $4 ~ /^inference\// { print $4, $3 }' |
        while read -r path sha; do
            vendored=dropped
            for s in "${submodules[@]}"; do
                [ "$s" = "$path" ] && vendored=vendored
            done
            echo "$path $sha $vendored"
        done
} >"$vendor_dir/VERSION"

# The inference/ tree has no license file of its own; mozilla/translations
# is MPL-2.0 at the repository root. Copy that in as the tree's license.
cp "$tmp/repo/LICENSE" "$vendor_dir/LICENSE"

# Overlay our replacement files (currently only marian's fragile FindCBLAS;
# see engine/patches/). Paths under patches/ mirror the vendor tree.
cp "$engine_dir/patches/FindCBLAS.cmake" "$vendor_dir/marian-fork/cmake/FindCBLAS.cmake"

# Apply source patches (each header explains why it exists). patch(1) fails
# loudly when a pin bump invalidates one; refresh it then.
for p in "$engine_dir"/patches/*.patch; do
    patch --directory="$vendor_dir" --strip=1 --forward --silent <"$p"
done

# marian's build generates common/git_revision.h from git metadata, which a
# vendored tree lacks; pre-generate it (see marian-no-git-metadata.patch).
printf '#define GIT_REVISION "%s (vendored from mozilla/translations)"\n' \
    "$(git -C "$tmp/repo" rev-parse --short FETCH_HEAD)" \
    >"$vendor_dir/marian-fork/src/common/git_revision.h"

# Collect the third-party license texts. Vendored files keep their own
# headers; this directory is for binary distributions.
licenses="$vendor_dir/LICENSES"
mkdir -p "$licenses"
copy_license() { # <source-in-vendor> <destination-name>
    if [ -f "$vendor_dir/$1" ]; then
        cp "$vendor_dir/$1" "$licenses/$2"
    else
        echo "warning: expected license file missing: $1" >&2
    fi
}
copy_license LICENSE bergamot-translator.txt   # MPL-2.0
copy_license marian-fork/LICENSE.md marian-fork.md # MIT
copy_license 3rd_party/ssplit-cpp/LICENSE.md ssplit-cpp.md # Apache-2.0
# The LGPL-2.1 nonbreaking_prefixes data ships inside ssplit-cpp and is read
# at runtime; keep its notice alongside the code (no separate file upstream).
for lic in "$vendor_dir"/marian-fork/src/3rd_party/*/LICENSE; do
    name="$(basename "$(dirname "$lic")")"
    copy_license "${lic#"$vendor_dir/"}" "$name.txt"
done

echo "Done. Review $vendor_dir/VERSION and $licenses/. Re-check the"
echo "dependency licenses whenever the pin moves."
