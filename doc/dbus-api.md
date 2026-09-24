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
for incompatible revisions.

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

### Translate

```
Translate(in source s, in target s, in segments as, in options a{sv},
          out request o)
```

Translates short text segments. Options: `handle_token` (s), `html` (b,
treat segments as HTML and preserve markup), `allow_pivot` (b, default
true), `priority` (s, `interactive` or `batch`). Results: `translations`
(as, same order as the input), and `pivot` (s) when the daemon pivoted
through an intermediate language.

Whole documents do not belong in `segments`; D-Bus is the control plane,
not a bulk transport. A file descriptor based `TranslateFd` is planned
for documents.

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

Refreshes the catalog cache (network) and reports available updates.
Options: `handle_token` (s). Results: `updates` (as), one line per pair
naming the installed and the available version.

### GetStatus

```
GetStatus(out status a{sv})
```

Returns `version` (s), `loaded` (as, the loaded routes) and `queued` (u).
The interface also exposes a read-only `Version` (s) property.

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
1 MiB of text. Per client: bounded request count and concurrency. Requests
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
- **Qt / C++**: `dragomanclient.{h,cpp}` in the `dragoman-ktexteditor`
  project is a compact QtDBus implementation of the full request
  pattern, including install on demand through `PreparePair` with
  progress reporting.
- Sandboxed (Flatpak) clients need
  `--talk-name=dev.l10n_bg.dragomand.Translator1`.
