#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Assembles the OBS package directory for one release: the vendored source
# tarball plus one build recipe per package format, with the version
# stamped in. OBS picks the recipe matching each repository:
#
#   dragomand.spec                     openSUSE, Fedora
#   dragomand.dsc + debian.*           Debian, Ubuntu (via debtransform)
#   PKGBUILD                           Arch Linux
#
# Usage: packaging/obs/prepare.sh <version> <vendored-tarball> <out-dir>
# The tarball comes from `scripts/make-release-tarball.sh --vendor`.

set -euo pipefail

version="${1:?usage: prepare.sh <version> <vendored-tarball> <out-dir>}"
tarball="${2:?usage: prepare.sh <version> <vendored-tarball> <out-dir>}"
out="${3:?usage: prepare.sh <version> <vendored-tarball> <out-dir>}"

here="$(cd -- "$(dirname -- "$0")" && pwd)"

[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
    echo "version must look like 1.2.3, got: $version" >&2
    exit 2
}
[ "$(basename "$tarball")" = "dragomand-$version.tar.gz" ] || {
    echo "tarball must be named dragomand-$version.tar.gz" >&2
    exit 2
}
tar -tzf "$tarball" "dragomand-$version/.cargo/config.toml" >/dev/null 2>&1 || {
    echo "$tarball is not vendored; build it with make-release-tarball.sh --vendor" >&2
    exit 2
}

# Changelog dates: the commit time when reproducing a release, else now.
date="$(LC_ALL=C date -R ${SOURCE_DATE_EPOCH:+-d @$SOURCE_DATE_EPOCH})"
rpm_date="$(LC_ALL=C date -u ${SOURCE_DATE_EPOCH:+-d @$SOURCE_DATE_EPOCH} '+%a %b %d %Y')"

# The .dsc repeats the Build-Depends of debian.control; derive it so the
# two cannot drift apart.
build_depends="$(sed -n '/^Build-Depends:/,/^[A-Z]/p' "$here/debian.control" |
    sed '$d' | sed 's/^Build-Depends://' | tr -d '\n' | sed 's/^ *//; s/  */ /g')"

mkdir -p "$out"
stamp() { # <source> <destination>
    sed -e "s/@VERSION@/$version/g" \
        -e "s/@DATE@/$date/g" \
        -e "s/@RPM_DATE@/$rpm_date/g" \
        -e "s/@BUILD_DEPENDS@/$build_depends/g" \
        "$1" >"$2"
}

stamp "$here/dragomand.spec" "$out/dragomand.spec"
stamp "$here/dragomand.dsc" "$out/dragomand.dsc"
stamp "$here/debian.control" "$out/debian.control"
stamp "$here/debian.rules" "$out/debian.rules"
stamp "$here/debian.changelog" "$out/debian.changelog"
sed -e "s/^pkgver=.*/pkgver=$version/" -e "s/^pkgrel=.*/pkgrel=1/" \
    "$here/PKGBUILD" >"$out/PKGBUILD"
cp "$tarball" "$out/"

if grep -l '@[A-Z_]*@' "$out"/dragomand.spec "$out"/dragomand.dsc "$out"/debian.*; then
    echo "unstamped placeholders left in the files above" >&2
    exit 1
fi
ls -l "$out"
