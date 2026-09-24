# Quick start

Everything below assumes `dragomanctl` is installed and you are in a
normal desktop session. You never start the daemon yourself: the first
command that needs it starts it, and it exits again when idle.

## Translate something

```sh
dragomanctl translate --from bg --to en "Добро утро"
```

On first use this downloads the Bulgarian to English model (a few tens of
megabytes), loads it and prints the translation. Every later use of the
pair is fully offline and much faster, because the model is already on
disk and often still warm in memory.

The short flags are `-f` and `-t`:

```sh
dragomanctl translate -f en -t bg "Good morning"
```

## Translate a file or a pipe

Without text arguments, `dragomanctl translate` reads standard input, one
segment per line:

```sh
dragomanctl translate -f bg -t en < notes-bg.txt > notes-en.txt
```

For large jobs add `--batch`, which lets interactive requests from other
applications jump ahead in the queue:

```sh
dragomanctl translate -f bg -t en --batch < book.txt > book-en.txt
```

## See what is available

```sh
dragomanctl pairs               # everything: installed and available
dragomanctl pairs --installed   # only what is on disk
```

The available list comes from cached catalog records, so it works offline
too.

## Install ahead of time

Install on demand is the default, but you can prefetch pairs, for example
before travelling:

```sh
dragomanctl install bg-en en-bg de-en
```

## Check the daemon

```sh
dragomanctl status
```

This prints the daemon version, the loaded routes, the number of queued
jobs and the approximate model memory in use. If the daemon is not
running, that is normal; it will start on the next request.

## Keep models current

```sh
dragomanctl update --check   # only report what is newer
dragomanctl update           # download and install updates
```

Or enable the weekly timer once and forget about it:

```sh
systemctl --user enable --now dragomand-update.timer
```

## Clean up

```sh
dragomanctl remove bg-en
```

This removes the pair from your user store. Pairs installed by
distribution packages stay; remove those with the package manager.

## Next steps

- Bind translation to a global shortcut or use it from KRunner, GNOME
  Shell, LibreOffice or Kate: [Desktop integrations](integrations.md).
- Tune memory use and offline behavior: [Configuration](configuration.md).
- The full command reference: [Command line reference](cli.md).
