# Installation

## Requirements

- Linux with a D-Bus session bus (any modern desktop session has one).
- x86-64 or 64-bit ARM. Distribution packages target the x86-64-v2
  baseline; the engine's int8 hot path detects AVX2 and AVX-512 at runtime,
  so packages stay fast on capable machines.
- Roughly 40 MB of disk per installed language pair.
- Roughly 260 MiB of memory per actively loaded pair. The default memory
  budget is 512 MiB, enough for two warm pairs.

## From a distribution package

An Arch Linux `PKGBUILD` ships in `packaging/obs/`. Until the first
public release it builds against a local tarball of the checkout:

```sh
scripts/make-release-tarball.sh packaging/obs/
cd packaging/obs
makepkg -si
```

Runtime dependencies on Arch: `cblas`, `lapack`, `pcre2`, `dbus`,
`gcc-libs`. On Debian the BLAS implementation is chosen through the
`libblas.so.3` alternatives system; on Fedora, FlexiBLAS is picked up
directly. OpenBLAS is the recommended implementation everywhere.

A package installs:

| File | Path |
|---|---|
| Daemon (not in `PATH`) | `/usr/lib/dragomand/dragomand` |
| Command line client | `/usr/bin/dragomanctl` |
| systemd user unit | `/usr/lib/systemd/user/dragomand.service` |
| Update timer (opt-in) | `/usr/lib/systemd/user/dragomand-update.{service,timer}` |
| D-Bus activation | `/usr/share/dbus-1/services/dev.l10n_bg.dragomand.Translator1.service` |
| D-Bus interface description | `/usr/share/dbus-1/interfaces/dev.l10n_bg.dragomand.Translator1.xml` |
| Icon | `/usr/share/icons/hicolor/scalable/apps/dev.l10n_bg.dragomand.svg` |
| Licenses | `/usr/share/licenses/dragomand/` |

## Nothing to enable

There is no service to enable or start. The daemon is D-Bus activated: the
first call to its bus name starts it, and it exits on its own after a short
idle period. `systemctl --user status dragomand.service` shows it only
while it is actually running.

The weekly model update timer is the one opt-in piece:

```sh
systemctl --user enable --now dragomand-update.timer
```

Without the timer, models are only updated when you run
`dragomanctl update` yourself.

## From source

The source lives at
[github.com/VuteTech/dragomand](https://github.com/VuteTech/dragomand).
The build has two stages: the C++ engine first, then the Rust workspace.

```sh
git clone https://github.com/VuteTech/dragomand.git
cd dragomand

# 1. The engine (slow the first time; ccache helps on rebuilds)
cmake -S engine -B engine/build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build engine/build -j 4

# 2. The daemon and the client
export DRAGOMAN_ENGINE_LIB_DIR=$PWD/engine/build
cargo build --release --locked --workspace
```

Notes:

- The release tarball contains the pinned Mozilla engine sources in
  `engine/vendor/`, so no network access is needed to build.
- The build resolves the CBLAS interface through pkg-config (`cblas`,
  `openblas`, `flexiblas`, in that order) and refuses to configure when
  none is found. Intel MKL is never used.
- For a distributable binary add `-DBUILD_ARCH=x86-64-v2` to the CMake
  configure step.
- `cargo test --workspace` runs the test suite; the daemon tests need
  `dbus-daemon` installed and run on a private bus.

## Models for offline machines

Models normally download on first use. For machines without network
access, either copy a filled user store from another machine, or build a
model package that installs into the read-only system store:

```sh
dragomanctl store install --root "$pkgdir/usr/share/dragomand/models" bg-en en-bg
```

`packaging/examples/dragomand-model-bg-en/PKGBUILD` is a complete example. See
[Managing models](models.md) for how the stores fit together.

## Editor plugin

The KTextEditor plugin for Kate, KWrite and KDevelop is a separate build
with its own dependencies (Qt 6 and KDE Frameworks 6). See
[Desktop integrations](integrations.md#ktexteditor-plugin-kate-kwrite-kdevelop).
