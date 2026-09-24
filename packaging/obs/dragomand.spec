#
# spec file for package dragomand (openSUSE and Fedora targets on OBS)
#
# SPDX-License-Identifier: GPL-3.0-or-later
#
# @VERSION@ is stamped by packaging/obs/prepare.sh from the release tag.
# The source tarball is the vendored form made by
# `scripts/make-release-tarball.sh --vendor`: it carries every crate and a
# .cargo/config.toml, so the build runs with no network.

# Distribution LTO puts GCC IR into the static libraries (the engine, the
# bundled zstd) and rustc's linker cannot read it; Rust's own thin LTO
# still applies through the workspace profile.
%global _lto_cflags %{nil}
# Marian replaces CMAKE_CXX_FLAGS wholesale and the Rust release profile
# carries no debug info, so there is nothing for a debuginfo package; the
# binaries are stripped at install instead.
%global debug_package %{nil}

Name:           dragomand
Version:        @VERSION@
Release:        0
Summary:        Offline machine translation daemon for the desktop
# The binaries statically link the vendored Bergamot/Marian engine and the
# Rust dependencies; every license text is installed with the package.
License:        GPL-3.0-or-later AND MPL-2.0 AND MIT AND Apache-2.0 AND BSD-3-Clause AND BSD-2-Clause
URL:            https://dragomand.l10n-bg.dev
Source0:        %{name}-%{version}.tar.gz

# The int8 models run through intgemm (x86-64) or ruy (ARM64); no other
# architecture has a path for them.
ExclusiveArch:  x86_64 aarch64

BuildRequires:  cargo
BuildRequires:  rust >= 1.85
BuildRequires:  cmake >= 3.25
BuildRequires:  desktop-file-utils
BuildRequires:  gcc-c++
BuildRequires:  hicolor-icon-theme
BuildRequires:  pkgconfig
BuildRequires:  pkgconfig(libpcre2-8)
BuildRequires:  systemd-rpm-macros
%if 0%{?fedora}
BuildRequires:  ninja-build
# FlexiBLAS is Fedora's system BLAS; the engine finds it through the
# pkg-config fallback chain (cblas, openblas, flexiblas).
BuildRequires:  pkgconfig(flexiblas)
BuildRequires:  dbus-daemon
%else
BuildRequires:  ninja
BuildRequires:  pkgconfig(cblas)
BuildRequires:  pkgconfig(lapack)
BuildRequires:  dbus-1
%endif

%description
Dragomand is a per-user daemon that gives desktop applications fully
offline machine translation over D-Bus. It runs Mozilla's
Bergamot/Marian translation models (the ones behind Firefox
Translations) locally, downloads and verifies models on demand, keeps
recently used pairs warm within a memory budget, and exits when idle.
The package ships the D-Bus activated daemon, the dragomanctl command
line client, and the KRunner and GNOME Shell search services.

%prep
%setup -q
# Shipped as documentation, not as commands.
chmod -x integrations/*.sh

%build
%ifarch x86_64
# Distribution baseline; the int8 hot path dispatches by CPUID at run
# time, so these builds keep AVX2 and AVX-512 speed.
build_arch=x86-64-v2
%else
build_arch=armv8-a
%endif
cmake -S engine -B engine/build -G Ninja \
    -DCMAKE_BUILD_TYPE=Release \
    -DBUILD_ARCH=$build_arch
cmake --build engine/build %{?_smp_mflags}

export DRAGOMAN_ENGINE_LIB_DIR="$PWD/engine/build"
cargo build --release --locked --offline --workspace

./target/release/dragomanctl manpage > dragomanctl.1
./target/release/dragomanctl completions bash > dragomanctl.bash
./target/release/dragomanctl completions zsh > _dragomanctl
./target/release/dragomanctl completions fish > dragomanctl.fish

%check
export DRAGOMAN_ENGINE_LIB_DIR="$PWD/engine/build"
cargo test --release --locked --offline --workspace
desktop-file-validate %{buildroot}%{_datadir}/applications/dev.l10n_bg.dragomand.desktop

%install
# The daemon and the search services are D-Bus activated, not in PATH;
# the shipped unit and D-Bus service files reference this exact path.
install -Dsm755 target/release/dragomand \
    %{buildroot}%{_prefix}/lib/dragomand/dragomand
install -Dsm755 target/release/dragoman-krunner \
    %{buildroot}%{_prefix}/lib/dragomand/dragoman-krunner
install -Dsm755 target/release/dragoman-search-provider \
    %{buildroot}%{_prefix}/lib/dragomand/dragoman-search-provider
install -Dsm755 target/release/dragomanctl %{buildroot}%{_bindir}/dragomanctl

install -Dm644 data/systemd/dragomand.service \
    %{buildroot}%{_userunitdir}/dragomand.service
install -Dm644 data/systemd/dragomand-update.service \
    %{buildroot}%{_userunitdir}/dragomand-update.service
install -Dm644 data/systemd/dragomand-update.timer \
    %{buildroot}%{_userunitdir}/dragomand-update.timer

install -Dm644 data/dbus/dev.l10n_bg.dragomand.Translator1.service \
    %{buildroot}%{_datadir}/dbus-1/services/dev.l10n_bg.dragomand.Translator1.service
install -Dm644 data/dbus/dev.l10n_bg.dragomand.KRunner1.service \
    %{buildroot}%{_datadir}/dbus-1/services/dev.l10n_bg.dragomand.KRunner1.service
install -Dm644 data/dbus/dev.l10n_bg.dragomand.SearchProvider.service \
    %{buildroot}%{_datadir}/dbus-1/services/dev.l10n_bg.dragomand.SearchProvider.service
install -Dm644 data/dbus/dev.l10n_bg.dragomand.Translator1.xml \
    %{buildroot}%{_datadir}/dbus-1/interfaces/dev.l10n_bg.dragomand.Translator1.xml

install -Dm644 data/krunner/dragoman-runner.desktop \
    %{buildroot}%{_datadir}/krunner/dbusplugins/dragoman-runner.desktop
install -Dm644 data/gnome/dev.l10n_bg.dragomand.search-provider.ini \
    %{buildroot}%{_datadir}/gnome-shell/search-providers/dev.l10n_bg.dragomand.search-provider.ini
install -Dm644 data/gnome/dev.l10n_bg.dragomand.desktop \
    %{buildroot}%{_datadir}/applications/dev.l10n_bg.dragomand.desktop
install -Dm644 artwork/dragomand-icon.svg \
    %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/dev.l10n_bg.dragomand.svg

install -Dm644 dragomanctl.1 %{buildroot}%{_mandir}/man1/dragomanctl.1
install -Dm644 dragomanctl.bash \
    %{buildroot}%{_datadir}/bash-completion/completions/dragomanctl
install -Dm644 _dragomanctl %{buildroot}%{_datadir}/zsh/site-functions/_dragomanctl
install -Dm644 dragomanctl.fish \
    %{buildroot}%{_datadir}/fish/vendor_completions.d/dragomanctl.fish

# License texts: ours plus the statically linked engine's.
mkdir -p dist-licenses
cp LICENSE dist-licenses/
cp engine/vendor/LICENSES/* dist-licenses/

%files
%license dist-licenses/*
%doc README.md data/config.toml.example integrations
%dir %{_prefix}/lib/dragomand
%{_prefix}/lib/dragomand/dragomand
%{_prefix}/lib/dragomand/dragoman-krunner
%{_prefix}/lib/dragomand/dragoman-search-provider
%{_bindir}/dragomanctl
%{_userunitdir}/dragomand.service
%{_userunitdir}/dragomand-update.service
%{_userunitdir}/dragomand-update.timer
%{_datadir}/dbus-1/services/dev.l10n_bg.dragomand.Translator1.service
%{_datadir}/dbus-1/services/dev.l10n_bg.dragomand.KRunner1.service
%{_datadir}/dbus-1/services/dev.l10n_bg.dragomand.SearchProvider.service
%dir %{_datadir}/dbus-1/interfaces
%{_datadir}/dbus-1/interfaces/dev.l10n_bg.dragomand.Translator1.xml
%dir %{_datadir}/krunner
%dir %{_datadir}/krunner/dbusplugins
%{_datadir}/krunner/dbusplugins/dragoman-runner.desktop
%dir %{_datadir}/gnome-shell
%dir %{_datadir}/gnome-shell/search-providers
%{_datadir}/gnome-shell/search-providers/dev.l10n_bg.dragomand.search-provider.ini
%{_datadir}/applications/dev.l10n_bg.dragomand.desktop
%{_datadir}/icons/hicolor/scalable/apps/dev.l10n_bg.dragomand.svg
%{_mandir}/man1/dragomanctl.1*
%dir %{_datadir}/bash-completion
%dir %{_datadir}/bash-completion/completions
%{_datadir}/bash-completion/completions/dragomanctl
%dir %{_datadir}/zsh
%dir %{_datadir}/zsh/site-functions
%{_datadir}/zsh/site-functions/_dragomanctl
%dir %{_datadir}/fish
%dir %{_datadir}/fish/vendor_completions.d
%{_datadir}/fish/vendor_completions.d/dragomanctl.fish

%changelog
* @RPM_DATE@ Blagovest Petrov <blagovest@petrovs.info> - @VERSION@
- First commit
