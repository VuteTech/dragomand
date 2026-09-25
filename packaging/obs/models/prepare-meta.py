#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
# SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
# SPDX-License-Identifier: GPL-3.0-or-later
"""Assembles the OBS package directory for dragomand-models-all, the meta
package that depends on every language package in the plan.

Usage: prepare-meta.py <available.json> <plan.tsv> <out-dir>

<available.json> is `dragomanctl --json store available` (for the total
size), <plan.tsv> the output of plan.py. Writes a small source tarball
(only a README listing the languages) and the recipes from meta/, and
prints the package version.

The version is the highest model version in the plan plus the newest
record date across all languages, like each language package's own
version: adding a language or changing any model raises it.
"""

import gzip
import io
import json
import os
import re
import sys
import tarfile
import time
from pathlib import Path

NAME = "dragomand-models-all"


def version_key(version):
    return tuple(int(part) for part in version.split("."))


def main():
    if len(sys.argv) != 4:
        sys.exit(__doc__.strip().splitlines()[3])
    available = json.loads(Path(sys.argv[1]).read_text())
    plan = [line.split("\t") for line in Path(sys.argv[2]).read_text().splitlines() if line]
    out = Path(sys.argv[3])
    here = Path(__file__).resolve().parent / "meta"

    # Each plan version is <model version>.<YYYYMMDD>.
    tops, dates = [], []
    for fields in plan:
        top, _, date = fields[2].rpartition(".")
        tops.append(top)
        dates.append(date)
    version = f"{max(tops, key=version_key)}.{max(dates)}"

    packages = sorted(fields[0] for fields in plan)
    size = sum(s["size"] for s in available if not s.get("variant"))
    size_text = f"{size / 2**30:.1f} GiB"
    epoch = int(os.environ.get("SOURCE_DATE_EPOCH", time.time()))

    readme = [
        "dragomand-models-all installs every dragomand model package:",
        f"{len(packages)} languages, each paired with English, roughly {size_text}.",
        "",
    ]
    readme += [f"  {fields[0]}  {fields[1]}: {' '.join(fields[3:])}" for fields in sorted(plan)]
    readme += ["", "Project: https://dragomand.l10n-bg.dev", ""]

    out.mkdir(parents=True, exist_ok=True)
    top_dir = f"{NAME}-{version}"
    data = "\n".join(readme).encode()
    buffer = io.BytesIO()
    # Reproducible: fixed owner, mode and timestamps, and a gzip header
    # without a time.
    with tarfile.open(fileobj=buffer, mode="w", format=tarfile.GNU_FORMAT) as tar:
        for path, content in ((top_dir, None), (f"{top_dir}/README", data)):
            info = tarfile.TarInfo(path)
            info.mtime = epoch
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            if content is None:
                info.type = tarfile.DIRTYPE
                info.mode = 0o755
                tar.addfile(info)
            else:
                info.mode = 0o644
                info.size = len(content)
                tar.addfile(info, io.BytesIO(content))
    with open(out / f"{top_dir}.tar.gz", "wb") as f:
        with gzip.GzipFile(fileobj=f, mode="wb", mtime=0, filename="") as gz:
            gz.write(buffer.getvalue())

    stamps = {
        "@VERSION@": version,
        "@COUNT@": str(len(packages)),
        "@SIZE@": size_text,
        "@REQUIRES@": "\n".join(f"Requires:       {p}" for p in packages),
        "@DEPENDS@": ",\n".join(f" {p}" for p in packages),
        "@DATE@": time.strftime("%a, %d %b %Y %H:%M:%S +0000", time.gmtime(epoch)),
        "@RPM_DATE@": time.strftime("%a %b %d %Y", time.gmtime(epoch)),
    }
    for template in sorted(here.iterdir()):
        text = template.read_text()
        if template.name == "PKGBUILD":
            # One dependency per line inside depends=( ... ).
            text = text.replace("@DEPENDS@", "\n".join(f"          {p}" for p in packages))
        for key, value in stamps.items():
            text = text.replace(key, value)
        if re.search(r"@[A-Z_]+@", text):
            sys.exit(f"unstamped placeholder left in {template.name}")
        target = out / template.name
        target.write_text(text)
        target.chmod(template.stat().st_mode & 0o777)
    print(version)


if __name__ == "__main__":
    main()
