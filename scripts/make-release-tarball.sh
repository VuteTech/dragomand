#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Builds the release source tarball: the git tree at HEAD (which includes
# the pinned engine/vendor/ and its LICENSES/) plus the Cargo.lock, ready
# for `makepkg` and friends. Writes dragomand-<version>.tar.gz into the
# given output directory (default: target/release-tarball).

set -euo pipefail

root="$(cd -- "$(dirname -- "$0")/.." && pwd)"
out_dir="${1:-$root/target/release-tarball}"
# Absolute, because `git -C` resolves archive -o against the repo root.
mkdir -p "$out_dir"
out_dir="$(cd -- "$out_dir" && pwd)"

version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/Cargo.toml" | head -1)"
[ -n "$version" ] || { echo "cannot read workspace version" >&2; exit 1; }

name="dragomand-$version"
mkdir -p "$out_dir"
git -C "$root" archive --format=tar.gz --prefix="$name/" -o "$out_dir/$name.tar.gz" HEAD
sha256sum "$out_dir/$name.tar.gz"
