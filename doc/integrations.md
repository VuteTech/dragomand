# Desktop integrations

Dragomand is a service, so "integration" usually means a few lines of
glue. Everything below talks to the same daemon: the first use of a pair
downloads its model, later uses are offline, and warm models are shared
across all of it.

The shell scripts live in the `integrations/` directory of the source
tree and only need `dragomanctl` on `PATH`.

## Translate the current selection (global shortcut)

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

## Translate the clipboard (Klipper)

`integrations/translate-clipboard.sh [SRC] [TRG]` translates the
clipboard contents in place and notifies. In Klipper, open Configure
Klipper, go to Actions, choose Add Action and use the script as the
command; or bind the script to a shortcut directly.

## KRunner

The package ships `dragoman-krunner`, a D-Bus activated KRunner plugin.
Press the KRunner shortcut (Alt+Space by default) and type:

```
tr bg en добро утро
```

The translation appears as a result; activating it copies the translation
to the clipboard. No setup is needed beyond installing the package.

## GNOME Shell search

`dragoman-search-provider` does the same for the GNOME Activities
overview. Search for:

```
tr bg en добро утро
```

or `tr some text` for the default pair. Activating the result copies the
translation. The provider is D-Bus activated and exits when unused, and
queries that do not start with `tr ` never reach it.

## LibreOffice

The extension (`.oxt`) is developed as its own project,
`dragoman-libreoffice`. From a checkout of it, build and install with:

```sh
./build-oxt.sh
unopkg add dist/dragoman-libreoffice.oxt
```

or add the file through the Extension Manager in the Tools menu.

What you get:

- A **Translate** submenu in the Tools menu and a **Translate toolbar**
  (enable it under View, Toolbars, Translate): translate the selection,
  choose languages, swap direction.
- In the Tabbed ("ribbon") interface the three commands appear as buttons
  in the **Extension** tab.
- A language chooser listing every available language by name, in
  editable dropdowns, so codes never need typing. The chosen pair is
  remembered in `~/.config/dragomand/libreoffice.json`.
- **Rich text support.** Selections with links or character formatting
  are translated as HTML, so hyperlinks stay attached to the translated
  words and bold or italic runs survive. Plain selections take a faster
  path. Input is also normalized and cleaned of invisible artifacts that
  web copies drag along (soft hyphens, zero-width characters, mixed
  Cyrillic and Latin lookalike letters) before translating.
- **Direction that follows the document.** "Translate selection" reads
  the language Writer has set on the selected text (the one the spelling
  checker uses, set under Format, Character, Language). Text in the saved
  source language translates normally, text in the saved target language
  reverses the direction, any other recognized language translates into
  the saved target, and text without a language uses the saved pair.

The first use of a pair downloads its model, which can take a few
seconds; LibreOffice waits meanwhile.

## KTextEditor plugin (Kate, KWrite, KDevelop)

The `dragoman-ktexteditor` project is a native plugin for the editor
framework behind Kate, KWrite and KDevelop. Unlike the script-based
recipes it
talks D-Bus directly, shows download progress in the editor and never
loses your text: if the document changes while a translation is in
flight, the result goes to the clipboard instead of overwriting the edit.

Once enabled (Settings, Configure, Plugins, then check **Dragoman
Translator**), the Tools menu gains:

- **Translate Selection** (Ctrl+Alt+T): translates with the saved pair;
  when the selection's script (Cyrillic versus Latin) clearly points the
  other way, the direction reverses automatically.
- **Translate Selection (Choose Languages)**: pick the pair from what the
  daemon reports, save it, translate.
- **Swap Translation Direction**.

Empty lines in the selection are preserved. Selections are capped at the
daemon's per-request limits (256 lines, 1 MiB); run bigger jobs through
`dragomanctl`.

Building needs Qt 6, KDE Frameworks 6 and extra-cmake-modules, from a
`dragoman-ktexteditor` checkout:

```sh
cmake -S . -B build -G Ninja
cmake --build build
sudo cmake --install build
```

Distributions should package it separately from the daemon so the daemon
package never pulls KDE dependencies.

## Kate without compiling anything

Any stock Kate can do in-place selection translation through External
Tools. Open Settings, Configure Kate, External Tools, choose Add, and
enter:

| Field | Value |
|---|---|
| Executable | `dragomanctl` |
| Arguments | `translate -f bg -t en` |
| Input | `%{Document:selection}` |
| Output | Replace selected text |

## Flatpak applications

A sandboxed client needs permission to talk to the daemon:

```sh
flatpak override --user --talk-name=dev.l10n_bg.dragomand.Translator1 <app-id>
```

Application authors add the same `--talk-name` to their manifest's
`finish-args` instead.

## Anything else

Any language that can call D-Bus, or simply run `dragomanctl`, can
integrate. A shell one-liner is a working integration:

```sh
echo "Добро утро" | dragomanctl translate -f bg -t en
```

For proper asynchronous clients with progress reporting, see the
[D-Bus API](dbus-api.md).
