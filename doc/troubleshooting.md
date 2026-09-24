# Troubleshooting

## Where things live

| What | Where |
|---|---|
| Configuration | `~/.config/dragomand/config.toml` |
| Downloaded models | `~/.local/share/dragomand/models` |
| Packaged models | `/usr/share/dragomand/models` |
| Daemon logs | the journal: `journalctl --user -u dragomand.service` |
| Daemon binary | `/usr/lib/dragomand/dragomand` (not in `PATH`; started by D-Bus) |

## The daemon is not running

That is normal. Dragomand is D-Bus activated and exits after a short idle
period; it is not supposed to be running when nothing uses it. Any
request starts it:

```sh
dragomanctl status
```

If that fails, look at the journal:

```sh
journalctl --user -u dragomand.service -e
```

## The daemon refuses to start after editing the configuration

A malformed `config.toml`, including an unknown key, is a hard error by
design, so typos cannot be silently ignored. The journal names the file
and the offending line. Fix or remove the file; every setting has a
default.

## Downloads fail

- `dragomanctl update --check` refreshes the catalog and is a quick
  network test.
- If the error says `NetworkDisabled`, you (or your configuration
  management) set `network = false` in the configuration. Installed
  models keep working; installs and update checks do not.
- Behind a proxy, the standard `https_proxy` environment variable must be
  visible to the daemon. Set it for the systemd user session
  (`systemctl --user set-environment https_proxy=...`), not just in your
  shell, because D-Bus activation does not read your shell profile.

## "Unsupported pair"

No model exists for the requested pair, even via pivoting through
English. `dragomanctl pairs --available` shows what Mozilla currently
publishes. Language codes are BCP-47 (`bg`, not `bul`).

## Translations look wrong for a pair without a direct model

Requests between two non-English languages usually pivot through English,
which compounds the errors of two models. Dragomand always tells you when
it pivots (`dragomanctl` prints a notice on standard error; the D-Bus
result carries a `pivot` key). Use `--no-pivot` if you would rather fail
than pivot.

## A model seems corrupted

```sh
dragomanctl store verify          # checks sizes and checksums
dragomanctl remove bg-en          # then reinstall the bad pair
dragomanctl install bg-en
```

If a packaged model in the system store fails verification, reinstall the
distribution package instead.

## A request is rejected with a limit error

Per request the daemon accepts at most 256 segments and 1 MiB of text,
and each client has request count and concurrency limits. Split large
jobs, or feed files through `dragomanctl translate --batch`, which
streams them in daemon-sized chunks.

## Memory use is too high (or eviction is too eager)

Model memory is bounded by `memory_budget_mb` (default 512 MiB, roughly
two loaded pairs). Lower it on small machines, raise it if pairs you
alternate between keep being evicted. Under system-wide memory pressure
the daemon unloads models immediately regardless of the budget. See
[Configuration](configuration.md).

## A Flatpak application cannot reach the daemon

The sandbox blocks the bus name by default:

```sh
flatpak override --user --talk-name=dev.l10n_bg.dragomand.Translator1 <app-id>
```

## First use of a pair is slow

The first request downloads the model (tens of megabytes) and loads it (a
few seconds). Both are one-time costs; afterwards the pair is on disk,
and stays warm in memory between requests. To take the download out of
the interactive path, prefetch with `dragomanctl install`.

## Reporting bugs

Include the output of:

```sh
dragomanctl --json status
dragomanctl store list
journalctl --user -u dragomand.service -e
```

and the daemon version from `dragomanctl status`.
