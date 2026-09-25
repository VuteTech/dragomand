#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Stamps a release version into a source tree. The version lives in git
# tags only: the checked-in workspace version is a development
# placeholder, and the release tarball gets the tag's version from this
# script. It rewrites the workspace version in Cargo.toml and the entries
# of the workspace's own crates in Cargo.lock, so builds with --locked
# keep working.
#
# Usage: scripts/set-version.sh <version> [<tree>]   (tree: the checkout)

set -euo pipefail

version="${1:?usage: set-version.sh <version> [<tree>]}"
tree="${2:-$(cd -- "$(dirname -- "$0")/.." && pwd)}"

[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
    echo "version must look like 1.2.3, got: $version" >&2
    exit 2
}

manifest="$tree/Cargo.toml"
lock="$tree/Cargo.lock"
current="$(sed -n '/^\[workspace\.package\]/,/^\[/ s/^version = "\(.*\)"$/\1/p' "$manifest")"
[ -n "$current" ] || { echo "no [workspace.package] version in $manifest" >&2; exit 1; }

# Workspace crates are the lock entries without a `source` line; only
# their version changes.
members="$(awk '
    /^\[\[package\]\]/ { if (name != "" && !source) print name; name = ""; source = 0 }
    /^name = /         { name = $3; gsub(/"/, "", name) }
    /^source = /       { source = 1 }
    END                { if (name != "" && !source) print name }
' "$lock")"
[ -n "$members" ] || { echo "no workspace crates found in $lock" >&2; exit 1; }

sed -i "/^\[workspace\.package\]/,/^\[/ s/^version = \".*\"$/version = \"$version\"/" "$manifest"
awk -v members="$members" -v version="$version" '
    BEGIN { n = split(members, list, "\n"); for (i = 1; i <= n; i++) ours[list[i]] = 1 }
    /^\[\[package\]\]/ { mine = 0 }
    /^name = / { name = $3; gsub(/"/, "", name); mine = (name in ours) }
    mine && /^version = / { $0 = "version = \"" version "\"" }
    { print }
' "$lock" >"$lock.new"
mv "$lock.new" "$lock"

stamped="$(grep -c "^version = \"$version\"$" "$lock" || true)"
expected="$(wc -l <<<"$members")"
[ "$stamped" -ge "$expected" ] || {
    echo "stamped $stamped lock entries, expected at least $expected" >&2
    exit 1
}
echo "version $current -> $version ($expected crates)"
