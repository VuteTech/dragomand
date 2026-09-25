#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Brings the model packages on OBS up to date with the provider: plans one
# package per language (plan.py), skips every package whose OBS sources
# already hold this version's tarball, and prepares and commits the rest
# (prepare.sh). Run by the release workflow; needs a configured osc.
#
# Packages for languages the provider stops offering are left alone.
#
# Usage: OBS_PROJECT=<project> packaging/obs/models/publish.sh <dragomanctl>

set -euo pipefail

dragomanctl="${1:?usage: OBS_PROJECT=<project> publish.sh <dragomanctl>}"
project="${OBS_PROJECT:?OBS_PROJECT must name the OBS project}"
here="$(cd -- "$(dirname -- "$0")" && pwd)"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

"$dragomanctl" --json store available >"$work/available.json"
"$here/plan.py" <"$work/available.json" >"$work/plan.tsv"
echo "Planned $(wc -l <"$work/plan.tsv") model packages."

osc ls "$project" >"$work/packages.txt"

updated=()
current=0
failed=()
# fd 3, so that nothing inside the loop can swallow the plan from stdin.
while IFS=$'\t' read -r -u 3 -a fields; do
    name="${fields[0]}"
    language="${fields[1]}"
    version="${fields[2]}"
    pairs=("${fields[@]:3}")
    tarball="$name-$version.tar.gz"

    if grep -qxF "$name" "$work/packages.txt" &&
        osc ls "$project" "$name" | grep -qxF "$tarball"; then
        current=$((current + 1))
        continue
    fi

    echo "::group::$name $version (${pairs[*]})"
    if (
        set -e
        pkg="$work/pkg"
        rm -rf "$pkg" "$work/checkout"
        "$here/prepare.sh" "$dragomanctl" "$pkg" "$name" "$language" "$version" "${pairs[@]}"

        if ! grep -qxF "$name" "$work/packages.txt"; then
            osc meta pkg "$project" "$name" -F - <<EOF
<package name="$name" project="$project">
  <title>$language translation models for dragomand</title>
  <description>Offline machine translation models between $language and English (${pairs[*]}), from Mozilla's Firefox Translations.</description>
  <url>https://dragomand.l10n-bg.dev</url>
</package>
EOF
        fi
        osc checkout -o "$work/checkout" "$project" "$name"
        cd "$work/checkout"
        # Replace everything, so the previous version's tarball goes.
        find . -maxdepth 1 -type f ! -name '.*' -delete
        cp "$pkg"/* .
        osc addremove
        osc commit -m "Models $version: ${pairs[*]}"
    ); then
        updated+=("$name $version")
    else
        failed+=("$name")
    fi
    rm -rf "$work/pkg" "$work/checkout"
    echo "::endgroup::"
done 3<"$work/plan.tsv"

echo "Model packages: ${#updated[@]} updated, $current already current, ${#failed[@]} failed."
for u in "${updated[@]}"; do echo "  updated: $u"; done
if [ "${#failed[@]}" -gt 0 ]; then
    echo "::error::model packages failed to publish: ${failed[*]}"
    exit 1
fi
