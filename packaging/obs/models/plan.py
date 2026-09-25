#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Plans the model packages: one package per language, holding every
direction the provider offers between it and English.

Reads the output of `dragomanctl --json store available` on stdin and
prints one tab-separated line per package:

    <package name> <language name> <version> <pair> [<pair> ...]

The version is the highest model version in the package plus the date of
its newest Remote Settings record, for example 3.0.20260912. Any change
to the package's models means a newer record, so the version rises with
it and sorts correctly for rpm, dpkg and pacman alike.
"""

import json
import sys
import time
from collections import defaultdict

# English names for package summaries; codes follow the Remote Settings
# records. An unknown code falls back to the code itself.
LANGUAGE_NAMES = {
    "af": "Afrikaans",
    "ar": "Arabic",
    "az": "Azerbaijani",
    "be": "Belarusian",
    "bg": "Bulgarian",
    "bn": "Bengali",
    "bs": "Bosnian",
    "ca": "Catalan",
    "cs": "Czech",
    "da": "Danish",
    "de": "German",
    "el": "Greek",
    "es": "Spanish",
    "et": "Estonian",
    "eu": "Basque",
    "fa": "Persian",
    "fi": "Finnish",
    "fr": "French",
    "gl": "Galician",
    "gu": "Gujarati",
    "he": "Hebrew",
    "hi": "Hindi",
    "hr": "Croatian",
    "hu": "Hungarian",
    "id": "Indonesian",
    "is": "Icelandic",
    "it": "Italian",
    "ja": "Japanese",
    "kn": "Kannada",
    "ko": "Korean",
    "lt": "Lithuanian",
    "lv": "Latvian",
    "ml": "Malayalam",
    "mr": "Marathi",
    "ms": "Malay",
    "nb": "Norwegian Bokmål",
    "nl": "Dutch",
    "nn": "Norwegian Nynorsk",
    "pl": "Polish",
    "pt": "Portuguese",
    "ro": "Romanian",
    "ru": "Russian",
    "sk": "Slovak",
    "sl": "Slovenian",
    "sq": "Albanian",
    "sr": "Serbian",
    "sv": "Swedish",
    "ta": "Tamil",
    "te": "Telugu",
    "th": "Thai",
    "tr": "Turkish",
    "uk": "Ukrainian",
    "ur": "Urdu",
    "vi": "Vietnamese",
    "zh-Hans": "Chinese (Simplified)",
    "zh-Hant": "Chinese (Traditional)",
}


def version_key(version):
    """Numeric sort key for plain release versions such as 3.0 or 2.10."""
    return tuple(int(part) for part in version.split("."))


def main():
    sets = json.load(sys.stdin)
    by_language = defaultdict(list)
    for s in sets:
        if s.get("variant"):
            continue  # no variants exist today; they would need their own packages
        if s["source"] == "en":
            language = s["target"]
        elif s["target"] == "en":
            language = s["source"]
        else:
            print(f"skipping {s['source']}-{s['target']}: not paired with English",
                  file=sys.stderr)
            continue
        by_language[language].append(s)

    for language in sorted(by_language):
        members = by_language[language]
        if any(s["last_modified"] is None for s in members):
            sys.exit(f"{language}: a record has no last_modified; cannot version it")
        top = max((s["version"] for s in members), key=version_key)
        newest = max(s["last_modified"] for s in members)
        stamp = time.strftime("%Y%m%d", time.gmtime(newest / 1000))
        name = "dragomand-model-" + language.lower()
        pairs = sorted(f"{s['source']}-{s['target']}" for s in members)
        label = LANGUAGE_NAMES.get(language, language)
        print("\t".join([name, label, f"{top}.{stamp}", *pairs]))


if __name__ == "__main__":
    main()
