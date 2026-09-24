# Desktop integrations

Small examples that only need `dragomanctl` on `PATH`. Everything here
talks to the daemon over D-Bus, so the first use downloads the models and
later uses are offline; the daemon loads models on demand and exits when
idle.

The two larger integrations are independent projects in their own
repositories:

- **LibreOffice extension** (`dragoman-libreoffice`): a Translate menu
  and toolbar for Writer, rich-text aware, direction detection from the
  document language.
- **KTextEditor plugin** (`dragoman-ktexteditor`): native in-place
  selection translation for Kate, KWrite and KDevelop, with
  install-on-demand progress in the editor.

The KRunner and GNOME Shell services live in this repository as
workspace crates (`crates/dragoman-krunner`,
`crates/dragoman-search-provider`) and ship with the main package.

## Global shortcut: translate the selection

`translate-selection.sh [SRC] [TRG]` reads the primary selection
(Wayland: `wl-clipboard`; X11: `xclip`), translates, and shows a
notification. On KDE: System Settings, Shortcuts, Add Command.

## Klipper action / clipboard translation

`translate-clipboard.sh [SRC] [TRG]` translates the clipboard in place
and notifies. In Klipper: Configure Klipper, Actions, Add Action, with
`/path/to/translate-clipboard.sh bg en` as the command; or bind it to a
shortcut.

## Kate without compiling anything

The native plugin in the `dragoman-ktexteditor` repository is the real
integration; the External Tools recipe below needs no compiling and
works in any stock Kate. Under Settings, Configure Kate, External
Tools, Add:

- Executable: `dragomanctl`
- Arguments:  `translate -f bg -t en`
- Input:      `%{Document:selection}`
- Output:     `Replace selected text`

The selection is piped through the daemon and replaced with the
translation.

## KRunner

`dragoman-krunner` is a small separate D-Bus service implementing
`org.kde.krunner1`: type `tr bg en добро утро` in KRunner and the
translation appears as a result; activating it copies the translation
to the clipboard. The package installs its registration to
`/usr/share/krunner/dbusplugins/` and KRunner activates the service on
demand over D-Bus.

## GNOME Shell search

`dragoman-search-provider` implements `org.gnome.Shell.SearchProvider2`
the same way: search `tr bg en добро утро` (or `tr text` for the
default pair) in the Activities overview; activating the result copies
the translation. Registered via
`/usr/share/gnome-shell/search-providers/`, D-Bus activated, exits when
unused. Queries that don't start with `tr ` never touch the daemon.

## Flatpak clients

A sandboxed client needs permission to talk to the daemon:

```
flatpak override --user --talk-name=dev.l10n_bg.dragomand.Translator1 <app-id>
```

(or `--talk-name=dev.l10n_bg.dragomand.Translator1` in the app's
manifest `finish-args`.)
