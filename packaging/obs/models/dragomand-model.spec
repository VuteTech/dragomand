#
# spec file for a dragomand model package (openSUSE and Fedora targets on OBS)
#
# SPDX-License-Identifier: GPL-3.0-or-later
#
# One package per language: every direction between @LANGUAGE@ and English
# that Mozilla publishes. packaging/obs/models/prepare.sh stamps the
# placeholders. The source tarball already holds a verified system store
# written by `dragomanctl store install --root`, because build hosts have
# no network; this recipe only copies it into place.

Name:           @NAME@
Version:        @VERSION@
Release:        0
Summary:        @LANGUAGE@ translation models for dragomand
License:        MPL-2.0
URL:            https://dragomand.l10n-bg.dev
Source0:        %{name}-%{version}.tar.gz
BuildArch:      noarch
Requires:       dragomand

%description
Offline machine translation models between @LANGUAGE@ and English
(@PAIRS@), from Mozilla's Firefox Translations. They install into
dragomand's read-only system store, so translating needs no download.

%prep
%setup -q

%build

%install
install -d %{buildroot}%{_datadir}/dragomand
cp -r models %{buildroot}%{_datadir}/dragomand/

%files
%license LICENSE
%doc README
%dir %{_datadir}/dragomand
%dir %{_datadir}/dragomand/models
%dir %{_datadir}/dragomand/models/mozilla-remote-settings
%{_datadir}/dragomand/models/mozilla-remote-settings/*

%changelog
* @RPM_DATE@ Blagovest Petrov <blagovest@petrovs.info> - @VERSION@
- First commit
