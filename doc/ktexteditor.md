# KTextEditor plugin

The Dragoman KTextEditor plugin translates the selected text in place,
offline, through the Dragomand daemon. KTextEditor is the editor
component behind Kate, KWrite and KDevelop, so one plugin serves all of
them. It adds three commands to the Tools menu, downloads a missing
language pair on first use with its progress shown in the editor, and
never overwrites text you changed while a translation was running.

It is developed as its own project,
[dragoman-ktexteditor](https://github.com/VuteTech/dragoman-ktexteditor),
and packaged in the same repository as Dragomand. It is a separate
package, so installing the daemon never pulls in KDE dependencies.

## Requirements

**Dragomand.** The plugin does no translation itself: it talks to the
daemon over D-Bus, and the daemon starts on demand. The package depends
on `dragomand`, so installing the plugin installs the daemon too.

**KDE Frameworks 6.** The plugin is built for Qt 6 and KDE Frameworks 6
(6.13 or newer), which is what current Kate and KDevelop releases use.
The Kate Flatpak cannot load plugins installed on the system; use Kate
from your distribution.

## Install

Add the Dragomand repository first, as described on the
[Install](install.md) page, then install the `dragoman-ktexteditor`
package:

=== "openSUSE"

    ```sh
    sudo zypper install dragoman-ktexteditor
    ```

=== "Fedora"

    ```sh
    sudo dnf install dragoman-ktexteditor
    ```

=== "Debian and Ubuntu"

    ```sh
    sudo apt install dragoman-ktexteditor
    ```

=== "Arch Linux"

    ```sh
    sudo pacman -S dragoman-ktexteditor
    ```

Packages are built for x86-64 everywhere and for 64-bit ARM wherever the
repository has ARM builds. Updates arrive with your normal system
updates.

## Enable the plugin

The editor does not enable new plugins by itself:

- **Kate:** open **Settings**, then **Configure Kate**, then **Plugins**,
  check **Dragoman Translator** and click **OK**.
- **KDevelop:** open **Settings**, then **Configure KDevelop**, then
  **Plugins**, and check **Dragoman Translator**.

The Tools menu now has the three translation commands.

## Using it

Select some text and choose **Tools**, then **Translate Selection**, or
press **Ctrl+Alt+T**. The translation replaces the selection.

- **Translate Selection (Choose Languages)** opens a dialog listing every
  language pair the daemon offers, installed or available for download.
  It saves the chosen pair and translates the selection with it. Until
  you choose a pair, it is Bulgarian to English.
- **Swap Translation Direction** reverses the saved pair.
- **The direction follows the text.** For a pair whose languages use
  different scripts, such as Bulgarian and English, Translate Selection
  checks the selected text: mostly Latin text with a Cyrillic source
  language, or the other way round, translates in the reverse direction.
  Selecting English text translates it into Bulgarian without swapping
  first.
- **Your edits are safe.** If the document changes while a translation
  runs, the result goes to the clipboard instead of into the document,
  and a message says so.
- **Empty lines are kept**, so paragraphs stay paragraphs.
- **Pivoting is reported.** When no direct model exists and the daemon
  translates through English, a message names the pivot language.

The first translation in a language pair downloads its model from
Mozilla; a message in the editor shows the progress, and the editor
stays usable meanwhile. After that the pair works offline. To skip the
download, install the pair's model package (see
[Language models](install.md#language-models)).

One request holds at most 256 lines and 1 MiB of text. For longer
documents use `dragomanctl` (see [Command line](cli.md)).

The saved pair lives in the editor's own configuration file, in a
`[Dragoman]` group: `~/.config/katerc` for Kate, `~/.config/kdeveloprc`
for KDevelop.

## Removing

Uncheck **Dragoman Translator** in the editor's Plugins settings, or
remove the package:

=== "openSUSE"

    ```sh
    sudo zypper remove dragoman-ktexteditor
    ```

=== "Fedora"

    ```sh
    sudo dnf remove dragoman-ktexteditor
    ```

=== "Debian and Ubuntu"

    ```sh
    sudo apt remove dragoman-ktexteditor
    ```

=== "Arch Linux"

    ```sh
    sudo pacman -R dragoman-ktexteditor
    ```

The daemon and its models stay installed.

## Build from source

Building needs CMake 3.24, extra-cmake-modules, Qt 6.8 and KDE
Frameworks 6.13 or newer (Config, CoreAddons, I18n, TextEditor and
XmlGui). From a `dragoman-ktexteditor` checkout:

```sh
cmake -S . -B build -G Ninja -DBUILD_TESTING=ON
cmake --build build
ctest --test-dir build
sudo cmake --install build
```

Restart the editor afterwards and enable the plugin as described above.

## Troubleshooting

**Dragoman Translator is not in the Plugins list.** Restart the editor
completely: close every window, then start it again. Check that the
package is installed and that you are not running the Flatpak build.

**The Tools menu has no translation commands.** The plugin is installed
but not enabled; see [Enable the plugin](#enable-the-plugin).

**"Select the text to translate first."** Select text before translating.

**"Block selections cannot be translated."** Switch off block selection
mode (**Ctrl+Shift+B**) and select the text again.

**"The selection is too large for one request."** Translate the text in
smaller pieces, or the whole file with `dragomanctl`.

**"The translation daemon reports no language pairs."** The daemon is
unreachable or has no provider. Check it from a terminal:

```sh
dragomanctl status
dragomanctl pairs
```

**"Translation failed: ..."** The message comes from the daemon. On a
first use it usually means the model download failed: check the network,
or install the pair ahead of time with `dragomanctl install bg-en` (or its
model package), then try again.

**The wrong direction comes out.** The script check only reverses pairs
whose languages use different scripts. Choose the pair explicitly with
Translate Selection (Choose Languages), or use Swap Translation
Direction.

For problems with the daemon itself, see
[Troubleshooting](troubleshooting.md).
