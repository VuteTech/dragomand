---
title: Dragomand
framed: true
---

**Dragomand** is a per-user Linux daemon that gives desktop applications
fully offline machine translation over D-Bus. It runs Mozilla's
Bergamot/Marian translation models locally, the ones behind Firefox
Translations, and manages which models are installed and which stay
loaded in memory.

```sh
$ dragomanctl translate -f bg -t en "Добро утро. Как си днес?"
Good morning. How are you today?
```

The first call downloads the model from Mozilla (verified by SHA-256)
and starts the daemon through D-Bus activation. Everything afterwards is
fully offline, and the daemon exits again when idle.

## The Daemon

The existing Linux options are either standalone applications or
translation tied to a single host such as Firefox. Dragomand is one
shared service instead:

- **One API for every app.** Editors, browsers, clipboard tools,
  KRunner, GNOME Shell, LibreOffice and accessibility software all call
  the same D-Bus interface: `dev.l10n_bg.dragomand.Translator1`.
- **One copy of every model.** The Bergamot engine keeps models in
  private memory, so every application embedding it would pay for its
  own copy. A shared daemon loads each model once.
- **Current models.** Models come from Firefox Remote Settings, exactly
  what Firefox Translations installs, in every language pair Mozilla
  publishes.
- **Private by design.** Text never leaves the machine. The network is
  used only to download models, and updates are checked only when you
  ask.
- **No idle cost.** The daemon is D-Bus activated and exits after an
  idle period; nothing runs permanently.

## Currently implemented

Version 0.1.0: the daemon with D-Bus API v1, the `dragomanctl`
command-line client, install-on-demand model downloads, LRU keep-warm
with memory-pressure eviction, KRunner and GNOME Shell search
integration, a LibreOffice extension and a KTextEditor (Kate) plugin.
Bulgarian and English form the first pair working end to end; every
pair Mozilla publishes is in scope.

Get started: [Install](install.md) &middot;
[Integrations](integrations.md) &middot;
[Documentation](quickstart.md)
