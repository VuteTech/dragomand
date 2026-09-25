#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
# SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
# SPDX-License-Identifier: GPL-3.0-or-later
"""Summarizes the build results of a whole OBS project, every package at
once, from the XML of `osc api /build/<project>/_result` on stdin.

Exit status: 0 when every build is final and none failed, 1 when every
build is final but some failed, 2 while anything is still pending. Pending
means a repository that is not yet published or is marked dirty (about to
be rescheduled), or a package whose status is not final.
"""

import sys
import xml.etree.ElementTree as ET

FINAL = {"succeeded", "failed", "unresolvable", "broken", "excluded", "disabled", "locked"}
BAD = {"failed", "unresolvable", "broken"}


def main():
    root = ET.parse(sys.stdin).getroot()
    pending, bad = [], []
    for result in root.iter("result"):
        where = f"{result.get('repository')}/{result.get('arch')}"
        state = result.get("state")
        if result.get("dirty") == "true":
            pending.append(f"{where}: repository {state}, marked for rescheduling")
        elif state not in {"published", "unpublished"}:
            pending.append(f"{where}: repository {state}")
        for status in result.iter("status"):
            code = status.get("code")
            line = f"{where} {status.get('package')}: {code}"
            if code not in FINAL:
                pending.append(line)
            elif code in BAD:
                bad.append(line)
    for line in pending:
        print(f"pending  {line}")
    for line in bad:
        print(f"FAILED   {line}")
    if pending:
        return 2
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
