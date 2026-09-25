#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
# SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Assembles the OBS package directory for one model package: a source
# tarball holding a verified system store plus one build recipe per
# package format, with the placeholders stamped in.
#
# The models are downloaded here, not on OBS: build hosts have no
# network. `dragomanctl store install --root` applies the daemon's own
# acceptance rule and sha256 checks and writes the manifests, so the
# packaged store is exactly what the daemon would have downloaded.
#
# Usage: packaging/obs/models/prepare.sh <dragomanctl> <out-dir> \
#            <package name> <language> <version> <pair>...
# The last four arguments are one line of plan.py's output.

set -euo pipefail

usage="usage: prepare.sh <dragomanctl> <out-dir> <package> <language> <version> <pair>..."
dragomanctl="${1:?$usage}"
out="${2:?$usage}"
name="${3:?$usage}"
language="${4:?$usage}"
version="${5:?$usage}"
shift 5
[ "$#" -gt 0 ] || { echo "$usage" >&2; exit 2; }
pairs=("$@")

here="$(cd -- "$(dirname -- "$0")" && pwd)"
root="$(cd -- "$here/../../.." && pwd)"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
top="$work/$name-$version"

"$dragomanctl" store install --root "$top/models" "${pairs[@]}"
rm -f "$top/models/.lock"

# Every requested pair must have landed; the provider could have moved on
# between planning and installing.
for pair in "${pairs[@]}"; do
    compgen -G "$top/models/mozilla-remote-settings/$pair/*/manifest.json" >/dev/null || {
        echo "$pair did not install into the package store" >&2
        exit 1
    }
done

cp "$root/engine/vendor/LICENSE" "$top/LICENSE"
{
    echo "$language translation models for dragomand"
    echo
    echo "Mozilla's Firefox Translations models, from the Remote Settings"
    echo "collection translations-models-v2, licensed under MPL-2.0 (see"
    echo "LICENSE; the mozilla/translations README states that the model"
    echo "files are distributed under the MPL 2.0 license)."
    echo
    echo "Pairs and versions:"
    for manifest in "$top"/models/mozilla-remote-settings/*/*/manifest.json; do
        dir="${manifest%/manifest.json}"
        echo "  ${dir#"$top/models/mozilla-remote-settings/"}"
    done
    echo
    echo "Each manifest.json lists the sha256 of every file; the daemon"
    echo "verifies them. Project: https://dragomand.l10n-bg.dev"
} >"$top/README"

# Reproducible: sorted entries, fixed owner and modes, and one timestamp.
epoch="${SOURCE_DATE_EPOCH:-$(date +%s)}"
mkdir -p "$out"
tar --sort=name --owner=0 --group=0 --numeric-owner \
    --mode='u+rwX,go+rX,go-w' --mtime="@$epoch" \
    -C "$work" -cf - "$name-$version" | gzip -9 -n >"$out/$name-$version.tar.gz"

date="$(LC_ALL=C date -R -d "@$epoch")"
rpm_date="$(LC_ALL=C date -u -d "@$epoch" '+%a %b %d %Y')"
stamp() { # <source> <destination>
    sed -e "s/@NAME@/$name/g" \
        -e "s/@LANGUAGE@/$language/g" \
        -e "s/@VERSION@/$version/g" \
        -e "s/@PAIRS@/${pairs[*]}/g" \
        -e "s/@DATE@/$date/g" \
        -e "s/@RPM_DATE@/$rpm_date/g" \
        "$1" >"$2"
}
stamp "$here/dragomand-model.spec" "$out/$name.spec"
stamp "$here/dragomand-model.dsc" "$out/$name.dsc"
for f in debian.control debian.rules debian.changelog PKGBUILD; do
    stamp "$here/$f" "$out/$f"
done

if grep -l '@[A-Z_]*@' "$out/$name.spec" "$out/$name.dsc" "$out"/debian.* "$out/PKGBUILD"; then
    echo "unstamped placeholders left in the files above" >&2
    exit 1
fi
ls -l "$out"
