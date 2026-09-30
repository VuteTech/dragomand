# D-Bus API

This page is for application developers. The API is small, asynchronous
and provider-neutral: clients see language pairs, versions and requests,
never engine or model internals.

| | |
|---|---|
| Bus | Session bus |
| Bus name | `dev.l10n_bg.dragomand.Translator1` |
| Object path | `/dev/l10n_bg/dragomand/Translator1` |
| Main interface | `dev.l10n_bg.dragomand.Translator1` |
| Request interface | `dev.l10n_bg.dragomand.Request1` |

The daemon is D-Bus activated: calling any method starts it. The full
machine-readable contract is installed at
`/usr/share/dbus-1/interfaces/dev.l10n_bg.dragomand.Translator1.xml`
(`data/dbus/` in the source tree), and a test keeps it identical to the
daemon's live introspection.

The trailing `1` in the names is the API major version; it only changes
for incompatible revisions. Compatible additions (new methods, options
and result keys) arrive within version 1; the list of them is under
[Additions](#additions), and a client can tell from the daemon's
introspection whether it has them.

## The request pattern

Anything slow (translating, downloading, loading a model) never blocks a
method call. Slow methods return the object path of a request immediately,
and the outcome arrives as signals on that object. This is the same
pattern the XDG desktop portals use:

1. Generate a `handle_token`, a short unique string, and pass it in the
   method's `options`.
2. Compute the request path yourself:
   `/dev/l10n_bg/dragomand/request/<sender>/<token>`, where `<sender>` is
   your connection's unique name with every non-alphanumeric character
   replaced by `_`.
3. Subscribe to the `Response` signal on that path **before** calling the
   method, so no signal can be missed.
4. Call the method; it returns the same path.
5. Optionally watch `Progress` signals, and call `Cancel` on the request
   to abort.
6. `Response` fires exactly once, then the request object disappears.

### The Request1 interface

```
method Cancel()
signal Progress(fraction d, stage s)
signal Response(code u, results a{sv})
```

Response codes: `0` success, `1` cancelled, `2` error. On error the
`results` dictionary carries `error` (s), a human-readable message.

## Methods

### ListLanguagePairs

```
ListLanguagePairs(out pairs aa{sv})
```

Returns installed pairs and, from cached catalog records (never the
network), available pairs. Per pair: `source` (s), `target` (s),
`installed_version` (s), `available_version` (s), `origin` (s, `system`
or `user`), `size` (t, installed bytes), `architecture` (s). A missing
key means unknown.

Once `CheckForUpdates` has cached Mozilla's model registry, pairs also
carry quality metadata for the installed model (or, for a pair that is
not installed, the model an install would fetch): `release_status` (s,
Mozilla's label such as `Release` or `Nightly`) and `quality` (d, the
model's COMET-22 score on the flores200-plus test set, from 0 to 1,
higher is better). The registry and the download provider are joined on
the checksum of the model file, so the numbers describe exactly that
file.

### Translate

```
Translate(in source s, in target s, in segments as, in options a{sv},
          out request o)
```

Translates short text segments. Options: `handle_token` (s), `html` (b,
treat segments as HTML and preserve markup), `allow_pivot` (b, default
true), `priority` (s, `interactive` or `batch`), `sentences` (b, see
below). Results: `translations` (as, same order as the input), and
`pivot` (s) when the daemon pivoted through an intermediate language.

With `sentences` set, the results also carry `sentences` (aa(uuuu)): for
every segment, one entry per sentence with the begin and end of the
sentence in the source and the begin and end of its rendering in the
translation, as offsets in Unicode code points. The engine translates
sentence by sentence, so sentence *i* of a translation always renders
sentence *i* of its source. Editors use this to highlight the matching
sentence on the other side.

Whole documents do not belong in `segments`; D-Bus is the control plane,
not a bulk transport. Documents go through `TranslateFd`.

### TranslateFd

```
TranslateFd(in source s, in target s, in input h, in output h,
            in options a{sv}, out request o)
```

Translates a document passed as a file descriptor: UTF-8 text read from
`input` until end of file (at most 64 MiB), translated line by line and
written to `output`. Lines without a letter or digit (blank lines,
separators) are copied unchanged, so the line structure survives; a
subtitle file keeps its numbering and timestamps when the client passes
only the text lines. Pipes and regular files (for example a `memfd`)
both work; with a file, the daemon writes from the descriptor's current
offset, so rewind before reading the result.

Options: `handle_token` (s), `html` (b), `allow_pivot` (b, default true),
`priority` (s, default `batch`). `Progress` reports the fraction of lines
done. Results: `lines` (u, the number of lines translated) and `pivot`
(s) when pivoting.

### DetectLanguage

```
DetectLanguage(in text s, in options a{sv}, out results a{sv})
```

Identifies the language of `text` (the first 64 KiB), offline, in
microseconds. Options: `candidates` (as) restricts the answer to these
languages; without it, the daemon uses the languages of the installed and
available pairs, which also keeps close relatives apart (Bulgarian and
Macedonian). Results: `language` (s, absent when the text gives nothing
to go on), `confidence` (d, 0 to 1) and `reliable` (b).

### PreparePair

```
PreparePair(in source s, in target s, in options a{sv}, out request o)
```

Installs the pair if missing (network), then loads it. Useful to warm a
pair up front, with download progress via `Progress`. Options:
`handle_token` (s), `allow_pivot` (b, default true). Results: `version`
(s), `pivot` (s).

### InstallPair

```
InstallPair(in source s, in target s, in options a{sv}, out request o)
```

Installs or upgrades one pair from the provider (network) without
loading it. Options: `handle_token` (s). Results: `version` (s),
`architecture` (s).

### RemovePair

```
RemovePair(in source s, in target s)
```

Removes every user-store copy of the pair immediately. System store
copies stay.

### CheckForUpdates

```
CheckForUpdates(in options a{sv}, out request o)
```

Refreshes the catalog cache and Mozilla's model registry (network) and
reports available updates. Options: `handle_token` (s). Results:
`updates` (as), one line per pair naming the installed and the available
version.

### GetStatus

```
GetStatus(out status a{sv})
```

Returns `version` (s), `loaded` (as, the loaded routes) and `queued` (u).
The interface also exposes a read-only `Version` (s) property.

### GetConfig and SetConfig

```
GetConfig(out config a{sv})
SetConfig(in changes a{sv})
signal ConfigChanged(config a{sv})
```

`GetConfig` returns the [configuration](configuration.md):
`memory_budget_mb` (t), `keep_warm` (u), `keep_warm_seconds` (t),
`idle_exit_seconds` (t), `network` (b) and `allow_prerelease` (b).

`SetConfig` changes some of these keys (integers of any width are
accepted), writes them to `config.toml` (keeping the file's comments and
layout) and applies them at once: a lower memory budget evicts idle
models immediately, and the network switch affects the next request. It
is all or nothing: an unknown key or a value out of range fails with
`InvalidArgument` and changes nothing. Every successful change emits
`ConfigChanged` with the whole new configuration, so settings screens
stay in sync.

## Additions

Added within API version 1, after the first release: `TranslateFd`,
`DetectLanguage`, `GetConfig`, `SetConfig` and `ConfigChanged`; the
`Translate` option `sentences`; the pair keys `release_status` and
`quality`. Older daemons answer the new methods with
`org.freedesktop.DBus.Error.UnknownMethod` and ignore the new option.

## Errors

Methods fail with `dev.l10n_bg.dragomand.Error.` errors:

| Error | Meaning |
|---|---|
| `InvalidArgument` | Malformed input, or a request over the size limits. |
| `UnsupportedPair` | No model exists for the pair, even with pivoting. |
| `NotInstalled` | The pair is not installed and could not be installed. |
| `LimitExceeded` | Per-client request count or concurrency limit hit. |
| `NetworkDisabled` | The operation needs the network and `network = false` is configured. |
| `EngineFailure` | The translation engine failed. |

## Limits

Clients are treated as untrusted. Per request: at most 256 segments and
1 MiB of text for `Translate`, 64 MiB for a `TranslateFd` document. Per client: bounded request count and concurrency. Requests
of a client that disconnects are cancelled and cleaned up automatically.

Interactive requests are scheduled ahead of batch ones, so a bulk job
never blocks a keystroke-sized translation from another application.

## Trying it from the shell

```sh
busctl --user call dev.l10n_bg.dragomand.Translator1 \
    /dev/l10n_bg/dragomand/Translator1 \
    dev.l10n_bg.dragomand.Translator1 GetStatus
```

`busctl` also does introspection:

```sh
busctl --user introspect dev.l10n_bg.dragomand.Translator1 \
    /dev/l10n_bg/dragomand/Translator1
```

## Reference clients

- **Rust**: the `dragoman-client` crate in the source tree provides typed
  proxies and the shared request handling; `dragomanctl` is built on it.
- **Qt / C++**: [libdragoman-qt](https://github.com/VuteTech/libdragoman-qt)
  is a shared QtDBus library with the full request pattern (jobs with
  cancellation, install on demand through `PreparePair` with progress)
  and a fake daemon for autotests;
  [Krakoman](https://github.com/VuteTech/krakoman) is built on it.
- Sandboxed (Flatpak) clients need
  `--talk-name=dev.l10n_bg.dragomand.Translator1`.
