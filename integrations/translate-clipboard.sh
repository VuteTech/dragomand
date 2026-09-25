#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
# SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Translate the clipboard in place: read it, translate, write the result
# back, and notify. Works as a Klipper action (KDE: Klipper settings ->
# Actions -> Add Action -> command "translate-clipboard.sh bg en") or a
# global shortcut.
#
# Usage: translate-clipboard.sh [SRC] [TRG]   (defaults: bg en)

set -euo pipefail

src="${1:-bg}"
trg="${2:-en}"

if [ -n "${WAYLAND_DISPLAY:-}" ] && command -v wl-paste >/dev/null; then
    read_cmd=(wl-paste --no-newline)
    write_cmd=(wl-copy)
elif command -v xclip >/dev/null; then
    read_cmd=(xclip -o -selection clipboard)
    write_cmd=(xclip -i -selection clipboard)
else
    notify-send "Dragomand" "install wl-clipboard or xclip" 2>/dev/null || true
    exit 1
fi

text="$("${read_cmd[@]}")"
[ -n "$text" ] || exit 0
translated="$(printf '%s\n' "$text" | dragomanctl translate -f "$src" -t "$trg")"
printf '%s' "$translated" | "${write_cmd[@]}"
if command -v notify-send >/dev/null; then
    notify-send --app-name=Dragomand "Clipboard translated to $trg" "$translated" || true
fi
