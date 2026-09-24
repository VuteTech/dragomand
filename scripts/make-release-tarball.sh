#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Builds the release source tarball: the git tree at HEAD (which includes
# the pinned engine/vendor/ and its LICENSES/) plus the Cargo.lock, ready
# for `makepkg` and friends. Writes dragomand-<version>.tar.gz into the
# given output directory (default: target/release-tarball).
#
# --vendor adds every crate (`cargo vendor --locked`) under vendor/ plus a
# .cargo/config.toml that points cargo at it, so the tarball builds with
# no network at all. Distribution build farms (OBS, Launchpad, Koji) need
# this form. The archive is reproducible: sorted entries, fixed owner, and
# the commit time as every file's mtime.
#
# Usage: scripts/make-release-tarball.sh [--vendor] [output-dir]

set -euo pipefail

vendor=false
if [ "${1:-}" = "--vendor" ]; then
    vendor=true
    shift
fi

root="$(cd -- "$(dirname -- "$0")/.." && pwd)"
out_dir="${1:-$root/target/release-tarball}"
# Absolute, because `git -C` resolves archive -o against the repo root.
mkdir -p "$out_dir"
out_dir="$(cd -- "$out_dir" && pwd)"

version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/Cargo.toml" | head -1)"
[ -n "$version" ] || { echo "cannot read workspace version" >&2; exit 1; }

name="dragomand-$version"

if ! $vendor; then
    git -C "$root" archive --format=tar.gz --prefix="$name/" -o "$out_dir/$name.tar.gz" HEAD
    sha256sum "$out_dir/$name.tar.gz"
    exit 0
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

git -C "$root" archive --format=tar --prefix="$name/" HEAD | tar -x -C "$tmp"

echo "Vendoring crates ..." >&2
mkdir -p "$tmp/$name/.cargo"
# The config snippet goes to stdout (--quiet would suppress it too).
(cd "$tmp/$name" && cargo vendor --locked vendor 2>/dev/null) >"$tmp/$name/.cargo/config.toml"
grep -q 'directory = "vendor"' "$tmp/$name/.cargo/config.toml" || {
    echo "unexpected cargo vendor output:" >&2
    cat "$tmp/$name/.cargo/config.toml" >&2
    exit 1
}

# Debian's source tooling strips every *.orig file, and cargo vendor keeps
# each crate's original manifest as Cargo.toml.orig; cargo then fails the
# per-file checksum. The file is informational, so drop it and its
# checksum entry (the package checksum against Cargo.lock is unaffected).
find "$tmp/$name/vendor" -type f -name '*.orig' -delete
sed -E -i \
    -e 's/"[^"]*\.orig":"[0-9a-f]{64}",//g' \
    -e 's/,"[^"]*\.orig":"[0-9a-f]{64}"//g' \
    "$tmp/$name"/vendor/*/.cargo-checksum.json
if grep -l '\.orig"' "$tmp/$name"/vendor/*/.cargo-checksum.json; then
    echo "checksum entries for .orig files left in the files above" >&2
    exit 1
fi

mtime="@$(git -C "$root" log -1 --format=%ct HEAD)"
tar -C "$tmp" --sort=name --mtime="$mtime" --owner=0 --group=0 --numeric-owner \
    -cf - "$name" | gzip -n -9 >"$out_dir/$name.tar.gz"
sha256sum "$out_dir/$name.tar.gz"
