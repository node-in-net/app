# fakelzo

A two-function stand-in for **liblzo2**, so that GPL library stays out of the release bundles.

## Why

node.in.net is MIT OR Apache-2.0, but the desktop bundles would otherwise be distributed
under GPL-2.0-or-later because of a single library nobody asked for. It arrives at the end
of a chain of optional dependencies:

```
libgtk-4  ->  libcairo-script-interpreter  ->  liblzo2   (GPL-2.0-or-later)
```

On the Windows runtime in `artifacts/gtk4-win32-x64/` that chain is visible in the import
tables: `libgtk-4-1.dll` imports `libcairo-script-interpreter-2.dll`, which imports exactly
two symbols from `liblzo2-2.dll` — `lzo2a_decompress` and `lzo2a_999_compress`.

The interpreter uses them only when serialising drawing operations to a cairo script, which
is GTK debugging machinery. The application never drives it.

That last sentence was **measured, not assumed.** The same chain exists on Linux
(`libgtk-4.so → libcairo-script-interpreter.so.2 → liblzo2.so.2`), so the measurement was
made there: an instrumented stand-in exporting the two symbols and logging every call was
placed ahead of the real library with `LD_PRELOAD`, and the headless suite — which launches
the real application and drives it over REST — was run against it.

    LOUDLZO_LOG=/tmp/lzo.log LD_PRELOAD=/path/to/loudlzo.so ./run-tests.sh headless

Across all 65 scenarios — setup wizard, sign-in, devices, files, terminal, registry,
network, system info, remote desktop, a peer session and relaunch — the stand-in was loaded
into every application process and recorded **zero calls**. The suite scored 65/65, exactly
as it does without the stand-in, so nothing was broken or skipped along the way.

If some future GTK did reach that path, the failure mode is a degraded debug feature, not a
crash.

The loader still needs the library to exist, because the interpreter links it. So instead of
shipping someone else's GPL code for a code path that never runs, we ship two functions.

## What it does

Exports the two symbols with the LZO signatures and reports failure (`LZO_E_ERROR`, output
length zero). It does not compress anything, and it contains no LZO code — the algorithm was
not reimplemented and no LZO source was consulted. Function signatures are not copyrightable;
this is the same approach that lets libedit stand in for readline.

## Trade-off

Cairo-script serialisation does not work in a build that uses this. Nothing the application
offers depends on it. If that ever changes, there are two honest fixes, in order of
preference:

1. Build GTK with the script interpreter disabled — upstream supports it, the dependency is
   declared `required: false`, and then neither library ships at all.
2. Implement these two entry points over zlib. The interpreter both writes and reads the data
   itself, so any algorithm round-trips consistently; only the on-disk format would differ
   from real cairo scripts.

## Building

**Windows** — called by `builder/build.sh exe` before NSIS runs:

```sh
./src/fakelzo/build-windows.sh          # writes bin/distr/fakelzo/liblzo2-2.dll
```

Nothing is overwritten in place. `src/gtk-app/setup.nsi` excludes `liblzo2-2.dll` from the
recursive `File /r` over `artifacts/gtk4-win32-x64` and packs ours instead, so the GPL
library never enters the installer archive — which is what matters, since packing it would
be distribution regardless of which copy wins on disk. The real DLL is also absent from
`artifacts/` in this repository; the exclusion stays as a guard for anyone who repopulates
that directory from MSYS2.

**macOS** — called by `builder/build.sh dmg` after `dylibbundler` has populated
`Contents/Libs`, and before code signing:

```sh
./src/fakelzo/build-macos.sh <path-to-bundle>/Contents/Libs/liblzo2.2.dylib
```

It reads the install_name and version fields off the real library before overwriting it, so
the replacement keeps whatever Homebrew's current lzo advertises, then verifies both symbols
are present.

**Linux** — nothing to do. The `deb`, `rpm` and `zst` packages take GTK from the system and
bundle no libraries at all.

## License

MIT, like the rest of the project.
