# Third-party licenses

node.in.net itself is licensed under **MIT OR Apache-2.0** (see
[LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE)). This file
lists everything else that ships with it.

The Rust section below is generated — do not edit it by hand:

```sh
cargo about generate --config about.toml about.hbs -o THIRD-PARTY-LICENSES.md
```

## Bundled native libraries

These are native libraries the application links, directly or through the GTK
stack. They are not Rust crates, so they are not covered by the generated section
— this list is maintained by hand.

| Library | License | Where | Linking |
| --- | --- | --- | --- |
| GTK 4, libadwaita, GLib, Pango, gdk-pixbuf, librsvg | LGPL-2.1-or-later | desktop, all platforms | dynamic |
| cairo | LGPL-2.1-only OR MPL-1.1 (used under LGPL-2.1) | desktop, all platforms | dynamic |
| GStreamer | LGPL-2.1-or-later | Linux: screen capture, taken from the system. Windows: bundled with GTK, not driven by this application | dynamic |
| libtiff | libtiff license (BSD-like) | Windows bundle | dynamic |
| libidn2 | LGPL-3.0-or-later OR GPL-2.0-or-later (used under LGPL-3.0) | Windows bundle | dynamic |
| PipeWire (screen capture) | MIT | Linux | dynamic |
| windows-capture / ScreenCaptureKit | screen capture on Windows / macOS | Windows, macOS | dynamic |
| OpenH264 (remote desktop codec) | BSD-2-Clause | all platforms | static, built from vendored source |
| OpenSSL 3.x | Apache-2.0 | where system TLS is used | dynamic |
| libdbus | AFL-2.1 OR GPL-2.0-or-later (used under AFL-2.1) | Linux | dynamic |
| MinHook | BSD-2-Clause | Windows | static |
| fakelzo (our stand-in for lzo2) | MIT | Windows, macOS | dynamic |
| fakejbig (our stand-in for libjbig) | MIT | Windows | dynamic |

**LGPL components** are linked dynamically and shipped as separate shared
libraries, so they may be replaced by the user. On Linux none of them is shipped:
the `deb`, `rpm` and `zst` packages declare `libgtk-4-1` / `libadwaita-1-0` (and
their equivalents) as dependencies, and everything those pull in — GStreamer,
PipeWire, the imaging stack — comes from the distribution too. The Windows
installer bundles the MSYS2 GTK 4 runtime, and the macOS bundle carries the copies
`dylibbundler` collects.

**The remote desktop codec is OpenH264**, Cisco's BSD-2-Clause implementation,
used by `client-core` behind the `feature-rdesk` feature
([`submodules/p2p-common/client-core/src/rtc/mod.rs`](submodules/p2p-common/client-core/src/rtc/mod.rs)).
Nothing here uses x264 or x265, so no GPL encoder is involved. The pinned
`openh264-sys2` vendors the upstream C++ sources and compiles them, so the shipped
codec is BSD-2-Clause OpenH264 built from source and linked statically — no Cisco
binary is downloaded, and none of Cisco's separate binary-distribution terms apply.

**`liblzo2` no longer ships.** It is GPL-2.0-or-later (Markus F.X.J. Oberhumer) and was never
something the application asked for — GTK links it through the cairo script interpreter
(`libgtk-4-1 → libcairo-script-interpreter-2 → liblzo2-2`), a debugging facility nothing
here drives.

It is replaced on both desktop platforms by [`src/fakelzo/`](src/fakelzo/), our own
MIT-licensed stand-in exporting the only two symbols the interpreter imports
(`lzo2a_decompress`, `lzo2a_999_compress`), which report failure instead of compressing.
No LZO code was used or consulted; only the two function signatures are shared.

On Windows the real DLL is excluded from the installer archive rather than overwritten after
install ([`src/gtk-app/setup.nsi`](src/gtk-app/setup.nsi)), because packing it would be
distribution regardless of which copy wins on disk; on macOS the replacement happens in
`builder/build.sh` after `dylibbundler` and before signing. Linux links nothing of the sort —
GTK comes from the distribution.

That the code path is unreachable was **measured, not assumed**: the same chain exists on
Linux, so an instrumented stand-in was preloaded ahead of the real library and the headless
suite — which drives the real application over REST — was run against it. Across all 65
scenarios the stand-in was loaded into every application process and logged zero calls, with
the suite scoring 65/65 exactly as it does without it.

The cost is that cairo-script serialisation does not work in these builds. Nothing the
application offers uses it.

**`libjbig` no longer ships either.** It is GPL-2.0-or-later — the DLL identifies itself as
"JBIG-KIT 2.1 — (c) 1995-2014 Markus Kuhn — Licence: GPL" — and it arrives through
`libgtk-4-1 / libgdk_pixbuf-2.0-0 → libtiff-6 → libjbig-0`, hard load-time imports every step
of the way, so neither it nor libtiff could simply be dropped. libtiff imports ten symbols
from it for one thing only: TIFF files whose Compression tag is JBIG. This project never asks
for TIFF at all; the viewer and the file manager hand bytes to gdk-pixbuf and let it sniff the
format.

It is replaced on Windows by [`src/fakejbig/`](src/fakejbig/), the same approach as fakelzo:
ten MIT-licensed entry points with the signatures from the public `jbig.h`, reporting failure
instead of decoding. No JBIG-KIT code was used or consulted.

Here too the conclusion was **measured**: the chain exists on Linux as well, so an instrumented
stand-in was preloaded and eight TIFFs — raw, LZW, Deflate, PackBits, JPEG and the three CCITT
bi-level variants — were loaded through gdk-pixbuf. All decoded exactly as in the control run,
and no `jbg_*` function was called once. The cost is that a genuinely JBIG-compressed TIFF
fails to open; effectively nothing produces those.

**No GPL code ships in any bundle.** One string suggests otherwise and is worth explaining
once, because every licence scanner finds it.

`libgdk_pixbuf-2.0-0.dll` reports `"GPL"` for one of its fifteen image formats — the macOS
icon format — and `"LGPL"` for the other fourteen. **The licence is LGPL**; it is stated in
the header of the source file itself, which has said so unchanged since the file was added
in 2007:

> [`gdk-pixbuf/io-icns.c`](https://gitlab.gnome.org/GNOME/gdk-pixbuf/-/blob/2.44.4/gdk-pixbuf/io-icns.c#L6-17)
> — *"This library is free software; you can redistribute it and/or modify it under the terms
> of the GNU **Lesser** General Public License … version 2 of the License, or (at your option)
> any later version."*

The `"GPL"` on [line 521](https://gitlab.gnome.org/GNOME/gdk-pixbuf/-/blob/2.44.4/gdk-pixbuf/io-icns.c#L521)
of that same file fills a display field returned by `gdk_pixbuf_format_get_license()`. It
contradicts the file's own header, gdk-pixbuf's `COPYING` (LGPL-2.1) and Debian's copyright
file, which puts GPL-2+ only on `tests/*` and `thumbnailer/*`. It is an upstream typo, not a
licence.

**This source tree stays MIT OR Apache-2.0**, and using the application —
including inside a company — carries no obligation under any of these licenses.
The obligations above attach to redistribution of the *binary packages* only.

## Icons

The interface uses SVG icons from [Icons8](https://icons8.com), under a license the
author purchased. That license covers the releases published by this project — it is
not granted to anyone else by the source being public.

**If you fork or redistribute node.in.net, the purchase does not travel with the
code.** Either credit Icons8 as their license requires, or replace the artwork with
your own. The author credits them here in any case.

The icons ship in every client, so this applies to all of them: the GTK assets
under [`src/gtk-app/assets/`](src/gtk-app/assets/) and the other `src/*-ui/assets/`
directories, the Android set under
[`src/android-app/app/src/main/assets/icons/`](src/android-app/app/src/main/assets/icons/),
and the copies served by the browser client from the `webapp` repository.

The application logo (`src/gtk-app/assets/logo.svg`, `icon-128.png`, `icon-512.png`,
`win32-icon.ico`, the Android launcher icons under `res/mipmap-*`) and the product
name are the project's own and are not covered by the above.

## Rust dependencies

- **Apache License 2.0** (`Apache-2.0`) — 481 crates
- **MIT License** (`MIT`) — 166 crates
- **ISC License** (`ISC`) — 29 crates
- **BSD 2-Clause "Simplified" License** (`BSD-2-Clause`) — 24 crates
- **Unicode License v3** (`Unicode-3.0`) — 19 crates
- **BSD 3-Clause "New" or "Revised" License** (`BSD-3-Clause`) — 14 crates
- **SSLeay License - standalone** (`SSLeay-standalone`) — 4 crates
- **zlib License** (`Zlib`) — 4 crates
- **Mozilla Public License 2.0** (`MPL-2.0`) — 3 crates
- **OpenSSL License** (`OpenSSL`) — 1 crate
- **OpenSSL License - standalone** (`OpenSSL-standalone`) — 1 crate

### Crates

| Crate | Version | License |
| --- | --- | --- |
| actix | 0.13.5 | MIT OR Apache-2.0 |
| actix-codec | 0.5.2 | MIT OR Apache-2.0 |
| actix-http | 3.13.3 | MIT OR Apache-2.0 |
| actix-macros | 0.2.4 | MIT OR Apache-2.0 |
| actix-router | 0.5.4 | MIT OR Apache-2.0 |
| actix-rt | 2.11.0 | MIT OR Apache-2.0 |
| actix-server | 2.7.0 | MIT OR Apache-2.0 |
| actix-service | 2.0.3 | MIT OR Apache-2.0 |
| actix-utils | 3.0.1 | MIT OR Apache-2.0 |
| actix-web | 4.14.1 | MIT OR Apache-2.0 |
| actix-web-actors | 4.3.1+deprecated | MIT OR Apache-2.0 |
| actix-web-codegen | 4.3.0 | MIT OR Apache-2.0 |
| actix-web-httpauth | 0.8.2 | MIT OR Apache-2.0 |
| actix_derive | 0.6.2 | MIT OR Apache-2.0 |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 |
| aead | 0.3.2 | MIT OR Apache-2.0 |
| aead | 0.4.3 | MIT OR Apache-2.0 |
| aead | 0.5.2 | MIT OR Apache-2.0 |
| aes | 0.6.0 | MIT OR Apache-2.0 |
| aes | 0.7.5 | MIT OR Apache-2.0 |
| aes | 0.8.4 | MIT OR Apache-2.0 |
| aes-gcm | 0.9.4 | Apache-2.0 OR MIT |
| aes-gcm | 0.10.3 | Apache-2.0 OR MIT |
| aes-soft | 0.6.4 | MIT OR Apache-2.0 |
| ahash | 0.8.12 | MIT OR Apache-2.0 |
| aho-corasick | 1.1.5 | Unlicense OR MIT |
| alloc-no-stdlib | 2.0.4 | BSD-3-Clause |
| alloc-stdlib | 0.2.4 | BSD-3-Clause |
| android-node | 0.5.513 | MIT OR Apache-2.0 |
| android_system_properties | 0.1.6 | MIT OR Apache-2.0 |
| anstream | 1.0.0 | MIT OR Apache-2.0 |
| anstyle | 1.0.14 | MIT OR Apache-2.0 |
| anstyle-parse | 1.0.0 | MIT OR Apache-2.0 |
| anstyle-query | 1.1.5 | MIT OR Apache-2.0 |
| anstyle-wincon | 3.0.11 | MIT OR Apache-2.0 |
| anyhow | 1.0.104 | MIT OR Apache-2.0 |
| app-core | 0.5.513 | MIT OR Apache-2.0 |
| app-headless | 0.5.513 | MIT OR Apache-2.0 |
| app-net | 0.5.513 | MIT OR Apache-2.0 |
| arc-swap | 1.9.2 | MIT OR Apache-2.0 |
| argon2 | 0.5.3 | MIT OR Apache-2.0 |
| arrayref | 0.3.9 | BSD-2-Clause |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 |
| ashpd | 0.9.3 | MIT |
| asn1-rs | 0.3.1 | MIT OR Apache-2.0 |
| asn1-rs | 0.5.2 | MIT OR Apache-2.0 |
| asn1-rs-derive | 0.1.0 | MIT OR Apache-2.0 |
| asn1-rs-derive | 0.4.0 | MIT OR Apache-2.0 |
| asn1-rs-impl | 0.1.0 | MIT OR Apache-2.0 |
| async-broadcast | 0.7.2 | MIT OR Apache-2.0 |
| async-channel | 2.5.0 | Apache-2.0 OR MIT |
| async-executor | 1.14.0 | Apache-2.0 OR MIT |
| async-fs | 2.2.0 | Apache-2.0 OR MIT |
| async-io | 2.6.0 | Apache-2.0 OR MIT |
| async-lock | 3.4.2 | Apache-2.0 OR MIT |
| async-net | 2.0.0 | Apache-2.0 OR MIT |
| async-process | 2.5.0 | Apache-2.0 OR MIT |
| async-recursion | 1.1.1 | MIT OR Apache-2.0 |
| async-signal | 0.2.14 | Apache-2.0 OR MIT |
| async-task | 4.7.1 | Apache-2.0 OR MIT |
| async-trait | 0.1.92 | MIT OR Apache-2.0 |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT |
| atomic_refcell | 0.1.14 | Apache-2.0 OR MIT |
| base16ct | 0.1.1 | Apache-2.0 OR MIT |
| base64 | 0.13.1 | MIT OR Apache-2.0 |
| base64 | 0.21.7 | MIT OR Apache-2.0 |
| base64 | 0.22.1 | MIT OR Apache-2.0 |
| base64ct | 1.8.3 | Apache-2.0 OR MIT |
| bincode | 1.3.3 | MIT |
| bit_field | 0.10.3 | Apache-2.0 OR MIT |
| bitflags | 1.3.2 | MIT OR Apache-2.0 |
| bitflags | 2.13.1 | MIT OR Apache-2.0 |
| bitvec | 1.1.1 | MIT |
| blake2 | 0.10.6 | MIT OR Apache-2.0 |
| block-buffer | 0.9.0 | MIT OR Apache-2.0 |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 |
| block-buffer | 0.12.1 | MIT OR Apache-2.0 |
| block-modes | 0.7.0 | MIT OR Apache-2.0 |
| block-padding | 0.2.1 | MIT OR Apache-2.0 |
| block2 | 0.6.2 | MIT |
| blocking | 1.6.2 | Apache-2.0 OR MIT |
| brotli | 8.0.4 | BSD-3-Clause AND MIT |
| brotli-decompressor | 5.0.3 | BSD-3-Clause OR MIT |
| bson | 3.1.0 | MIT |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT |
| byteorder | 1.5.0 | Unlicense OR MIT |
| byteorder-lite | 0.1.0 | Unlicense OR MIT |
| bytes | 1.12.1 | MIT |
| bytestring | 1.5.1 | MIT OR Apache-2.0 |
| cairo-rs | 0.20.12 | MIT |
| cairo-sys-rs | 0.20.10 | MIT |
| ccm | 0.3.0 | Apache-2.0 OR MIT |
| cesu8 | 1.1.0 | Apache-2.0 OR MIT |
| cfg-if | 1.0.4 | MIT OR Apache-2.0 |
| chacha20 | 0.9.1 | Apache-2.0 OR MIT |
| chacha20 | 0.10.1 | MIT OR Apache-2.0 |
| chacha20poly1305 | 0.10.1 | Apache-2.0 OR MIT |
| chrono | 0.4.45 | MIT OR Apache-2.0 |
| cipher | 0.2.5 | MIT OR Apache-2.0 |
| cipher | 0.3.0 | MIT OR Apache-2.0 |
| cipher | 0.4.4 | MIT OR Apache-2.0 |
| clap | 4.6.6 | MIT OR Apache-2.0 |
| clap_builder | 4.6.6 | MIT OR Apache-2.0 |
| clap_derive | 4.6.4 | MIT OR Apache-2.0 |
| clap_lex | 1.1.0 | MIT OR Apache-2.0 |
| color_quant | 1.1.0 | MIT |
| colorchoice | 1.0.5 | MIT OR Apache-2.0 |
| colored | 2.2.0 | MPL-2.0 |
| combine | 4.6.7 | MIT |
| concurrent-queue | 2.5.0 | Apache-2.0 OR MIT |
| console-app | 0.5.513 | MIT OR Apache-2.0 |
| console_error_panic_hook | 0.1.7 | Apache-2.0 OR MIT |
| const-oid | 0.9.6 | Apache-2.0 OR MIT |
| const-oid | 0.10.2 | Apache-2.0 OR MIT |
| convert_case | 0.10.0 | MIT |
| cookie | 0.16.2 | MIT OR Apache-2.0 |
| core-foundation | 0.9.4 | MIT OR Apache-2.0 |
| core-foundation | 0.10.1 | MIT OR Apache-2.0 |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 |
| core-graphics | 0.22.3 | MIT  OR  Apache-2.0 |
| core-graphics | 0.23.2 | MIT OR Apache-2.0 |
| core-graphics-types | 0.1.3 | MIT OR Apache-2.0 |
| core_maths | 0.1.1 | MIT |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 |
| cpufeatures | 0.3.0 | MIT OR Apache-2.0 |
| crc | 3.4.0 | MIT OR Apache-2.0 |
| crc-catalog | 2.5.0 | MIT OR Apache-2.0 |
| crc32fast | 1.5.0 | MIT OR Apache-2.0 |
| crossbeam-channel | 0.5.16 | MIT OR Apache-2.0 |
| crossbeam-deque | 0.8.7 | MIT OR Apache-2.0 |
| crossbeam-epoch | 0.9.20 | MIT OR Apache-2.0 |
| crossbeam-utils | 0.8.22 | MIT OR Apache-2.0 |
| crossterm | 0.27.0 | MIT |
| crossterm_winapi | 0.9.1 | MIT |
| crypto-bigint | 0.4.9 | Apache-2.0 OR MIT |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 |
| crypto-common | 0.2.2 | MIT OR Apache-2.0 |
| crypto-mac | 0.11.1 | MIT OR Apache-2.0 |
| ctr | 0.8.0 | MIT OR Apache-2.0 |
| ctr | 0.9.2 | MIT OR Apache-2.0 |
| curve25519-dalek | 3.2.0 | BSD-3-Clause |
| curve25519-dalek | 4.1.3 | BSD-3-Clause |
| curve25519-dalek-derive | 0.1.1 | MIT OR Apache-2.0 |
| darling | 0.14.4 | MIT |
| darling_core | 0.14.4 | MIT |
| darling_macro | 0.14.4 | MIT |
| data-encoding | 2.11.1 | MIT |
| data-url | 0.3.2 | MIT OR Apache-2.0 |
| dav-server | 0.5.8 | Apache-2.0 |
| dbus | 0.9.12 | Apache-2.0 OR MIT |
| der | 0.6.1 | Apache-2.0 OR MIT |
| der-parser | 7.0.0 | MIT OR Apache-2.0 |
| der-parser | 8.2.0 | MIT OR Apache-2.0 |
| deranged | 0.5.8 | MIT OR Apache-2.0 |
| derive_builder | 0.11.2 | MIT OR Apache-2.0 |
| derive_builder_core | 0.11.2 | MIT OR Apache-2.0 |
| derive_builder_macro | 0.11.2 | MIT OR Apache-2.0 |
| derive_more | 2.1.1 | MIT |
| derive_more-impl | 2.1.1 | MIT |
| digest | 0.9.0 | MIT OR Apache-2.0 |
| digest | 0.10.7 | MIT OR Apache-2.0 |
| digest | 0.11.3 | MIT OR Apache-2.0 |
| dirs | 5.0.1 | MIT OR Apache-2.0 |
| dirs-sys | 0.4.1 | MIT OR Apache-2.0 |
| dispatch2 | 0.3.1 | Zlib OR Apache-2.0 OR MIT |
| display-info | 0.4.8 | Apache-2.0 |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 |
| dlib | 0.5.3 | MIT |
| downcast-rs | 1.2.1 | MIT OR Apache-2.0 |
| ecdsa | 0.14.8 | Apache-2.0 OR MIT |
| ed25519 | 2.2.3 | Apache-2.0 OR MIT |
| ed25519-dalek | 2.2.0 | BSD-3-Clause |
| either | 1.17.0 | MIT OR Apache-2.0 |
| elliptic-curve | 0.12.3 | Apache-2.0 OR MIT |
| encoding_rs | 0.8.35 | (Apache-2.0 OR MIT) AND BSD-3-Clause |
| endi | 1.1.1 | MIT |
| enumflags2 | 0.7.12 | MIT OR Apache-2.0 |
| enumflags2_derive | 0.7.12 | MIT OR Apache-2.0 |
| equivalent | 1.0.2 | Apache-2.0 OR MIT |
| errno | 0.3.14 | MIT OR Apache-2.0 |
| event-listener | 5.4.2 | Apache-2.0 OR MIT |
| event-listener-strategy | 0.5.4 | Apache-2.0 OR MIT |
| exr | 1.74.2 | BSD-3-Clause |
| fastrand | 2.5.0 | Apache-2.0 OR MIT |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 |
| ff | 0.12.1 | MIT OR Apache-2.0 |
| field-offset | 0.3.6 | MIT OR Apache-2.0 |
| filedescriptor | 0.8.3 | MIT |
| flate2 | 1.1.9 | MIT OR Apache-2.0 |
| float-cmp | 0.9.0 | MIT |
| flume | 0.11.1 | Apache-2.0 OR MIT |
| flume | 0.12.0 | Apache-2.0 OR MIT |
| fnv | 1.0.7 | Apache-2.0  OR  MIT |
| foldhash | 0.2.0 | Zlib |
| fontconfig-parser | 0.5.8 | MIT |
| fontdb | 0.23.0 | MIT |
| foreign-types | 0.3.2 | MIT OR Apache-2.0 |
| foreign-types | 0.5.0 | MIT OR Apache-2.0 |
| foreign-types-macros | 0.2.4 | MIT OR Apache-2.0 |
| foreign-types-shared | 0.1.1 | MIT OR Apache-2.0 |
| foreign-types-shared | 0.3.1 | MIT OR Apache-2.0 |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 |
| fragile | 2.1.0 | Apache-2.0 |
| funty | 2.0.0 | MIT |
| futures | 0.3.34 | MIT OR Apache-2.0 |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 |
| futures-core | 0.3.34 | MIT OR Apache-2.0 |
| futures-executor | 0.3.34 | MIT OR Apache-2.0 |
| futures-io | 0.3.34 | MIT OR Apache-2.0 |
| futures-lite | 2.6.1 | Apache-2.0 OR MIT |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 |
| futures-task | 0.3.34 | MIT OR Apache-2.0 |
| futures-util | 0.3.34 | MIT OR Apache-2.0 |
| fxhash | 0.2.1 | Apache-2.0 OR MIT |
| gdk-pixbuf | 0.20.10 | MIT |
| gdk-pixbuf-sys | 0.20.10 | MIT |
| gdk4 | 0.9.6 | MIT |
| gdk4-sys | 0.9.6 | MIT |
| generic-array | 0.14.7 | MIT |
| getrandom | 0.1.16 | MIT OR Apache-2.0 |
| getrandom | 0.2.17 | MIT OR Apache-2.0 |
| getrandom | 0.3.4 | MIT OR Apache-2.0 |
| getrandom | 0.4.3 | MIT OR Apache-2.0 |
| ghash | 0.4.4 | Apache-2.0 OR MIT |
| ghash | 0.5.1 | Apache-2.0 OR MIT |
| gif | 0.13.3 | MIT OR Apache-2.0 |
| gif | 0.14.2 | MIT OR Apache-2.0 |
| gio | 0.20.12 | MIT |
| gio-sys | 0.19.8 | MIT |
| gio-sys | 0.20.10 | MIT |
| glib | 0.19.9 | MIT |
| glib | 0.20.12 | MIT |
| glib-macros | 0.19.9 | MIT |
| glib-macros | 0.20.12 | MIT |
| glib-sys | 0.19.8 | MIT |
| glib-sys | 0.20.10 | MIT |
| gobject-sys | 0.19.8 | MIT |
| gobject-sys | 0.20.10 | MIT |
| graphene-rs | 0.20.10 | MIT |
| graphene-sys | 0.20.10 | MIT |
| group | 0.12.1 | MIT OR Apache-2.0 |
| gsk4 | 0.9.6 | MIT |
| gsk4-sys | 0.9.6 | MIT |
| gstreamer | 0.22.8 | MIT OR Apache-2.0 |
| gstreamer-app | 0.22.6 | MIT OR Apache-2.0 |
| gstreamer-app-sys | 0.22.6 | MIT |
| gstreamer-base | 0.22.6 | MIT OR Apache-2.0 |
| gstreamer-base-sys | 0.22.6 | MIT |
| gstreamer-sys | 0.22.6 | MIT |
| gtk4 | 0.9.7 | MIT |
| gtk4-macros | 0.9.5 | MIT |
| gtk4-sys | 0.9.6 | MIT |
| h2 | 0.3.27 | MIT |
| half | 2.7.1 | MIT OR Apache-2.0 |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 |
| headers | 0.3.9 | MIT |
| headers-core | 0.2.0 | MIT |
| heck | 0.5.0 | MIT OR Apache-2.0 |
| hex | 0.4.3 | MIT OR Apache-2.0 |
| hkdf | 0.12.4 | MIT OR Apache-2.0 |
| hmac | 0.11.0 | MIT OR Apache-2.0 |
| hmac | 0.12.1 | MIT OR Apache-2.0 |
| hostname | 0.3.1 | MIT |
| hostname | 0.4.2 | MIT |
| htmlescape | 0.3.1 | Apache-2.0  OR  MIT  OR  MPL-2.0 |
| http | 0.2.12 | MIT OR Apache-2.0 |
| http-body | 0.4.6 | MIT |
| httparse | 1.10.1 | MIT OR Apache-2.0 |
| httpdate | 1.0.3 | MIT OR Apache-2.0 |
| hybrid-array | 0.4.14 | MIT OR Apache-2.0 |
| hyper | 0.14.32 | MIT |
| hyper-rustls | 0.24.2 | Apache-2.0 OR ISC OR MIT |
| hyper-tls | 0.5.0 | MIT OR Apache-2.0 |
| iana-time-zone | 0.1.65 | MIT OR Apache-2.0 |
| icu_collections | 2.3.0 | Unicode-3.0 |
| icu_locale_core | 2.3.0 | Unicode-3.0 |
| icu_normalizer | 2.3.0 | Unicode-3.0 |
| icu_normalizer_data | 2.3.0 | Unicode-3.0 |
| icu_properties | 2.3.0 | Unicode-3.0 |
| icu_properties_data | 2.3.0 | Unicode-3.0 |
| icu_provider | 2.3.0 | Unicode-3.0 |
| ident_case | 1.0.1 | MIT OR Apache-2.0 |
| idna | 1.1.0 | MIT OR Apache-2.0 |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT |
| if-addrs | 0.15.0 | MIT OR BSD-3-Clause |
| image | 0.24.9 | MIT OR Apache-2.0 |
| image-webp | 0.2.4 | MIT OR Apache-2.0 |
| imagesize | 0.14.0 | MIT |
| impl-more | 0.3.5 | MIT OR Apache-2.0 |
| indexmap | 2.14.0 | Apache-2.0 OR MIT |
| inout | 0.1.4 | MIT OR Apache-2.0 |
| interceptor | 0.8.2 | MIT OR Apache-2.0 |
| io-lifetimes | 1.0.11 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| ipnet | 2.12.1 | MIT OR Apache-2.0 |
| is-docker | 0.2.0 | MIT |
| is-wsl | 0.4.0 | MIT |
| is_terminal_polyfill | 1.70.2 | MIT OR Apache-2.0 |
| itertools | 0.13.0 | MIT OR Apache-2.0 |
| itoa | 1.0.18 | MIT OR Apache-2.0 |
| jni | 0.21.1 | MIT OR Apache-2.0 |
| jni-sys | 0.3.1 | MIT OR Apache-2.0 |
| jni-sys | 0.4.1 | MIT OR Apache-2.0 |
| jni-sys-macros | 0.4.1 | MIT OR Apache-2.0 |
| jpeg-decoder | 0.3.2 | MIT OR Apache-2.0 |
| js-sys | 0.3.104 | MIT OR Apache-2.0 |
| kurbo | 0.13.1 | Apache-2.0 OR MIT |
| language-tags | 0.3.2 | MIT OR Apache-2.0 |
| lazy_static | 1.5.0 | MIT OR Apache-2.0 |
| lebe | 0.5.3 | BSD-3-Clause |
| libadwaita | 0.7.2 | MIT |
| libadwaita-sys | 0.7.2 | MIT |
| libc | 0.2.189 | MIT OR Apache-2.0 |
| libdbus-sys | 0.2.7 | Apache-2.0 OR MIT |
| libloading | 0.8.9 | ISC |
| libm | 0.2.16 | MIT |
| libwayshot | 0.2.0 | BSD-2-Clause |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| litemap | 0.8.3 | Unicode-3.0 |
| local-channel | 0.1.5 | MIT OR Apache-2.0 |
| local-waker | 0.1.4 | MIT OR Apache-2.0 |
| lock_api | 0.4.14 | MIT OR Apache-2.0 |
| log | 0.4.33 | MIT OR Apache-2.0 |
| match_cfg | 0.1.0 | MIT OR Apache-2.0 |
| md-5 | 0.10.6 | MIT OR Apache-2.0 |
| md5 | 0.8.1 | Apache-2.0 OR MIT |
| mdns-sd | 0.20.3 | Apache-2.0 OR MIT |
| memchr | 2.8.3 | Unlicense OR MIT |
| memmap2 | 0.7.1 | MIT OR Apache-2.0 |
| memmap2 | 0.9.11 | MIT OR Apache-2.0 |
| memoffset | 0.6.5 | MIT |
| memoffset | 0.7.1 | MIT |
| memoffset | 0.9.1 | MIT |
| mime | 0.3.17 | MIT OR Apache-2.0 |
| mime_guess | 2.0.5 | MIT |
| minhook | 0.5.0 | MIT |
| minimal-lexical | 0.2.1 | MIT OR Apache-2.0 |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 |
| mio | 0.8.11 | MIT |
| mio | 1.2.2 | MIT |
| muldiv | 1.0.1 | MIT |
| nanorand | 0.7.0 | Zlib |
| native-tls | 0.2.18 | MIT OR Apache-2.0 |
| network-interface | 1.1.4 | MIT OR Apache-2.0 |
| nix | 0.24.3 | MIT |
| nix | 0.26.4 | MIT |
| nix | 0.28.0 | MIT |
| nix | 0.29.0 | MIT |
| nodeinnet-gtk | 0.5.513 | MIT OR Apache-2.0 |
| nom | 7.1.3 | MIT |
| ntapi | 0.4.3 | Apache-2.0 OR MIT |
| num-bigint | 0.4.8 | MIT OR Apache-2.0 |
| num-complex | 0.4.6 | MIT OR Apache-2.0 |
| num-conv | 0.2.2 | MIT OR Apache-2.0 |
| num-integer | 0.1.47 | MIT OR Apache-2.0 |
| num-rational | 0.4.2 | MIT OR Apache-2.0 |
| num-traits | 0.2.19 | MIT OR Apache-2.0 |
| objc2 | 0.6.4 | MIT |
| objc2-app-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-encode | 4.1.0 | MIT |
| objc2-foundation | 0.3.2 | MIT |
| oid-registry | 0.4.0 | MIT OR Apache-2.0 |
| once_cell | 1.21.4 | MIT OR Apache-2.0 |
| once_cell_polyfill | 1.70.2 | MIT OR Apache-2.0 |
| opaque-debug | 0.3.1 | MIT OR Apache-2.0 |
| open | 5.4.1 | MIT |
| openh264 | 0.4.4 | BSD-2-Clause |
| openh264-sys2 | 0.4.4 | BSD-2-Clause |
| openssl | 0.10.81 | Apache-2.0 |
| openssl-macros | 0.1.1 | MIT OR Apache-2.0 |
| openssl-probe | 0.2.1 | MIT OR Apache-2.0 |
| openssl-sys | 0.9.117 | MIT |
| option-ext | 0.2.0 | MPL-2.0 |
| option-operations | 0.5.0 | MIT OR Apache-2.0 |
| ordered-stream | 0.2.0 | MIT OR Apache-2.0 |
| p256 | 0.11.1 | Apache-2.0 OR MIT |
| p384 | 0.11.2 | Apache-2.0 OR MIT |
| pango | 0.20.12 | MIT |
| pango-sys | 0.20.10 | MIT |
| parking | 2.2.1 | Apache-2.0 OR MIT |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 |
| password-hash | 0.5.0 | MIT OR Apache-2.0 |
| paste | 1.0.15 | MIT OR Apache-2.0 |
| pem | 1.1.1 | MIT |
| pem-rfc7468 | 0.6.0 | Apache-2.0 OR MIT |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 |
| pico-args | 0.5.0 | MIT |
| pin-project | 1.1.13 | Apache-2.0 OR MIT |
| pin-project-internal | 1.1.13 | Apache-2.0 OR MIT |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT |
| pin-utils | 0.1.0 | MIT OR Apache-2.0 |
| piper | 0.2.5 | MIT OR Apache-2.0 |
| pkcs8 | 0.9.0 | Apache-2.0 OR MIT |
| png | 0.17.16 | MIT OR Apache-2.0 |
| png | 0.18.1 | MIT OR Apache-2.0 |
| polling | 3.11.0 | Apache-2.0 OR MIT |
| poly1305 | 0.8.0 | Apache-2.0 OR MIT |
| polycool | 0.4.0 | MIT OR Apache-2.0 |
| polyval | 0.5.3 | Apache-2.0 OR MIT |
| polyval | 0.6.2 | Apache-2.0 OR MIT |
| portable-pty | 0.9.0 | MIT |
| potential_utf | 0.1.6 | Unicode-3.0 |
| powerfmt | 0.2.0 | MIT OR Apache-2.0 |
| ppv-lite86 | 0.2.21 | MIT OR Apache-2.0 |
| proc-macro-crate | 3.5.0 | MIT OR Apache-2.0 |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 |
| pulp | 0.22.3 | MIT |
| pulp-wasm-simd-flag | 0.1.1 | MIT |
| qoi | 0.4.1 | MIT OR Apache-2.0 |
| quick-error | 2.0.1 | MIT OR Apache-2.0 |
| quick-xml | 0.28.2 | MIT |
| quote | 1.0.47 | MIT OR Apache-2.0 |
| radium | 0.7.0 | MIT |
| rand | 0.8.7 | MIT OR Apache-2.0 |
| rand | 0.9.5 | MIT OR Apache-2.0 |
| rand | 0.10.2 | MIT OR Apache-2.0 |
| rand_chacha | 0.3.1 | MIT OR Apache-2.0 |
| rand_chacha | 0.9.0 | MIT OR Apache-2.0 |
| rand_core | 0.5.1 | MIT OR Apache-2.0 |
| rand_core | 0.6.4 | MIT OR Apache-2.0 |
| rand_core | 0.9.5 | MIT OR Apache-2.0 |
| rand_core | 0.10.1 | MIT OR Apache-2.0 |
| raw-cpuid | 11.6.0 | MIT |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib |
| rayon | 1.12.0 | MIT OR Apache-2.0 |
| rayon-core | 1.13.0 | MIT OR Apache-2.0 |
| rcgen | 0.9.3 | MIT OR Apache-2.0 |
| rcgen | 0.10.0 | MIT OR Apache-2.0 |
| reborrow | 0.5.5 | MIT |
| regex | 1.13.1 | MIT OR Apache-2.0 |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 |
| regex-lite | 0.1.9 | MIT OR Apache-2.0 |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 |
| relm4 | 0.9.1 | Apache-2.0 OR MIT |
| relm4-css | 0.9.0 | Apache-2.0 OR MIT |
| relm4-macros | 0.9.1 | Apache-2.0 OR MIT |
| reqwest | 0.11.27 | MIT OR Apache-2.0 |
| resvg | 0.47.0 | Apache-2.0 OR MIT |
| rfc6979 | 0.3.1 | Apache-2.0 OR MIT |
| rfd | 0.15.4 | MIT |
| rgb | 0.8.53 | MIT |
| ring | 0.16.20 | Unknown |
| ring | 0.17.14 | Apache-2.0 AND ISC |
| roxmltree | 0.20.0 | MIT OR Apache-2.0 |
| roxmltree | 0.21.1 | MIT OR Apache-2.0 |
| rpassword | 7.5.4 | Apache-2.0 |
| rtcp | 0.7.2 | MIT OR Apache-2.0 |
| rtoolbox | 0.0.5 | Apache-2.0 |
| rtp | 0.6.8 | MIT OR Apache-2.0 |
| rusticata-macros | 4.1.0 | MIT OR Apache-2.0 |
| rustix | 1.1.4 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| rustls | 0.19.1 | Apache-2.0 OR ISC OR MIT |
| rustls | 0.21.12 | Apache-2.0 OR ISC OR MIT |
| rustls-pemfile | 1.0.4 | Apache-2.0 OR ISC OR MIT |
| rustls-webpki | 0.101.7 | ISC |
| rustversion | 1.0.23 | MIT OR Apache-2.0 |
| rustybuzz | 0.20.1 | MIT |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 |
| same-file | 1.0.6 | Unlicense OR MIT |
| schannel | 0.1.29 | MIT |
| scoped-tls | 1.0.1 | MIT OR Apache-2.0 |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 |
| screencapturekit | 1.5.4 | MIT OR Apache-2.0 |
| screenshots | 0.8.10 | Apache-2.0 |
| sct | 0.6.1 | Apache-2.0 OR ISC OR MIT |
| sct | 0.7.1 | Apache-2.0 OR ISC OR MIT |
| sdp | 0.5.3 | MIT OR Apache-2.0 |
| sec1 | 0.3.0 | Apache-2.0 OR MIT |
| security-framework | 3.7.0 | MIT OR Apache-2.0 |
| security-framework-sys | 2.17.0 | MIT OR Apache-2.0 |
| serde | 1.0.229 | MIT OR Apache-2.0 |
| serde_bytes | 0.11.19 | MIT OR Apache-2.0 |
| serde_core | 1.0.229 | MIT OR Apache-2.0 |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 |
| serde_json | 1.0.151 | MIT OR Apache-2.0 |
| serde_repr | 0.1.21 | MIT OR Apache-2.0 |
| serde_urlencoded | 0.7.1 | MIT OR Apache-2.0 |
| serial2 | 0.2.38 | BSD-2-Clause OR Apache-2.0 |
| sha-1 | 0.9.8 | MIT OR Apache-2.0 |
| sha1 | 0.10.7 | MIT OR Apache-2.0 |
| sha1 | 0.11.0 | MIT OR Apache-2.0 |
| sha2 | 0.10.9 | MIT OR Apache-2.0 |
| shared_library | 0.1.9 | Apache-2.0 OR MIT |
| shell-words | 1.1.1 | MIT OR Apache-2.0 |
| signal-hook | 0.3.18 | Apache-2.0 OR MIT |
| signal-hook-mio | 0.2.5 | MIT OR Apache-2.0 |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 |
| signature | 1.6.4 | Apache-2.0 OR MIT |
| signature | 2.2.0 | Apache-2.0 OR MIT |
| simd-adler32 | 0.3.10 | MIT |
| simdutf8 | 0.1.5 | MIT OR Apache-2.0 |
| simplecss | 0.2.2 | Apache-2.0 OR MIT |
| siphasher | 1.0.3 | MIT OR Apache-2.0 |
| slab | 0.4.12 | MIT |
| slotmap | 1.1.1 | Zlib |
| smallvec | 1.15.2 | MIT OR Apache-2.0 |
| snow | 0.9.6 | Apache-2.0 OR MIT |
| socket-pktinfo | 0.4.1 | MIT |
| socket2 | 0.4.10 | MIT OR Apache-2.0 |
| socket2 | 0.5.10 | MIT OR Apache-2.0 |
| socket2 | 0.6.5 | MIT OR Apache-2.0 |
| spin | 0.5.2 | MIT |
| spin | 0.9.9 | MIT |
| spki | 0.6.0 | Apache-2.0 OR MIT |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 |
| strict-num | 0.1.1 | MIT |
| strsim | 0.10.0 | MIT |
| strsim | 0.11.1 | MIT |
| stun | 0.4.4 | MIT OR Apache-2.0 |
| substring | 1.4.5 | MIT OR Apache-2.0 |
| subtle | 2.4.1 | BSD-3-Clause |
| svgtypes | 0.16.1 | Apache-2.0 OR MIT |
| syn | 1.0.109 | MIT OR Apache-2.0 |
| syn | 2.0.119 | MIT OR Apache-2.0 |
| syn | 3.0.3 | MIT OR Apache-2.0 |
| sync_wrapper | 0.1.2 | Apache-2.0 |
| synstructure | 0.12.6 | MIT |
| synstructure | 0.13.2 | MIT |
| sysinfo | 0.29.11 | MIT |
| system-configuration | 0.5.1 | MIT OR Apache-2.0 |
| system-configuration-sys | 0.5.0 | MIT OR Apache-2.0 |
| tap | 1.0.1 | MIT |
| tempfile | 3.27.0 | MIT OR Apache-2.0 |
| thiserror | 1.0.69 | MIT OR Apache-2.0 |
| thiserror | 2.0.20 | MIT OR Apache-2.0 |
| thiserror-impl | 1.0.69 | MIT OR Apache-2.0 |
| thiserror-impl | 2.0.20 | MIT OR Apache-2.0 |
| tiff | 0.9.1 | MIT |
| time | 0.3.55 | MIT OR Apache-2.0 |
| time-core | 0.1.9 | MIT OR Apache-2.0 |
| time-macros | 0.2.32 | MIT OR Apache-2.0 |
| tiny-skia | 0.12.0 | BSD-3-Clause |
| tiny-skia-path | 0.12.0 | BSD-3-Clause |
| tinystr | 0.8.4 | Unicode-3.0 |
| tinyvec | 1.12.0 | Zlib OR Apache-2.0 OR MIT |
| tinyvec_macros | 0.1.1 | MIT OR Apache-2.0 OR Zlib |
| tokio | 1.53.1 | MIT |
| tokio-macros | 2.7.2 | MIT |
| tokio-native-tls | 0.3.1 | MIT |
| tokio-rustls | 0.24.1 | MIT OR Apache-2.0 |
| tokio-stream | 0.1.19 | MIT |
| tokio-tungstenite | 0.20.1 | MIT |
| tokio-util | 0.7.19 | MIT |
| toml_datetime | 1.1.1+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_edit | 0.25.13+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_parser | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 |
| tower-service | 0.3.3 | MIT |
| tracing | 0.1.44 | MIT |
| tracing-attributes | 0.1.31 | MIT |
| tracing-core | 0.1.36 | MIT |
| try-lock | 0.2.5 | MIT |
| ttf-parser | 0.25.1 | MIT OR Apache-2.0 |
| tungstenite | 0.20.1 | MIT OR Apache-2.0 |
| turn | 0.6.1 | MIT OR Apache-2.0 |
| typenum | 1.20.1 | MIT OR Apache-2.0 |
| uds_windows | 1.2.1 | MIT |
| unicase | 2.9.0 | MIT OR Apache-2.0 |
| unicode-bidi | 0.3.18 | MIT OR Apache-2.0 |
| unicode-bidi-mirroring | 0.4.0 | MIT OR Apache-2.0 |
| unicode-ccc | 0.4.0 | MIT OR Apache-2.0 |
| unicode-ident | 1.0.24 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| unicode-properties | 0.1.4 | MIT OR Apache-2.0 |
| unicode-script | 0.5.8 | MIT OR Apache-2.0 |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 |
| unicode-vo | 0.1.0 | MIT OR Apache-2.0 |
| unicode-width | 0.1.14 | MIT OR Apache-2.0 |
| unicode-xid | 0.2.6 | MIT OR Apache-2.0 |
| universal-hash | 0.4.1 | MIT OR Apache-2.0 |
| universal-hash | 0.5.1 | MIT OR Apache-2.0 |
| untrusted | 0.7.1 | ISC |
| untrusted | 0.9.0 | ISC |
| url | 2.5.8 | MIT OR Apache-2.0 |
| usvg | 0.47.0 | Apache-2.0 OR MIT |
| utf-8 | 0.7.6 | MIT OR Apache-2.0 |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT |
| utf8parse | 0.2.2 | Apache-2.0 OR MIT |
| uuid | 1.24.1 | Apache-2.0 OR MIT |
| vt100 | 0.15.2 | MIT |
| vte | 0.11.1 | Apache-2.0 OR MIT |
| vte_generate_state_changes | 0.1.2 | Apache-2.0 OR MIT |
| waitgroup | 0.1.2 | Apache-2.0 |
| walkdir | 2.5.0 | Unlicense OR MIT |
| want | 0.3.1 | MIT |
| wasm-bindgen | 0.2.127 | MIT OR Apache-2.0 |
| wasm-bindgen-futures | 0.4.77 | MIT OR Apache-2.0 |
| wasm-bindgen-macro | 0.2.127 | MIT OR Apache-2.0 |
| wasm-bindgen-macro-support | 0.2.127 | MIT OR Apache-2.0 |
| wasm-bindgen-shared | 0.2.127 | MIT OR Apache-2.0 |
| wasm-node | 0.5.513 | MIT OR Apache-2.0 |
| wasm-streams | 0.4.2 | MIT OR Apache-2.0 |
| wayland-backend | 0.1.2 | MIT |
| wayland-client | 0.30.2 | MIT |
| wayland-protocols | 0.30.1 | MIT |
| wayland-protocols-wlr | 0.1.0 | MIT |
| wayland-scanner | 0.30.1 | MIT |
| wayland-sys | 0.30.1 | MIT |
| web-sys | 0.3.104 | MIT OR Apache-2.0 |
| webpki | 0.21.4 | ISC |
| webpki-roots | 0.25.4 | MPL-2.0 |
| webrtc | 0.6.0 | MIT OR Apache-2.0 |
| webrtc-data | 0.6.0 | MIT OR Apache-2.0 |
| webrtc-dtls | 0.7.2 | MIT OR Apache-2.0 |
| webrtc-ice | 0.9.1 | MIT OR Apache-2.0 |
| webrtc-mdns | 0.5.2 | MIT OR Apache-2.0 |
| webrtc-media | 0.5.1 | MIT OR Apache-2.0 |
| webrtc-sctp | 0.7.0 | MIT OR Apache-2.0 |
| webrtc-srtp | 0.9.1 | MIT OR Apache-2.0 |
| webrtc-util | 0.7.0 | MIT OR Apache-2.0 |
| weezl | 0.1.12 | MIT OR Apache-2.0 |
| widestring | 1.2.1 | MIT OR Apache-2.0 |
| winapi | 0.3.9 | MIT OR Apache-2.0 |
| winapi-util | 0.1.11 | Unlicense OR MIT |
| winapi-x86_64-pc-windows-gnu | 0.4.0 | MIT OR Apache-2.0 |
| windows | 0.51.1 | MIT OR Apache-2.0 |
| windows | 0.52.0 | MIT OR Apache-2.0 |
| windows | 0.62.2 | MIT OR Apache-2.0 |
| windows-capture | 2.0.1 | MIT |
| windows-collections | 0.3.2 | MIT OR Apache-2.0 |
| windows-core | 0.51.1 | MIT OR Apache-2.0 |
| windows-core | 0.52.0 | MIT OR Apache-2.0 |
| windows-core | 0.62.2 | MIT OR Apache-2.0 |
| windows-future | 0.3.2 | MIT OR Apache-2.0 |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 |
| windows-link | 0.2.1 | MIT OR Apache-2.0 |
| windows-numerics | 0.3.1 | MIT OR Apache-2.0 |
| windows-result | 0.4.1 | MIT OR Apache-2.0 |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 |
| windows-sys | 0.45.0 | MIT OR Apache-2.0 |
| windows-sys | 0.48.0 | MIT OR Apache-2.0 |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 |
| windows-sys | 0.59.0 | MIT OR Apache-2.0 |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 |
| windows-targets | 0.42.2 | MIT OR Apache-2.0 |
| windows-targets | 0.48.5 | MIT OR Apache-2.0 |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 |
| windows-threading | 0.2.1 | MIT OR Apache-2.0 |
| windows_x86_64_gnu | 0.42.2 | MIT OR Apache-2.0 |
| windows_x86_64_gnu | 0.48.5 | MIT OR Apache-2.0 |
| windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 |
| winnow | 1.0.4 | MIT |
| winreg | 0.10.1 | MIT |
| winreg | 0.50.0 | MIT |
| winreg | 0.52.0 | MIT |
| writeable | 0.6.4 | Unicode-3.0 |
| wyz | 0.5.1 | MIT |
| x25519-dalek | 2.0.1 | BSD-3-Clause |
| x509-parser | 0.13.2 | MIT OR Apache-2.0 |
| xcb | 1.7.1 | MIT |
| xdg-home | 1.3.0 | MIT |
| xml-rs | 0.8.29 | MIT |
| xmltree | 0.10.3 | MIT |
| xmlwriter | 0.1.0 | MIT |
| yasna | 0.5.2 | MIT OR Apache-2.0 |
| yoke | 0.8.3 | Unicode-3.0 |
| yoke-derive | 0.8.2 | Unicode-3.0 |
| yuv | 0.8.17 | BSD-3-Clause OR Apache-2.0 |
| zbus | 4.4.0 | MIT |
| zbus_macros | 4.4.0 | MIT |
| zbus_names | 3.0.0 | MIT |
| zerocopy | 0.8.56 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zerocopy-derive | 0.8.56 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zerofrom | 0.1.8 | Unicode-3.0 |
| zerofrom-derive | 0.1.7 | Unicode-3.0 |
| zeroize | 1.9.0 | Apache-2.0 OR MIT |
| zeroize_derive | 1.5.0 | Apache-2.0 OR MIT |
| zerotrie | 0.2.5 | Unicode-3.0 |
| zerovec | 0.11.7 | Unicode-3.0 |
| zerovec-derive | 0.11.4 | Unicode-3.0 |
| zmij | 1.0.23 | MIT |
| zstd | 0.13.3 | MIT |
| zstd-safe | 7.2.4 | MIT OR Apache-2.0 |
| zstd-sys | 2.0.16+zstd.1.5.7 | MIT OR Apache-2.0 |
| zune-core | 0.5.3 | MIT OR Apache-2.0 OR Zlib |
| zune-inflate | 0.2.54 | MIT OR Apache-2.0 OR Zlib |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib |
| zvariant | 4.2.0 | MIT |
| zvariant_derive | 4.2.0 | MIT |
| zvariant_utils | 2.1.0 | MIT |

`Unknown` in the table means the crate declares no SPDX expression in its
`Cargo.toml`; its terms come from the license file it ships. It does not mean the
license is unidentified. One crate is in that position — `ring` 0.16.20, whose
single LICENSE file combines ISC, MIT and OpenSSL notices; `about.toml` accepts
that combination explicitly.

### License texts

Each license appears once below; the table above says which crates it covers.

#### Apache License 2.0 (`Apache-2.0`)

```

                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

   APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "[]"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

   Copyright 2023 Jacob Pratt et al.

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.

```

#### MIT License (`MIT`)

```
    MIT License

    Copyright (c) Microsoft Corporation.

    Permission is hereby granted, free of charge, to any person obtaining a copy
    of this software and associated documentation files (the "Software"), to deal
    in the Software without restriction, including without limitation the rights
    to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
    copies of the Software, and to permit persons to whom the Software is
    furnished to do so, subject to the following conditions:

    The above copyright notice and this permission notice shall be included in all
    copies or substantial portions of the Software.

    THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
    IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
    FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
    AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
    LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
    OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
    SOFTWARE

```

#### ISC License (`ISC`)

```
/* Copyright (c) 2014, Google Inc.
 *
 * Permission to use, copy, modify, and/or distribute this software for any
 * purpose with or without fee is hereby granted, provided that the above
 * copyright notice and this permission notice appear in all copies.
 *
 * THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
 * WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
 * MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY
 * SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
 * WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION
 * OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
 * CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE. */

#ifndef OPENSSL_HEADER_POLY1305_H
#define OPENSSL_HEADER_POLY1305_H

#include <GFp/base.h>

// Keep in sync with `poly1305_state` in poly1305.rs.
typedef uint8_t poly1305_state[512];

#endif  // OPENSSL_HEADER_POLY1305_H

```

#### BSD 2-Clause "Simplified" License (`BSD-2-Clause`)

```
/*!
 * \copy
 *     Copyright (c)  2008-2013, Cisco Systems
 *     All rights reserved.
 *
 *     Redistribution and use in source and binary forms, with or without
 *     modification, are permitted provided that the following conditions
 *     are met:
 *
 *        * Redistributions of source code must retain the above copyright
 *          notice, this list of conditions and the following disclaimer.
 *
 *        * Redistributions in binary form must reproduce the above copyright
 *          notice, this list of conditions and the following disclaimer in
 *          the documentation and/or other materials provided with the
 *          distribution.
 *
 *     THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
 *     "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
 *     LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS
 *     FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE
 *     COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,
 *     INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
 *     BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
 *     LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
 *     CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
 *     LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN
 *     ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 *     POSSIBILITY OF SUCH DAMAGE.
 *
 *  read_config.h
 *
 *  Abstract
 *      Class for reading parameter settings in a configure file.
 *
 *  History
 *      08/18/2008 Created
 *
 *****************************************************************************/
#ifndef READ_CONFIG_H__
#define READ_CONFIG_H__

#include <stdlib.h>
#include <string>


class CReadConfig {
 public:
  CReadConfig();
  CReadConfig (const char* pConfigFileName);
  CReadConfig (const std::string& pConfigFileName);
  virtual ~CReadConfig();

  void Openf (const char* strFile);
  long ReadLine (std::string* strVal, const int iValSize = 4);
  const bool EndOfFile();
  const int GetLines();
  const bool ExistFile();
  const std::string& GetFileName();

 private:
  FILE*             m_pCfgFile;
  std::string       m_strCfgFileName;
  unsigned int      m_iLines;
};

#endif // READ_CONFIG_H__


```

#### Unicode License v3 (`Unicode-3.0`)

```
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 1991-2023 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

```

#### BSD 3-Clause "New" or "Revised" License (`BSD-3-Clause`)

```
// Copyright © WHATWG (Apple, Google, Mozilla, Microsoft).
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this
//    list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
//    this list of conditions and the following disclaimer in the documentation
//    and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its
//    contributors may be used to endorse or promote products derived from
//    this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
// OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

/// The PUA code points special-cased in the GB18030 encoder.
pub(crate) static GB18030_2022_OVERRIDE_PUA: [u16; 18] = [
    0xE78D, 0xE78E, 0xE78F, 0xE790, 0xE791, 0xE792, 0xE793, 0xE794, 0xE795, 0xE796, 0xE81E, 0xE826,
    0xE82B, 0xE82C, 0xE832, 0xE843, 0xE854, 0xE864,
];

/// The bytes corresponding to the PUA code points special-cased in the GB18030 encoder.
pub(crate) static GB18030_2022_OVERRIDE_BYTES: [[u8; 2]; 18] = [
    [0xA6, 0xD9],
    [0xA6, 0xDA],
    [0xA6, 0xDB],
    [0xA6, 0xDC],
    [0xA6, 0xDD],
    [0xA6, 0xDE],
    [0xA6, 0xDF],
    [0xA6, 0xEC],
    [0xA6, 0xED],
    [0xA6, 0xF3],
    [0xFE, 0x59],
    [0xFE, 0x61],
    [0xFE, 0x66],
    [0xFE, 0x67],
    [0xFE, 0x6D],
    [0xFE, 0x7E],
    [0xFE, 0x90],
    [0xFE, 0xA0],
];

```

#### SSLeay License - standalone (`SSLeay-standalone`)

```
/* Copyright (C) 1995-1998 Eric Young (eay@cryptsoft.com)
 * All rights reserved.
 *
 * This package is an SSL implementation written
 * by Eric Young (eay@cryptsoft.com).
 * The implementation was written so as to conform with Netscapes SSL.
 *
 * This library is free for commercial and non-commercial use as long as
 * the following conditions are aheared to.  The following conditions
 * apply to all code found in this distribution, be it the RC4, RSA,
 * lhash, DES, etc., code; not just the SSL code.  The SSL documentation
 * included with this distribution is covered by the same copyright terms
 * except that the holder is Tim Hudson (tjh@cryptsoft.com).
 *
 * Copyright remains Eric Young's, and as such any Copyright notices in
 * the code are not to be removed.
 * If this package is used in a product, Eric Young should be given attribution
 * as the author of the parts of the library used.
 * This can be in the form of a textual message at program startup or
 * in documentation (online or textual) provided with the package.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 * 3. All advertising materials mentioning features or use of this software
 *    must display the following acknowledgement:
 *    "This product includes cryptographic software written by
 *     Eric Young (eay@cryptsoft.com)"
 *    The word 'cryptographic' can be left out if the rouines from the library
 *    being used are not cryptographic related :-).
 * 4. If you include any Windows specific code (or a derivative thereof) from
 *    the apps directory (application code) you must include an acknowledgement:
 *    "This product includes software written by Tim Hudson (tjh@cryptsoft.com)"
 *
 * THIS SOFTWARE IS PROVIDED BY ERIC YOUNG ``AS IS'' AND
 * ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 * ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
 * FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
 * OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
 * HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
 * LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
 * OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
 * SUCH DAMAGE.
 *
 * The licence and distribution terms for any publically available version or
 * derivative of this code cannot be changed.  i.e. this code cannot simply be
 * copied and put under another distribution licence
 * [including the GNU Public Licence.]
 *
 * This product includes cryptographic software written by Eric Young
 * (eay@cryptsoft.com).  This product includes software written by Tim
 * Hudson (tjh@cryptsoft.com). */

#ifndef OPENSSL_HEADER_CPU_H
#define OPENSSL_HEADER_CPU_H

#include <GFp/base.h>

// Runtime CPU feature support


#if defined(OPENSSL_X86) || defined(OPENSSL_X86_64)
// GFp_ia32cap_P contains the Intel CPUID bits when running on an x86 or
// x86-64 system.
//
//   Index 0:
//     EDX for CPUID where EAX = 1
//     Bit 20 is always zero
//     Bit 28 is adjusted to reflect whether the data cache is shared between
//       multiple logical cores
//     Bit 30 is used to indicate an Intel CPU
//   Index 1:
//     ECX for CPUID where EAX = 1
//     Bit 11 is used to indicate AMD XOP support, not SDBG
//   Index 2:
//     EBX for CPUID where EAX = 7
//   Index 3:
//     ECX for CPUID where EAX = 7
//
// Note: the CPUID bits are pre-adjusted for the OSXSAVE bit and the YMM and XMM
// bits in XCR0, so it is not necessary to check those.
extern uint32_t GFp_ia32cap_P[4];
#endif

#endif  // OPENSSL_HEADER_CPU_H

```

#### zlib License (`Zlib`)

```
Copyright (c) 2021 Orson Peters <orsonpeters@gmail.com>

This software is provided 'as-is', without any express or implied warranty. In
no event will the authors be held liable for any damages arising from the use of
this software.

Permission is granted to anyone to use this software for any purpose, including
commercial applications, and to alter it and redistribute it freely, subject to
the following restrictions:

 1. The origin of this software must not be misrepresented; you must not claim
    that you wrote the original software. If you use this software in a product,
    an acknowledgment in the product documentation would be appreciated but is
    not required.

 2. Altered source versions must be plainly marked as such, and must not be
    misrepresented as being the original software.

 3. This notice may not be removed or altered from any source distribution.

```

#### Mozilla Public License 2.0 (`MPL-2.0`)

```
Mozilla Public License Version 2.0
==================================

1. Definitions
--------------

1.1. "Contributor"
    means each individual or legal entity that creates, contributes to
    the creation of, or owns Covered Software.

1.2. "Contributor Version"
    means the combination of the Contributions of others (if any) used
    by a Contributor and that particular Contributor's Contribution.

1.3. "Contribution"
    means Covered Software of a particular Contributor.

1.4. "Covered Software"
    means Source Code Form to which the initial Contributor has attached
    the notice in Exhibit A, the Executable Form of such Source Code
    Form, and Modifications of such Source Code Form, in each case
    including portions thereof.

1.5. "Incompatible With Secondary Licenses"
    means

    (a) that the initial Contributor has attached the notice described
        in Exhibit B to the Covered Software; or

    (b) that the Covered Software was made available under the terms of
        version 1.1 or earlier of the License, but not also under the
        terms of a Secondary License.

1.6. "Executable Form"
    means any form of the work other than Source Code Form.

1.7. "Larger Work"
    means a work that combines Covered Software with other material, in 
    a separate file or files, that is not Covered Software.

1.8. "License"
    means this document.

1.9. "Licensable"
    means having the right to grant, to the maximum extent possible,
    whether at the time of the initial grant or subsequently, any and
    all of the rights conveyed by this License.

1.10. "Modifications"
    means any of the following:

    (a) any file in Source Code Form that results from an addition to,
        deletion from, or modification of the contents of Covered
        Software; or

    (b) any new file in Source Code Form that contains any Covered
        Software.

1.11. "Patent Claims" of a Contributor
    means any patent claim(s), including without limitation, method,
    process, and apparatus claims, in any patent Licensable by such
    Contributor that would be infringed, but for the grant of the
    License, by the making, using, selling, offering for sale, having
    made, import, or transfer of either its Contributions or its
    Contributor Version.

1.12. "Secondary License"
    means either the GNU General Public License, Version 2.0, the GNU
    Lesser General Public License, Version 2.1, the GNU Affero General
    Public License, Version 3.0, or any later versions of those
    licenses.

1.13. "Source Code Form"
    means the form of the work preferred for making modifications.

1.14. "You" (or "Your")
    means an individual or a legal entity exercising rights under this
    License. For legal entities, "You" includes any entity that
    controls, is controlled by, or is under common control with You. For
    purposes of this definition, "control" means (a) the power, direct
    or indirect, to cause the direction or management of such entity,
    whether by contract or otherwise, or (b) ownership of more than
    fifty percent (50%) of the outstanding shares or beneficial
    ownership of such entity.

2. License Grants and Conditions
--------------------------------

2.1. Grants

Each Contributor hereby grants You a world-wide, royalty-free,
non-exclusive license:

(a) under intellectual property rights (other than patent or trademark)
    Licensable by such Contributor to use, reproduce, make available,
    modify, display, perform, distribute, and otherwise exploit its
    Contributions, either on an unmodified basis, with Modifications, or
    as part of a Larger Work; and

(b) under Patent Claims of such Contributor to make, use, sell, offer
    for sale, have made, import, and otherwise transfer either its
    Contributions or its Contributor Version.

2.2. Effective Date

The licenses granted in Section 2.1 with respect to any Contribution
become effective for each Contribution on the date the Contributor first
distributes such Contribution.

2.3. Limitations on Grant Scope

The licenses granted in this Section 2 are the only rights granted under
this License. No additional rights or licenses will be implied from the
distribution or licensing of Covered Software under this License.
Notwithstanding Section 2.1(b) above, no patent license is granted by a
Contributor:

(a) for any code that a Contributor has removed from Covered Software;
    or

(b) for infringements caused by: (i) Your and any other third party's
    modifications of Covered Software, or (ii) the combination of its
    Contributions with other software (except as part of its Contributor
    Version); or

(c) under Patent Claims infringed by Covered Software in the absence of
    its Contributions.

This License does not grant any rights in the trademarks, service marks,
or logos of any Contributor (except as may be necessary to comply with
the notice requirements in Section 3.4).

2.4. Subsequent Licenses

No Contributor makes additional grants as a result of Your choice to
distribute the Covered Software under a subsequent version of this
License (see Section 10.2) or under the terms of a Secondary License (if
permitted under the terms of Section 3.3).

2.5. Representation

Each Contributor represents that the Contributor believes its
Contributions are its original creation(s) or it has sufficient rights
to grant the rights to its Contributions conveyed by this License.

2.6. Fair Use

This License is not intended to limit any rights You have under
applicable copyright doctrines of fair use, fair dealing, or other
equivalents.

2.7. Conditions

Sections 3.1, 3.2, 3.3, and 3.4 are conditions of the licenses granted
in Section 2.1.

3. Responsibilities
-------------------

3.1. Distribution of Source Form

All distribution of Covered Software in Source Code Form, including any
Modifications that You create or to which You contribute, must be under
the terms of this License. You must inform recipients that the Source
Code Form of the Covered Software is governed by the terms of this
License, and how they can obtain a copy of this License. You may not
attempt to alter or restrict the recipients' rights in the Source Code
Form.

3.2. Distribution of Executable Form

If You distribute Covered Software in Executable Form then:

(a) such Covered Software must also be made available in Source Code
    Form, as described in Section 3.1, and You must inform recipients of
    the Executable Form how they can obtain a copy of such Source Code
    Form by reasonable means in a timely manner, at a charge no more
    than the cost of distribution to the recipient; and

(b) You may distribute such Executable Form under the terms of this
    License, or sublicense it under different terms, provided that the
    license for the Executable Form does not attempt to limit or alter
    the recipients' rights in the Source Code Form under this License.

3.3. Distribution of a Larger Work

You may create and distribute a Larger Work under terms of Your choice,
provided that You also comply with the requirements of this License for
the Covered Software. If the Larger Work is a combination of Covered
Software with a work governed by one or more Secondary Licenses, and the
Covered Software is not Incompatible With Secondary Licenses, this
License permits You to additionally distribute such Covered Software
under the terms of such Secondary License(s), so that the recipient of
the Larger Work may, at their option, further distribute the Covered
Software under the terms of either this License or such Secondary
License(s).

3.4. Notices

You may not remove or alter the substance of any license notices
(including copyright notices, patent notices, disclaimers of warranty,
or limitations of liability) contained within the Source Code Form of
the Covered Software, except that You may alter any license notices to
the extent required to remedy known factual inaccuracies.

3.5. Application of Additional Terms

You may choose to offer, and to charge a fee for, warranty, support,
indemnity or liability obligations to one or more recipients of Covered
Software. However, You may do so only on Your own behalf, and not on
behalf of any Contributor. You must make it absolutely clear that any
such warranty, support, indemnity, or liability obligation is offered by
You alone, and You hereby agree to indemnify every Contributor for any
liability incurred by such Contributor as a result of warranty, support,
indemnity or liability terms You offer. You may include additional
disclaimers of warranty and limitations of liability specific to any
jurisdiction.

4. Inability to Comply Due to Statute or Regulation
---------------------------------------------------

If it is impossible for You to comply with any of the terms of this
License with respect to some or all of the Covered Software due to
statute, judicial order, or regulation then You must: (a) comply with
the terms of this License to the maximum extent possible; and (b)
describe the limitations and the code they affect. Such description must
be placed in a text file included with all distributions of the Covered
Software under this License. Except to the extent prohibited by statute
or regulation, such description must be sufficiently detailed for a
recipient of ordinary skill to be able to understand it.

5. Termination
--------------

5.1. The rights granted under this License will terminate automatically
if You fail to comply with any of its terms. However, if You become
compliant, then the rights granted under this License from a particular
Contributor are reinstated (a) provisionally, unless and until such
Contributor explicitly and finally terminates Your grants, and (b) on an
ongoing basis, if such Contributor fails to notify You of the
non-compliance by some reasonable means prior to 60 days after You have
come back into compliance. Moreover, Your grants from a particular
Contributor are reinstated on an ongoing basis if such Contributor
notifies You of the non-compliance by some reasonable means, this is the
first time You have received notice of non-compliance with this License
from such Contributor, and You become compliant prior to 30 days after
Your receipt of the notice.

5.2. If You initiate litigation against any entity by asserting a patent
infringement claim (excluding declaratory judgment actions,
counter-claims, and cross-claims) alleging that a Contributor Version
directly or indirectly infringes any patent, then the rights granted to
You by any and all Contributors for the Covered Software under Section
2.1 of this License shall terminate.

5.3. In the event of termination under Sections 5.1 or 5.2 above, all
end user license agreements (excluding distributors and resellers) which
have been validly granted by You or Your distributors under this License
prior to termination shall survive termination.

************************************************************************
*                                                                      *
*  6. Disclaimer of Warranty                                           *
*  -------------------------                                           *
*                                                                      *
*  Covered Software is provided under this License on an "as is"       *
*  basis, without warranty of any kind, either expressed, implied, or  *
*  statutory, including, without limitation, warranties that the       *
*  Covered Software is free of defects, merchantable, fit for a        *
*  particular purpose or non-infringing. The entire risk as to the     *
*  quality and performance of the Covered Software is with You.        *
*  Should any Covered Software prove defective in any respect, You     *
*  (not any Contributor) assume the cost of any necessary servicing,   *
*  repair, or correction. This disclaimer of warranty constitutes an   *
*  essential part of this License. No use of any Covered Software is   *
*  authorized under this License except under this disclaimer.         *
*                                                                      *
************************************************************************

************************************************************************
*                                                                      *
*  7. Limitation of Liability                                          *
*  --------------------------                                          *
*                                                                      *
*  Under no circumstances and under no legal theory, whether tort      *
*  (including negligence), contract, or otherwise, shall any           *
*  Contributor, or anyone who distributes Covered Software as          *
*  permitted above, be liable to You for any direct, indirect,         *
*  special, incidental, or consequential damages of any character      *
*  including, without limitation, damages for lost profits, loss of    *
*  goodwill, work stoppage, computer failure or malfunction, or any    *
*  and all other commercial damages or losses, even if such party      *
*  shall have been informed of the possibility of such damages. This   *
*  limitation of liability shall not apply to liability for death or   *
*  personal injury resulting from such party's negligence to the       *
*  extent applicable law prohibits such limitation. Some               *
*  jurisdictions do not allow the exclusion or limitation of           *
*  incidental or consequential damages, so this exclusion and          *
*  limitation may not apply to You.                                    *
*                                                                      *
************************************************************************

8. Litigation
-------------

Any litigation relating to this License may be brought only in the
courts of a jurisdiction where the defendant maintains its principal
place of business and such litigation shall be governed by laws of that
jurisdiction, without reference to its conflict-of-law provisions.
Nothing in this Section shall prevent a party's ability to bring
cross-claims or counter-claims.

9. Miscellaneous
----------------

This License represents the complete agreement concerning the subject
matter hereof. If any provision of this License is held to be
unenforceable, such provision shall be reformed only to the extent
necessary to make it enforceable. Any law or regulation which provides
that the language of a contract shall be construed against the drafter
shall not be used to construe this License against a Contributor.

10. Versions of the License
---------------------------

10.1. New Versions

Mozilla Foundation is the license steward. Except as provided in Section
10.3, no one other than the license steward has the right to modify or
publish new versions of this License. Each version will be given a
distinguishing version number.

10.2. Effect of New Versions

You may distribute the Covered Software under the terms of the version
of the License under which You originally received the Covered Software,
or under the terms of any subsequent version published by the license
steward.

10.3. Modified Versions

If you create software not governed by this License, and you want to
create a new license for such software, you may create and use a
modified version of this License if you rename the license and remove
any references to the name of the license steward (except to note that
such modified license differs from this License).

10.4. Distributing Source Code Form that is Incompatible With Secondary
Licenses

If You choose to distribute Source Code Form that is Incompatible With
Secondary Licenses under the terms of this version of the License, the
notice described in Exhibit B of this License must be attached.

Exhibit A - Source Code Form License Notice
-------------------------------------------

  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at http://mozilla.org/MPL/2.0/.

If it is not possible or desirable to put the notice in a particular
file, then You may include the notice in a location (such as a LICENSE
file in a relevant directory) where a recipient would be likely to look
for such a notice.

You may add additional accurate notices of copyright ownership.

Exhibit B - "Incompatible With Secondary Licenses" Notice
---------------------------------------------------------

  This Source Code Form is "Incompatible With Secondary Licenses", as
  defined by the Mozilla Public License, v. 2.0.

```

#### OpenSSL License (`OpenSSL`)

```
/* Copyright (C) 1995-1998 Eric Young (eay@cryptsoft.com)
 * All rights reserved.
 *
 * This package is an SSL implementation written
 * by Eric Young (eay@cryptsoft.com).
 * The implementation was written so as to conform with Netscapes SSL.
 *
 * This library is free for commercial and non-commercial use as long as
 * the following conditions are aheared to.  The following conditions
 * apply to all code found in this distribution, be it the RC4, RSA,
 * lhash, DES, etc., code; not just the SSL code.  The SSL documentation
 * included with this distribution is covered by the same copyright terms
 * except that the holder is Tim Hudson (tjh@cryptsoft.com).
 *
 * Copyright remains Eric Young's, and as such any Copyright notices in
 * the code are not to be removed.
 * If this package is used in a product, Eric Young should be given attribution
 * as the author of the parts of the library used.
 * This can be in the form of a textual message at program startup or
 * in documentation (online or textual) provided with the package.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 * 3. All advertising materials mentioning features or use of this software
 *    must display the following acknowledgement:
 *    "This product includes cryptographic software written by
 *     Eric Young (eay@cryptsoft.com)"
 *    The word 'cryptographic' can be left out if the rouines from the library
 *    being used are not cryptographic related :-).
 * 4. If you include any Windows specific code (or a derivative thereof) from
 *    the apps directory (application code) you must include an acknowledgement:
 *    "This product includes software written by Tim Hudson (tjh@cryptsoft.com)"
 *
 * THIS SOFTWARE IS PROVIDED BY ERIC YOUNG ``AS IS'' AND
 * ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 * ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
 * FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
 * OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
 * HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
 * LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
 * OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
 * SUCH DAMAGE.
 *
 * The licence and distribution terms for any publically available version or
 * derivative of this code cannot be changed.  i.e. this code cannot simply be
 * copied and put under another distribution licence
 * [including the GNU Public Licence.]
 */
/* ====================================================================
 * Copyright (c) 1998-2006 The OpenSSL Project.  All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 *
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 *
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in
 *    the documentation and/or other materials provided with the
 *    distribution.
 *
 * 3. All advertising materials mentioning features or use of this
 *    software must display the following acknowledgment:
 *    "This product includes software developed by the OpenSSL Project
 *    for use in the OpenSSL Toolkit. (http://www.openssl.org/)"
 *
 * 4. The names "OpenSSL Toolkit" and "OpenSSL Project" must not be used to
 *    endorse or promote products derived from this software without
 *    prior written permission. For written permission, please contact
 *    openssl-core@openssl.org.
 *
 * 5. Products derived from this software may not be called "OpenSSL"
 *    nor may "OpenSSL" appear in their names without prior written
 *    permission of the OpenSSL Project.
 *
 * 6. Redistributions of any form whatsoever must retain the following
 *    acknowledgment:
 *    "This product includes software developed by the OpenSSL Project
 *    for use in the OpenSSL Toolkit (http://www.openssl.org/)"
 *
 * THIS SOFTWARE IS PROVIDED BY THE OpenSSL PROJECT ``AS IS'' AND ANY
 * EXPRESSED OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE OpenSSL PROJECT OR
 * ITS CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
 * SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT
 * NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
 * LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
 * HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT,
 * STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED
 * OF THE POSSIBILITY OF SUCH DAMAGE.
 * ====================================================================
 *
 * This product includes cryptographic software written by Eric Young
 * (eay@cryptsoft.com).  This product includes software written by Tim
 * Hudson (tjh@cryptsoft.com). */

#include "internal.h"
#include "../../internal.h"

#include "../../limbs/limbs.h"
#include "../../limbs/limbs.inl"

OPENSSL_STATIC_ASSERT(BN_MONT_CTX_N0_LIMBS == 1 || BN_MONT_CTX_N0_LIMBS == 2,
  "BN_MONT_CTX_N0_LIMBS value is invalid");
OPENSSL_STATIC_ASSERT(
  sizeof(BN_ULONG) * BN_MONT_CTX_N0_LIMBS == sizeof(uint64_t),
  "uint64_t is insufficient precision for n0");

int GFp_bn_from_montgomery_in_place(BN_ULONG r[], size_t num_r, BN_ULONG a[],
                                    size_t num_a, const BN_ULONG n[],
                                    size_t num_n,
                                    const BN_ULONG n0_[BN_MONT_CTX_N0_LIMBS]) {
  if (num_n == 0 || num_r != num_n || num_a != 2 * num_n) {
    return 0;
  }

  // Add multiples of |n| to |r| until R = 2^(nl * BN_BITS2) divides it. On
  // input, we had |r| < |n| * R, so now |r| < 2 * |n| * R. Note that |r|
  // includes |carry| which is stored separately.
  BN_ULONG n0 = n0_[0];
  BN_ULONG carry = 0;
  for (size_t i = 0; i < num_n; i++) {
    BN_ULONG v = GFp_limbs_mul_add_limb(a + i, n, a[i] * n0, num_n);
    v += carry + a[i + num_n];
    carry |= (v != a[i + num_n]);
    carry &= (v <= a[i + num_n]);
    a[i + num_n] = v;
  }

  // Shift |num_n| words to divide by R. We have |a| < 2 * |n|. Note that |a|
  // includes |carry| which is stored separately.
  a += num_n;

  // |a| thus requires at most one additional subtraction |n| to be reduced.
  // Subtract |n| and select the answer in constant time.
  BN_ULONG v = limbs_sub(r, a, n, num_n) - carry;
  // |v| is one if |a| - |n| underflowed or zero if it did not. Note |v| cannot
  // be -1. That would imply the subtraction did not fit in |num_n| words, and
  // we know at most one subtraction is needed.
  v = 0u - v;
  for (size_t i = 0; i < num_n; i++) {
    r[i] = constant_time_select_w(v, a[i], r[i]);
    a[i] = 0;
  }
  return 1;
}

```

#### OpenSSL License - standalone (`OpenSSL-standalone`)

```
/* ====================================================================
 * Copyright (c) 2002-2006 The OpenSSL Project.  All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 *
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 *
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in
 *    the documentation and/or other materials provided with the
 *    distribution.
 *
 * 3. All advertising materials mentioning features or use of this
 *    software must display the following acknowledgment:
 *    "This product includes software developed by the OpenSSL Project
 *    for use in the OpenSSL Toolkit. (http://www.openssl.org/)"
 *
 * 4. The names "OpenSSL Toolkit" and "OpenSSL Project" must not be used to
 *    endorse or promote products derived from this software without
 *    prior written permission. For written permission, please contact
 *    openssl-core@openssl.org.
 *
 * 5. Products derived from this software may not be called "OpenSSL"
 *    nor may "OpenSSL" appear in their names without prior written
 *    permission of the OpenSSL Project.
 *
 * 6. Redistributions of any form whatsoever must retain the following
 *    acknowledgment:
 *    "This product includes software developed by the OpenSSL Project
 *    for use in the OpenSSL Toolkit (http://www.openssl.org/)"
 *
 * THIS SOFTWARE IS PROVIDED BY THE OpenSSL PROJECT ``AS IS'' AND ANY
 * EXPRESSED OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE OpenSSL PROJECT OR
 * ITS CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
 * SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT
 * NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
 * LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
 * HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT,
 * STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED
 * OF THE POSSIBILITY OF SUCH DAMAGE.
 * ==================================================================== */

#ifndef OPENSSL_HEADER_AES_H
#define OPENSSL_HEADER_AES_H

#include <GFp/base.h>

// Raw AES functions.


// AES_MAXNR is the maximum number of AES rounds.
#define AES_MAXNR 14

// aes_key_st should be an opaque type, but EVP requires that the size be
// known.
struct aes_key_st {
  uint32_t rd_key[4 * (AES_MAXNR + 1)];
  unsigned rounds;
};
typedef struct aes_key_st AES_KEY;

#endif  // OPENSSL_HEADER_AES_H

```

