---
title: Инсталиране
---

# Инсталиране

Dragomand е млад проект: има пакет за Арч Линукс, а изходният код се
компилира на всяка съвременна дистрибуция. До първото публично издание
изходният код идва от копие на хранилището.

## Арч Линукс

Изградете пакета с PKGBUILD файла от хранилището:

```sh
scripts/make-release-tarball.sh packaging/arch/
cd packaging/arch
makepkg -si
```

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

## Поведение офлайн

- Моделите се инсталират в потребителското ви хранилище; пакети с
  модели от дистрибуцията могат вместо това да запълнят системно
  хранилище само за четене.
- При инсталиран използваем модел демонът изобщо не докосва мрежата.
  Проверки за обновления стават само чрез `dragomanctl update` или
  незадължителния седмичен таймер:
  `systemctl --user enable dragomand-update.timer`.
