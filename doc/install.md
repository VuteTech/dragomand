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

Every release is built on the
[openSUSE Build Service](https://build.opensuse.org/package/show/home:blago:dragomand/dragomand)
for the distributions below. Add the repository once; after that, new
releases arrive with your normal system updates. Each repository is signed,
and the commands import its key.

Packages are built for x86-64 on every distribution listed; Fedora 44 and
openSUSE Leap 16.0 also get 64-bit ARM packages.

=== "openSUSE Tumbleweed"

    ```sh
    sudo zypper addrepo https://download.opensuse.org/repositories/home:/blago:/dragomand/openSUSE_Tumbleweed/home:blago:dragomand.repo
    sudo zypper refresh
    sudo zypper install dragomand
    sudo zypper install dragomand-model-bg   # optional: Bulgarian models, see below
    ```

    `zypper refresh` asks whether to trust the repository key; answer
    `a` to trust it always.

=== "openSUSE Slowroll"

    ```sh
    sudo zypper addrepo https://download.opensuse.org/repositories/home:/blago:/dragomand/openSUSE_Slowroll/home:blago:dragomand.repo
    sudo zypper refresh
    sudo zypper install dragomand
    sudo zypper install dragomand-model-bg   # optional: Bulgarian models, see below
    ```

    `zypper refresh` asks whether to trust the repository key; answer
    `a` to trust it always.

=== "openSUSE Leap 16.0"

    ```sh
    sudo zypper addrepo https://download.opensuse.org/repositories/home:/blago:/dragomand/16.0/home:blago:dragomand.repo
    sudo zypper refresh
    sudo zypper install dragomand
    sudo zypper install dragomand-model-bg   # optional: Bulgarian models, see below
    ```

    `zypper refresh` asks whether to trust the repository key; answer
    `a` to trust it always.

=== "Fedora 44"

    ```sh
    sudo dnf config-manager addrepo --from-repofile=https://download.opensuse.org/repositories/home:/blago:/dragomand/Fedora_44/home:blago:dragomand.repo
    sudo dnf install dragomand
    sudo dnf install dragomand-model-bg      # optional: Bulgarian models, see below
    ```

    `dnf` shows the repository key's fingerprint on first use and asks
    you to confirm it.

=== "Debian"

    For Debian 13 (trixie):

    If `curl` is missing, install it first with `sudo apt install curl`.

    ```sh
    curl -fsSL https://download.opensuse.org/repositories/home:/blago:/dragomand/Debian_13/Release.key \
      | sudo tee /etc/apt/keyrings/dragomand.asc > /dev/null
    echo 'deb [signed-by=/etc/apt/keyrings/dragomand.asc] https://download.opensuse.org/repositories/home:/blago:/dragomand/Debian_13/ /' \
      | sudo tee /etc/apt/sources.list.d/dragomand.list
    sudo apt update
    sudo apt install dragomand
    sudo apt install dragomand-model-bg      # optional: Bulgarian models, see below
    ```

    On Debian testing or unstable, replace `Debian_13` in both URLs with
    `Debian_Testing` or `Debian_Unstable`.

=== "Ubuntu 26.04"

    If `curl` is missing, install it first with `sudo apt install curl`.

    ```sh
    curl -fsSL https://download.opensuse.org/repositories/home:/blago:/dragomand/xUbuntu_26.04/Release.key \
      | sudo tee /etc/apt/keyrings/dragomand.asc > /dev/null
    echo 'deb [signed-by=/etc/apt/keyrings/dragomand.asc] https://download.opensuse.org/repositories/home:/blago:/dragomand/xUbuntu_26.04/ /' \
      | sudo tee /etc/apt/sources.list.d/dragomand.list
    sudo apt update
    sudo apt install dragomand
    sudo apt install dragomand-model-bg      # optional: Bulgarian models, see below
    ```

=== "Arch Linux"

    Import and locally sign the repository key, add the repository to
    `/etc/pacman.conf`, then install:

    ```sh
    key=$(curl -fsSL https://download.opensuse.org/repositories/home:/blago:/dragomand/Arch/x86_64/home_blago_dragomand_Arch.key)
    fingerprint=$(gpg --quiet --with-colons --import-options show-only --import --fingerprint <<< "$key" \
      | awk -F: '$1 == "fpr" { print $10; exit }')
    sudo pacman-key --add - <<< "$key"
    sudo pacman-key --lsign-key "$fingerprint"

    printf '\n[home_blago_dragomand_Arch]\nServer = https://download.opensuse.org/repositories/home:/blago:/dragomand/Arch/$arch\n' \
      | sudo tee -a /etc/pacman.conf
    sudo pacman -Syu dragomand
    sudo pacman -S dragomand-model-bg        # optional: Bulgarian models, see below
    ```

    The `$arch` in the `Server` line is literal: pacman fills it in.

### Language models

The `dragomand` package holds no models: by default each language pair
downloads from Mozilla the first time you use it, into your home
directory. For machines without network access, or to skip that first
download, install model packages instead. There is one per language,
named `dragomand-model-` plus the language code, holding every direction
between that language and English: `dragomand-model-bg`,
`dragomand-model-de`, `dragomand-model-zh-hans` and so on. Translating
between two non-English languages works through English when both are
installed. Packaged models live in the read-only system store
`/usr/share/dragomand/models` and update with your system.

To install every language at once, install `dragomand-models-all`, a
meta package that depends on all of them (roughly 3 GB), with the same
package manager command as above.

To try the first translation, see the [Quick start](quickstart.md).

On Debian and Ubuntu the BLAS implementation is chosen through the
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
- A plain `cargo build` reports the development version `0.0.0-dev`;
  release versions come from the git tag and are stamped into the release
  tarball.
- To build the Arch package from a checkout instead of using the
  repository, see the comment at the top of `packaging/obs/PKGBUILD`.

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
