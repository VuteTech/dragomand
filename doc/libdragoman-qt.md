# Qt client library

libdragoman-qt is the Qt 6 client library for Dragomand. It wraps the
[D-Bus API](dbus-api.md) in asynchronous jobs, so a Qt application can
translate text and documents, detect languages and manage language pairs
without handling request objects and signals itself.
[Krakoman](krakoman.md), the [System Settings module](system-settings.md),
the [Plasma widget](plasma-widget.md) and the
[KTextEditor plugin](ktexteditor.md) are built on it.

It is developed as its own project,
[libdragoman-qt](https://github.com/VuteTech/libdragoman-qt), and
packaged in the same repository as Dragomand.

## Install

Applications that use the library pull it in by themselves. To build
your own, install the development package from the Dragomand repository
(see [Install](install.md)):

| Distribution | Package |
|---|---|
| openSUSE, Fedora | `libdragoman-qt-devel` |
| Debian and Ubuntu | `libdragoman-qt-dev` |
| Arch Linux | `libdragoman-qt` |

The library needs Qt 6.8 (Core, DBus) and KDE Frameworks I18n 6.13 or
newer. Its API is young: until 1.0 the SO version follows the minor
version, so any minor release may change it.

## Use it

CMake finds it as the `DragomanQt` package:

```cmake
find_package(DragomanQt REQUIRED)
target_link_libraries(myapp PRIVATE DragomanQt::DragomanQt)
```

Every call returns a `Dragoman::Job`, which finishes exactly once, with a
result, an error or a cancellation:

```cpp
#include <dragomanclient.h>

auto *client = new Dragoman::Client(this);
Dragoman::Job *job = client->translate(u"bg"_s, u"en"_s, {u"Добро утро"_s});
connect(job, &Dragoman::Job::finished, this, [](const Dragoman::Reply &reply) {
    if (reply.ok()) {
        qDebug() << reply.translations();
    }
});
```

- A job never finishes inside the call that created it, and can be
  cancelled. A missing language pair is installed on demand, with
  progress, unless the caller opts out.
- `translate()` can return sentence alignment (`Reply::sentences()`);
  `translateDocument()` sends whole documents through a memory file
  instead of the bus.
- `detectLanguage()`, `listPairs()` with Mozilla's quality scores,
  `installPair()`, `removePair()`, `checkForUpdates()`, `status()`, and
  `config()` and `setConfig()` with the `configChanged()` signal cover
  the rest of the daemon's API.
- `Dragoman::languageName()` names a language in the user's language.

## Testing against a fake daemon

The development package installs `DragomanQt/testing/fakedaemon.h`. It
starts a private `dbus-daemon` with a scriptable fake of the daemon, so
an application's unit tests never touch the session bus or need real
models. The library's own tests and those of every application above use
it.

## Other toolkits

There is no GTK library yet. Any language with D-Bus bindings can use
the [D-Bus API](dbus-api.md) directly, and `dragomanctl` covers scripts
(see [Command line](cli.md)).
