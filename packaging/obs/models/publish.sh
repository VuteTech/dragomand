#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
# SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Brings the model packages on OBS up to date with the provider: plans one
# package per language (plan.py), skips every package whose OBS sources
# already hold this version's tarball, and prepares and commits the rest
# (prepare.sh). Then does the same for dragomand-models-all, the meta
# package that depends on every language package (prepare-meta.py). Run
# by the release workflow; needs a configured osc.
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

# True when the OBS package already holds this tarball, i.e. this version.
is_current() { # <package> <tarball>
    grep -qxF "$1" "$work/packages.txt" && osc ls "$project" "$1" | grep -qxF "$2"
}

# Commits the files in <dir> as the whole content of OBS package <name>,
# creating the package first when it does not exist yet.
commit_package() { # <name> <dir> <title> <description> <message>
    local name="$1" dir="$2" title="$3" description="$4" message="$5"
    if ! grep -qxF "$name" "$work/packages.txt"; then
        osc meta pkg "$project" "$name" -F - <<EOF
<package name="$name" project="$project">
  <title>$title</title>
  <description>$description</description>
  <url>https://dragomand.l10n-bg.dev</url>
</package>
EOF
    fi
    rm -rf "$work/checkout"
    osc checkout -o "$work/checkout" "$project" "$name"
    (
        cd "$work/checkout"
        # Replace everything, so the previous version's tarball goes.
        find . -maxdepth 1 -type f ! -name '.*' -delete
        cp "$dir"/* .
        osc addremove
        osc commit -m "$message"
    )
    rm -rf "$work/checkout"
}

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

    if is_current "$name" "$tarball"; then
        current=$((current + 1))
        continue
    fi

    echo "::group::$name $version (${pairs[*]})"
    # A plain subshell statement: bash ignores `set -e` inside a subshell
    # used as an if or && condition, which would commit after a failure.
    set +e
    (
        set -e
        rm -rf "$work/pkg"
        "$here/prepare.sh" "$dragomanctl" "$work/pkg" "$name" "$language" "$version" "${pairs[@]}"
        commit_package "$name" "$work/pkg" \
            "$language translation models for dragomand" \
            "Offline machine translation models between $language and English (${pairs[*]}), from Mozilla's Firefox Translations." \
            "Models $version: ${pairs[*]}"
    )
    status=$?
    set -e
    if [ "$status" -eq 0 ]; then
        updated+=("$name $version")
    else
        failed+=("$name")
    fi
    rm -rf "$work/pkg"
    echo "::endgroup::"
done 3<"$work/plan.tsv"

# The meta package. Preparing it downloads nothing, so prepare first and
# compare the resulting version with OBS.
meta=dragomand-models-all
rm -rf "$work/meta"
meta_version="$("$here/prepare-meta.py" "$work/available.json" "$work/plan.tsv" "$work/meta")"
if is_current "$meta" "$meta-$meta_version.tar.gz"; then
    current=$((current + 1))
else
    echo "::group::$meta $meta_version"
    set +e
    (
        set -e
        commit_package "$meta" "$work/meta" \
            "All translation models for dragomand" \
            "Meta package that installs every dragomand model package." \
            "Meta package $meta_version: $(wc -l <"$work/plan.tsv") languages"
    )
    status=$?
    set -e
    if [ "$status" -eq 0 ]; then
        updated+=("$meta $meta_version")
    else
        failed+=("$meta")
    fi
    echo "::endgroup::"
fi

echo "Model packages: ${#updated[@]} updated, $current already current, ${#failed[@]} failed."
for u in "${updated[@]}"; do echo "  updated: $u"; done
if [ "${#failed[@]}" -gt 0 ]; then
    echo "::error::model packages failed to publish: ${failed[*]}"
    exit 1
fi
