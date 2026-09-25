---
title: Инсталиране
---

# Инсталиране

## От пакет за дистрибуцията

Всяко издание се изгражда в
[openSUSE Build Service](https://build.opensuse.org/package/show/home:blago:dragomand/dragomand)
за дистрибуциите по-долу. Добавете хранилището веднъж; след това новите
издания пристигат с обичайните обновления на системата. Всяко хранилище
е подписано и командите внасят ключа му.

Пакетите са за x86-64 на всяка изброена дистрибуция; за Fedora 44 и
openSUSE Leap 16.0 има и пакети за 64-битов ARM.

=== "openSUSE Tumbleweed"

    ```sh
    sudo zypper addrepo https://download.opensuse.org/repositories/home:/blago:/dragomand/openSUSE_Tumbleweed/home:blago:dragomand.repo
    sudo zypper refresh
    sudo zypper install dragomand
    sudo zypper install dragomand-model-bg   # по избор: модели за български, вижте по-долу
    ```

    `zypper refresh` пита дали да се довери на ключа на хранилището;
    отговорете с `a`, за да му се доверява винаги.

=== "openSUSE Slowroll"

    ```sh
    sudo zypper addrepo https://download.opensuse.org/repositories/home:/blago:/dragomand/openSUSE_Slowroll/home:blago:dragomand.repo
    sudo zypper refresh
    sudo zypper install dragomand
    sudo zypper install dragomand-model-bg   # по избор: модели за български, вижте по-долу
    ```

    `zypper refresh` пита дали да се довери на ключа на хранилището;
    отговорете с `a`, за да му се доверява винаги.

=== "openSUSE Leap 16.0"

    ```sh
    sudo zypper addrepo https://download.opensuse.org/repositories/home:/blago:/dragomand/16.0/home:blago:dragomand.repo
    sudo zypper refresh
    sudo zypper install dragomand
    sudo zypper install dragomand-model-bg   # по избор: модели за български, вижте по-долу
    ```

    `zypper refresh` пита дали да се довери на ключа на хранилището;
    отговорете с `a`, за да му се доверява винаги.

=== "Fedora 44"

    ```sh
    sudo dnf config-manager addrepo --from-repofile=https://download.opensuse.org/repositories/home:/blago:/dragomand/Fedora_44/home:blago:dragomand.repo
    sudo dnf install dragomand
    sudo dnf install dragomand-model-bg      # по избор: модели за български, вижте по-долу
    ```

    При първа употреба `dnf` показва отпечатъка на ключа на хранилището
    и иска потвърждение.

=== "Debian"

    За Debian 13 (trixie):

    Ако `curl` липсва, първо го инсталирайте със `sudo apt install curl`.

    ```sh
    curl -fsSL https://download.opensuse.org/repositories/home:/blago:/dragomand/Debian_13/Release.key \
      | sudo tee /etc/apt/keyrings/dragomand.asc > /dev/null
    echo 'deb [signed-by=/etc/apt/keyrings/dragomand.asc] https://download.opensuse.org/repositories/home:/blago:/dragomand/Debian_13/ /' \
      | sudo tee /etc/apt/sources.list.d/dragomand.list
    sudo apt update
    sudo apt install dragomand
    sudo apt install dragomand-model-bg      # по избор: модели за български, вижте по-долу
    ```

    За Debian testing или unstable заменете `Debian_13` и в двата адреса
    с `Debian_Testing` или `Debian_Unstable`.

=== "Ubuntu 26.04"

    Ако `curl` липсва, първо го инсталирайте със `sudo apt install curl`.

    ```sh
    curl -fsSL https://download.opensuse.org/repositories/home:/blago:/dragomand/xUbuntu_26.04/Release.key \
      | sudo tee /etc/apt/keyrings/dragomand.asc > /dev/null
    echo 'deb [signed-by=/etc/apt/keyrings/dragomand.asc] https://download.opensuse.org/repositories/home:/blago:/dragomand/xUbuntu_26.04/ /' \
      | sudo tee /etc/apt/sources.list.d/dragomand.list
    sudo apt update
    sudo apt install dragomand
    sudo apt install dragomand-model-bg      # по избор: модели за български, вижте по-долу
    ```

=== "Arch Linux"

    Внесете и подпишете локално ключа на хранилището, добавете
    хранилището в `/etc/pacman.conf` и инсталирайте:

    ```sh
    key=$(curl -fsSL https://download.opensuse.org/repositories/home:/blago:/dragomand/Arch/x86_64/home_blago_dragomand_Arch.key)
    fingerprint=$(gpg --quiet --with-colons --import-options show-only --import --fingerprint <<< "$key" \
      | awk -F: '$1 == "fpr" { print $10; exit }')
    sudo pacman-key --add - <<< "$key"
    sudo pacman-key --lsign-key "$fingerprint"

    printf '\n[home_blago_dragomand_Arch]\nServer = https://download.opensuse.org/repositories/home:/blago:/dragomand/Arch/$arch\n' \
      | sudo tee -a /etc/pacman.conf
    sudo pacman -Syu dragomand
    sudo pacman -S dragomand-model-bg        # по избор: модели за български, вижте по-долу
    ```

    `$arch` в реда `Server` се пише буквално: pacman го попълва сам.

### Езикови модели

Пакетът `dragomand` не съдържа модели: по подразбиране всяка езикова
двойка се изтегля от Mozilla при първата ѝ употреба, в домашната ви
папка. За компютри без мрежа или за да избегнете първото изтегляне,
инсталирайте пакети с модели. Има по един за всеки език, с име
`dragomand-model-` плюс кода на езика, съдържащ всички посоки между
този език и английския: `dragomand-model-bg`, `dragomand-model-de`,
`dragomand-model-zh-hans` и т.н. Преводът между два неанглийски езика
минава през английския, когато и двата са инсталирани. Пакетираните
модели са в системното хранилище само за четене
`/usr/share/dragomand/models` и се обновяват заедно със системата.

Пакетът инсталира активирания през D-Bus демон, конзолния клиент
`dragomanctl` със завършване за обвивките и страница на ръководството,
потребителските единици на systemd и регистрациите за KRunner и
GNOME Shell.

## Първа употреба

```sh
dragomanctl translate -f bg -t en "Добро утро"
```

Първото извикване за дадена езикова двойка изтегля модела ѝ от Mozilla
в `~/.local/share/dragomand/models`, проверява SHA-256 на всеки файл,
зарежда го и превежда. Всичко след това работи офлайн.

Полезни команди:

```sh
dragomanctl pairs          # инсталирани и налични езикови двойки
dragomanctl install de-en  # изрично инсталиране на двойка
dragomanctl status         # състояние на демона, заредени модели, памет
dragomanctl update         # изрична проверка за обновления (мрежа)
dragomanctl remove de-en   # премахване на двойка от потребителското хранилище
```

## От изходен код

Изходният код е в
[github.com/VuteTech/dragomand](https://github.com/VuteTech/dragomand).
Два етапа: двигателят на C++, после работното пространство на Rust.

```sh
git clone https://github.com/VuteTech/dragomand.git
cd dragomand
cmake -S engine -B engine/build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build engine/build
DRAGOMAN_ENGINE_LIB_DIR=$PWD/engine/build cargo build --release --workspace
```

Необходими са CMake, Ninja, компилатор на C++17, обикновена реализация
на CBLAS (препоръчва се OpenBLAS; Intel MKL нарочно не се използва) и
Rust 1.85 или по-нов. Бележките за пакетиращите са в `docs/packaging.md`
в хранилището.

Обикновеното `cargo build` отчита версия за разработка `0.0.0-dev`;
версиите на изданията идват от етикета в git и се вписват в архива на
изданието. За да изградите пакета за Arch от копие на хранилището вместо
от хранилището за пакети, вижте коментара в началото на
`packaging/obs/PKGBUILD`.

## Поведение офлайн

- Моделите се инсталират в потребителското ви хранилище; пакети с
  модели от дистрибуцията могат вместо това да запълнят системно
  хранилище само за четене.
- При инсталиран използваем модел демонът изобщо не докосва мрежата.
  Проверки за обновления стават само чрез `dragomanctl update` или
  незадължителния седмичен таймер:
  `systemctl --user enable dragomand-update.timer`.
