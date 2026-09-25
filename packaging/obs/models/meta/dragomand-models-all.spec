#
# spec file for dragomand-models-all, the meta package that installs every
# dragomand model package (openSUSE and Fedora targets on OBS)
#
# SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
# SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# packaging/obs/models/prepare-meta.py stamps the placeholders from the
# same plan as the language packages, with one Requires: line per
# language package.

Name:           dragomand-models-all
Version:        @VERSION@
Release:        0
Summary:        All translation models for dragomand
License:        GPL-3.0-or-later
URL:            https://dragomand.l10n-bg.dev
Source0:        %{name}-%{version}.tar.gz
BuildArch:      noarch
Requires:       dragomand
@REQUIRES@

%description
Installs every dragomand model package: offline machine translation
between English and each of @COUNT@ languages, from Mozilla's Firefox
Translations. The models take roughly @SIZE@ of disk space.

%prep
%setup -q

%build

%install

%files
%doc README

%changelog
* @RPM_DATE@ Blagovest Petrov <blagovest@petrovs.info> - @VERSION@
- First commit
