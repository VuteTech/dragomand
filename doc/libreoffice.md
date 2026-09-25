# LibreOffice extension

The Dragomand extension for LibreOffice translates the selected text in
Writer offline, through the Dragomand daemon. It adds a **Translate**
submenu to the Tools menu, a Translate toolbar and a language chooser,
keeps links and character formatting through the translation, and picks
the direction from the language Writer has set on the text.

It is developed as its own project,
[dragoman-libreoffice](https://github.com/VuteTech/dragoman-libreoffice),
and for now it is only distributed as a file on its
[GitHub releases](https://github.com/VuteTech/dragoman-libreoffice/releases):
it is not in the LibreOffice extension site or in distribution
repositories yet. Installing it takes three steps: make sure Dragomand
and LibreOffice's Python support are installed, download the extension
file, and add it to LibreOffice.

## Requirements

**Dragomand.** The extension does no translation itself: it calls
`dragomanctl`, which starts the daemon on demand. Install the
`dragomand` package first, as described on the [Install](install.md)
page, and check that it works:

```sh
dragomanctl translate -f bg -t en "Добро утро"
```

**LibreOffice from your distribution.** Use the LibreOffice packaged by
your distribution. The Flatpak and Snap builds run in a sandbox that
cannot start programs installed on the system, so the extension cannot
reach `dragomanctl` from inside them.

**Python scripting for LibreOffice.** The extension is written in Python,
which some distributions package separately from LibreOffice:

=== "Debian and Ubuntu"

    ```sh
    sudo apt install libreoffice-script-provider-python
    ```

=== "Fedora"

    ```sh
    sudo dnf install libreoffice-pyuno
    ```

=== "openSUSE"

    ```sh
    sudo zypper install libreoffice-pyuno
    ```

=== "Arch Linux"

    Nothing to install: the `libreoffice-fresh` and `libreoffice-still`
    packages include Python scripting.

Without it, adding the extension fails with an error: LibreOffice cannot
load its Python part.

## Download

Open the [latest release](https://github.com/VuteTech/dragoman-libreoffice/releases/latest)
and download `dragoman-libreoffice-<version>.oxt`, for example
`dragoman-libreoffice-0.0.1.oxt`. Each release also carries a `.sha256`
file for checking the download. From a terminal:

```sh
version=0.0.1
base=https://github.com/VuteTech/dragoman-libreoffice/releases/download/v$version
curl -LO "$base/dragoman-libreoffice-$version.oxt"
curl -LO "$base/dragoman-libreoffice-$version.oxt.sha256"
sha256sum -c "dragoman-libreoffice-$version.oxt.sha256"
```

The last command must print `OK`.

## Install

Pick one of the two ways. Both install the extension for your user
only.

### With the Extensions dialog

1. Start LibreOffice (any module, for example Writer).
2. Open **Tools**, then **Extensions** (called Extension Manager in
   older LibreOffice versions).
3. Click **Add**, select the downloaded `.oxt` file and confirm.
4. Close the dialog and restart LibreOffice when it offers to, or close
   every LibreOffice window and start it again.

The Extensions dialog now lists "Dragomand offline translation" with its
version.

### From a terminal

Close every LibreOffice window first, then run:

```sh
unopkg add dragoman-libreoffice-0.0.1.oxt
```

and check the result:

```sh
unopkg list | grep -A1 dev.l10n_bg.dragomand.libreoffice
```

It prints the extension's identifier and version.

To install it for every user of the machine instead, add `--shared` and
run the command as root: `sudo unopkg add --shared
dragoman-libreoffice-0.0.1.oxt`.

## Using it

Open a Writer document, select some text and choose **Tools**, then
**Translate**, then **Translate selection**. The translation replaces the
selection.

- **Translate selection (choose languages)...** first opens a dialog with
  every available language by name, then translates with the chosen
  pair. The pair is saved in `~/.config/dragomand/libreoffice.json`;
  until you choose one, it is Bulgarian to English.
- **Swap translation direction** reverses the saved pair.
- **The direction follows the document.** "Translate selection" reads the
  language Writer has set on the selected text (the one the spelling
  checker uses, set under Format, then Character, then Language). Text
  in the saved source language translates as saved, text in the saved
  target language translates the other way, and any other recognized
  language translates into the saved target.
- **Formatting survives.** Links, bold and italic are carried onto the
  translated words.
- **The toolbar.** In the standard interface, enable it under **View**,
  then **Toolbars**, then **Translate**. In the Tabbed interface the three
  commands are buttons in the **Extension** tab.

The first translation in a language pair downloads its model from
Mozilla, which takes a few seconds; LibreOffice waits meanwhile. After
that the pair works offline. To avoid the download altogether, install
the pair's model package (see [Language models](install.md#language-models)).

## Updating

Download the new release and install it the same way. Adding a newer
version through the Extensions dialog or with `unopkg add` replaces the
installed one; there is no need to remove it first. LibreOffice's own
"Check for Updates" does not know about these releases, so watch the
releases page (GitHub can notify you: "Watch", then "Custom", then
"Releases").

## Removing

In the Extensions dialog select "Dragomand offline translation" and click
**Remove**, or close LibreOffice and run:

```sh
unopkg remove dev.l10n_bg.dragomand.libreoffice
```

The saved language pair stays in `~/.config/dragomand/libreoffice.json`;
delete that file too for a clean removal.

## Development builds

Every change to the extension's `master` branch is built automatically.
Signed-in GitHub users can download these builds from the
[Actions](https://github.com/VuteTech/dragoman-libreoffice/actions) tab:
open a run, then download the artifact at the bottom of the page. It is
a `.zip` holding the `.oxt`. Their versions look like `0.0.1.3` (three
changes after 0.0.1), so they install over the release they follow, and
the next release installs over them.

## Troubleshooting

**Adding the extension fails with an error** (`unopkg failed`, or an
error in the Extensions dialog). Python scripting for LibreOffice is
missing: install it (see [Requirements](#requirements)), then add the
extension again.

**There is no Translate submenu.** Restart LibreOffice completely: close
every window, including the Start Center, and check that no LibreOffice
process is left running. If the menu is still missing, check that the
Extensions dialog lists the extension as enabled.

**"dragomanctl not found: is the dragomand package installed?"** The
extension cannot find `dragomanctl`. Install Dragomand, and use your
distribution's LibreOffice rather than the Flatpak or Snap build.

**"select some text first".** Select text in a Writer document before
choosing Translate selection.

**The first translation is slow or times out.** The first use of a pair
downloads its model. Check the network, or install the pair ahead of
time with `dragomanctl install bg-en` (or its model package), then try
again.

**The wrong language comes out.** Check the language Writer has set on
the text (Format, then Character, then Language): the direction follows
it. "Translate selection (choose languages)..." sets the pair used for
everything else.

For problems with the daemon itself, see
[Troubleshooting](troubleshooting.md).
