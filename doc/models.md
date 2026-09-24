# Managing models

## Where models come from

Dragomand installs the models Mozilla publishes for Firefox Translations,
fetched from the same distribution channel Firefox itself uses. That means
the exact models your Firefox would install, for every language pair
Mozilla releases, kept current as Mozilla ships new versions.

Every download is verified: each file's size and sha256 checksum are
checked against the provider's records before the model is installed, and
installation is atomic, so an interrupted download never leaves a broken
model behind.

The models are data, licensed by Mozilla under MPL-2.0. Each installed
model carries a manifest recording its provider, pair, version,
architecture, per-file checksums, license and source URL, so the daemon
needs no network to know exactly what it has.

## The stores

Models live in one user store and any number of read-only system stores.

| Store | Location | Written by |
|---|---|---|
| User store | `~/.local/share/dragomand/models` (`$XDG_DATA_HOME/dragomand/models`) | The daemon and `dragomanctl`. Every download lands here. |
| System stores | `$XDG_DATA_DIRS/dragomand/models`, typically `/usr/share/dragomand/models` | Distribution packages only. Never modified by the daemon. |

When a pair exists in several places, the newest version the engine
supports wins; on a tie, the system copy is preferred. Removing a pair
with `dragomanctl remove` only affects the user store; packaged models are
removed with the package manager.

## Offline behavior

The rule is simple: when a usable model is installed, the network is never
touched. Only two things go online, and both are explicit:

- installing a pair that is not on disk (including install on demand), and
- update checks (`dragomanctl update`, the optional timer, or a client
  calling `CheckForUpdates`).

Setting `network = false` in the [configuration](configuration.md)
disables even those, turning Dragomand into a strictly offline service
that uses only what is already installed.

## Updates

Nothing updates behind your back. The choices, from manual to automatic:

```sh
dragomanctl update --check                            # report only
dragomanctl update                                    # install updates now
systemctl --user enable --now dragomand-update.timer  # weekly, in the background
```

During an upgrade the previous version stays on disk until no loaded
engine uses it, then it is removed.

Pre-release models exist in Mozilla's channel but are skipped by default;
opt in with `allow_prerelease = true` in the configuration if you want to
test them.

## Pivoting

When you request a pair with no direct model, for example Bulgarian to
German, the daemon translates through English using two models. This works
transparently, but it adds latency and compounds translation errors, so
the daemon always reports when it happened: `dragomanctl` prints a notice
on standard error, and D-Bus clients receive the pivot language in the
result. Pass `--no-pivot` (or the `allow_pivot = false` option over D-Bus)
to fail instead.

## Inspecting and repairing

```sh
dragomanctl pairs                # what is installed and what is available
dragomanctl store list           # every model in every store, with origin
dragomanctl store verify         # re-check all files against the manifests
```

If verification reports a damaged pair, reinstall it:

```sh
dragomanctl remove bg-en
dragomanctl install bg-en
```

The `store` subcommands work without the daemon and without a session
bus, which makes them suitable for scripts, containers and package build
roots. See the [command line reference](cli.md#store).
