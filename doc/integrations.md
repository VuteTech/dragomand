# Desktop integrations

Dragomand is a service, so "integration" usually means a few lines of
glue. Everything below talks to the same daemon: the first use of a pair
downloads its model, later uses are offline, and warm models are shared
across all of it.

| Integration | Where it works | Setup |
|---|---|---|
| [Selection shortcut](#selection) | Any application | A shell script bound to a key |
| [Clipboard](#clipboard) | Any application, through Klipper | A shell script as a Klipper action |
| [KRunner](#krunner) | KDE Plasma | None, ships with Dragomand |
| [GNOME Shell search](#gnome-shell) | GNOME | None, ships with Dragomand |
| [LibreOffice](#libreoffice) | LibreOffice Writer | An extension to download |
| [KTextEditor plugin](#ktexteditor) | Kate, KWrite, KDevelop | A separate package |
| [Krakoman](#krakoman) | Any desktop, best on KDE Plasma | A separate package |
| [System Settings module](#system-settings) | KDE Plasma | A separate package |
| [Plasma widget](#plasma-widget) | KDE Plasma | A separate package |
| [Flatpak applications](#flatpak) | Sandboxed applications | One permission |
| [Anything else](#anything-else) | Scripts, other programs | `dragomanctl` or D-Bus |

The shell scripts live in the `integrations/` directory of the source
tree and only need `dragomanctl` on `PATH`.

## Translate the current selection (global shortcut) {#selection}

`integrations/translate-selection.sh [SRC] [TRG]` reads the primary
selection (using `wl-clipboard` on Wayland or `xclip` on X11), translates
it and shows the result as a desktop notification.

On KDE Plasma, open System Settings, go to Shortcuts, choose Add Command,
and enter for example:

```sh
/path/to/translate-selection.sh bg en
```

Bind it to a key, select any text anywhere, press the key, read the
translation in the notification.

## Translate the clipboard (Klipper) {#clipboard}

`integrations/translate-clipboard.sh [SRC] [TRG]` translates the
clipboard contents in place and notifies. In Klipper, open Configure
Klipper, go to Actions, choose Add Action and use the script as the
command; or bind the script to a shortcut directly.

## KRunner {#krunner}

The package ships `dragoman-krunner`, a D-Bus activated KRunner plugin.
Press the KRunner shortcut (Alt+Space by default) and type:

```
tr bg en добро утро
```

The translation appears as a result; activating it copies the translation
to the clipboard. No setup is needed beyond installing the package.

## GNOME Shell search {#gnome-shell}

`dragoman-search-provider` does the same for the GNOME Activities
overview. Search for:

```
tr bg en добро утро
```

or `tr some text` for the default pair. Activating the result copies the
translation. The provider is D-Bus activated and exits when unused, and
queries that do not start with `tr ` never reach it.

## LibreOffice {#libreoffice}

The LibreOffice extension translates the selection in Writer: a
Translate submenu in the Tools menu and a toolbar, a language chooser,
links and formatting kept through the translation, and a direction that
follows the language Writer has set on the text. It is its own project,
[dragoman-libreoffice](https://github.com/VuteTech/dragoman-libreoffice),
distributed as an `.oxt` file on its GitHub releases.

Requirements, download, installation, updates and troubleshooting are
on the [LibreOffice extension](libreoffice.md) page.

## KTextEditor plugin (Kate, KWrite, KDevelop) {#ktexteditor}

A native plugin for the editor component behind Kate, KWrite and
KDevelop. The Tools menu gains Translate Selection (Ctrl+Alt+T), a
language chooser and a swap of the direction; a missing pair downloads
on first use with its progress shown in the editor, and text you edit
while a translation runs is never overwritten. It is its own project,
[dragoman-ktexteditor](https://github.com/VuteTech/dragoman-ktexteditor),
packaged as `dragoman-ktexteditor` in the same repository as Dragomand.

Installation, usage and troubleshooting are on the
[KTextEditor plugin](ktexteditor.md) page.

## Krakoman {#krakoman}

A graphical translator for the KDE desktop: translation as you type, in
tabs, with the direction detected and pasted formatting kept; documents
and subtitles; text on screen and in images, read with Tesseract; a
history and phrasebook; a tray icon and a global shortcut that
translates the selected text in any application. It is its own project,
[Krakoman](https://github.com/VuteTech/krakoman), packaged as `krakoman`.

Installation, usage and troubleshooting are on the
[Krakoman](krakoman.md) page.

## System Settings module {#system-settings}

The **Offline Translation** page of KDE System Settings: the daemon's
memory budget, keep-warm and network settings, applied live, and the
installed language models with their quality, updates and removal. It
is its own project,
[dragoman-kcm](https://github.com/VuteTech/dragoman-kcm), packaged as
`dragoman-kcm`.

Details are on the [System Settings module](system-settings.md) page.

## Plasma widget {#plasma-widget}

The **Offline Translator** widget translates from the Plasma panel or
the desktop, in a popup. It is its own project,
[dragoman-plasmoid](https://github.com/VuteTech/dragoman-plasmoid),
packaged as `dragoman-plasmoid`.

Details are on the [Plasma widget](plasma-widget.md) page.

## Flatpak applications {#flatpak}

A sandboxed client needs permission to talk to the daemon:

```sh
flatpak override --user --talk-name=dev.l10n_bg.dragomand.Translator1 <app-id>
```

Application authors add the same `--talk-name` to their manifest's
`finish-args` instead.

## Anything else {#anything-else}

Any language that can call D-Bus, or simply run `dragomanctl`, can
integrate. A shell one-liner is a working integration:

```sh
echo "Добро утро" | dragomanctl translate -f bg -t en
```

For proper asynchronous clients with progress reporting, see the
[D-Bus API](dbus-api.md); Qt applications can use the
[Qt client library](libdragoman-qt.md).
