# Command line reference

`dragomanctl` is the command line client for the `dragomand` daemon.
Every command except the `store` group talks to the daemon over the
session bus and starts it automatically when needed.

```
dragomanctl [--json] <command> [options]
```

**Global options**

| Option | Meaning |
|---|---|
| `--json` | Machine-readable JSON output on standard output. Available on every command. |

**Exit codes**

| Code | Meaning |
|---|---|
| 0 | Success. |
| 1 | Runtime failure: daemon error, verification failure, network trouble. |
| 2 | Usage error. |

Languages are BCP-47 codes (`bg`, `en`, `de`, `zh-Hans`). Pairs are
written `SRC-TRG`, for example `bg-en`; a script or region subtag stays
with its language, so `zh-Hans-en` is Simplified Chinese to English.

---

## translate

```
dragomanctl translate --from SRC --to TRG [options] [TEXT...]
```

Translates the given text segments, or standard input when no `TEXT` is
given (one segment per line). If the language pair is not installed, it is
downloaded first.

| Option | Meaning |
|---|---|
| `-f`, `--from SRC` | Source language. |
| `-t`, `--to TRG` | Target language. |
| `--html` | Treat the input as HTML and preserve the markup in the translation. |
| `--no-pivot` | Fail instead of pivoting through English when no direct model exists. |
| `--batch` | Queue behind interactive requests from other applications. |

When the daemon pivots through English, it says so on standard error, so
scripts capturing standard output are unaffected.

## pairs

```
dragomanctl pairs [--installed | --available]
```

Lists language pairs with their install state, versions and origin
(system or user store). With no flag it lists both installed pairs and
pairs available from the provider's cached catalog. The list works
offline; refresh the catalog with `dragomanctl update --check`.

| Option | Meaning |
|---|---|
| `--installed` | Only pairs installed locally. |
| `--available` | Only pairs known from the provider catalog. |

## install

```
dragomanctl install PAIR...
```

Installs or upgrades one or more pairs from the provider, verifying every
file's checksum before it is placed in the user store. Example:
`dragomanctl install bg-en en-bg`.

## remove

```
dragomanctl remove PAIR...
```

Removes pairs from the user store. Read-only system store copies
(installed by distribution packages) are not touched.

## update

```
dragomanctl update [--check]
```

Refreshes the model catalog from the network and installs every available
update. With `--check` it only reports what is newer, without installing.

## status

```
dragomanctl status
```

Shows the daemon version, currently loaded routes, queued jobs and the
approximate memory used by loaded models. Starts the daemon if it is not
running.

## store

```
dragomanctl store <subcommand>
```

Direct access to the model stores on disk, without the daemon. Meant for
packagers, offline maintenance and debugging; normal use goes through the
commands above.

### store list

Lists every installed model in every store, with version, architecture
and origin.

### store verify

```
dragomanctl store verify [PAIR...]
```

Re-verifies installed models (file sizes and sha256 checksums) against
their manifests. Verifies everything when no pairs are given. Exits 1
when anything fails.

### store available

```
dragomanctl store available
```

Lists every pair the model provider offers, with the version and
architecture that `store install` would fetch. It contacts the provider
(Mozilla's Remote Settings) but not the daemon; with `--json` each entry
also carries the date of its newest record, which the model packages use
for their version.

### store install

```
dragomanctl store install [--root DIR] PAIR...
```

Downloads and installs pairs into a store directory, the user store by
default. `--root` targets any directory and is how model packages fill a
system store:

```sh
dragomanctl store install --root "$pkgdir/usr/share/dragomand/models" bg-en
```

### store remove

```
dragomanctl store remove PAIR...
```

Removes pairs from the user store, without contacting the daemon.

## completions

```
dragomanctl completions <shell>
```

Prints shell completions to standard output for `bash`, `zsh`, `fish`,
`elvish` or `powershell`. Packages usually install these already; for a
manual setup:

```sh
dragomanctl completions zsh > ~/.local/share/zsh/site-functions/_dragomanctl
```

A man page, `dragomanctl(1)`, is generated from the same command
definitions and ships with packages.
