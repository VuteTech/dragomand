#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Translate the current text selection and show the result as a
# notification. Bind this to a global shortcut (KDE: System Settings ->
# Shortcuts -> Add Command).
#
# Usage: translate-selection.sh [SRC] [TRG]   (defaults: bg en)

set -euo pipefail

src="${1:-bg}"
trg="${2:-en}"

if [ -n "${WAYLAND_DISPLAY:-}" ] && command -v wl-paste >/dev/null; then
    text="$(wl-paste --primary --no-newline 2>/dev/null || wl-paste --no-newline)"
elif command -v xclip >/dev/null; then
    text="$(xclip -o -selection primary 2>/dev/null || xclip -o -selection clipboard)"
else
    notify-send "Dragomand" "install wl-clipboard or xclip" 2>/dev/null || true
    exit 1
fi

[ -n "$text" ] || exit 0
translated="$(printf '%s\n' "$text" | dragomanctl translate -f "$src" -t "$trg")"

if command -v notify-send >/dev/null; then
    notify-send --app-name=Dragomand "Translated to $trg" "$translated"
else
    printf '%s\n' "$translated"
fi
