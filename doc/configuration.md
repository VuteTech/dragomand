# Configuration

The daemon reads a single optional file at startup:

```
~/.config/dragomand/config.toml
```

(`$XDG_CONFIG_HOME/dragomand/config.toml` when `XDG_CONFIG_HOME` is set.)

Every setting has a sensible default and the file may be absent entirely.
A malformed file or an unknown key is a hard error: the daemon refuses to
start rather than silently ignore a typo, and the reason appears in the
journal.

Because the daemon exits when idle, configuration changes take effect
soon on their own. To apply one immediately:

```sh
systemctl --user stop dragomand.service
```

The next request starts a fresh daemon with the new settings.

## All settings and their defaults

```toml
# ~/.config/dragomand/config.toml

# Rough memory budget for loaded models, in MiB. One active pair costs
# about 260 MiB, so the default fits two.
memory_budget_mb = 512

# How many recently used routes stay warm when idle.
keep_warm = 2

# How long an unused route stays warm, in seconds.
keep_warm_seconds = 600

# The daemon exits this long after the last activity, once nothing is
# loaded any more. Seconds.
idle_exit_seconds = 60

# Master network switch. false disables downloads and update checks
# entirely; installed models keep working.
network = true

# Opt into Mozilla's pre-release models.
allow_prerelease = false
```

## What the settings mean in practice

**`memory_budget_mb`**
:   The ceiling for model memory. When loading another pair would exceed
    it, the least recently used pair is evicted first. Lower it on small
    machines (256 keeps one pair warm); raise it if you switch between
    many pairs and have memory to spare.

**`keep_warm` and `keep_warm_seconds`**
:   A loaded model answers in tens of milliseconds; loading one takes a
    few seconds. These two settings control how many recently used pairs
    stay loaded while idle and for how long. The pair in active use is
    always kept.

**`idle_exit_seconds`**
:   How quickly the daemon leaves memory entirely once idle and empty.
    Exiting is cheap: D-Bus activation restarts it transparently on the
    next request.

**`network`**
:   With `network = false`, any operation that would go online fails with
    a clear error (`NetworkDisabled`) instead. Use this for strictly
    offline machines fed by model packages or a pre-filled store.

**`allow_prerelease`**
:   Mozilla marks some model versions as pre-releases. They are skipped by
    default; set `true` to test them.

## Memory pressure

Independently of the budget, the daemon watches system memory pressure
(kernel PSI, and systemd memory pressure signals) and unloads models
immediately when the system is struggling. No configuration is needed;
the budget is the steady-state limit, pressure handling is the emergency
brake.
