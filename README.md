# node.in.net

A peer-to-peer client: browse a remote machine's files, open a terminal on it,
watch and control its screen, use its internet connection — directly between two
devices, without the traffic passing through a server.

The same core ships as a GTK4 desktop application, a headless console node, and
an Android app.

## What a node can do

| | |
| --- | --- |
| Files | Browse, transfer and manage a peer's shared folders, or mount one as a local drive over WebDAV. |
| Terminal | A real PTY on the remote machine. |
| Remote desktop | H.264 screen sharing with input control, over WebRTC. |
| Network | Route your traffic through a peer's internet connection, or a peer's through yours. |
| Registry | Read and edit a Windows peer's registry. |
| Sync | Keep a folder mirrored between two nodes. |
| System info | A peer's OS, CPU, memory and uptime. |

Every capability is opt-in on the serving side. A node that shares nothing still
connects and uses what its peers offer.

## Crates

| Crate | What it is |
| --- | --- |
| `nodeinnet-gtk` (`src/gtk-app`) | The desktop application. |
| `app-core` | Application state and logic, with no network and no toolkit. Everything else plugs into its traits. |
| `app-net` | The network side: message routing, node runtime, device identity, account sign-in. |
| `app-headless` | The same application driven over a local REST API instead of a window, for automated runs. |
| `console-app` | A headless node for servers. |
| `android-node`, `iphone-node` | JNI and FFI wrappers around the same core. |
| `wasm-node` | The protocol compiled to WebAssembly for the browser client. |
| `node-network` | A small library loaded into a program you launch, so its connections go through the peer instead of your own network. See below. |

The transport, the capability implementations and the shared widgets live in
separate repositories, pulled in as submodules:
[`p2p-common`](https://github.com/node-in-net/p2p-common),
[`p2p-functions`](https://github.com/node-in-net/p2p-functions),
[`ui-common`](https://github.com/node-in-net/ui-common) and
[`common`](https://github.com/node-in-net/common).

## Sending one program's traffic through a peer

Routing a whole machine through a peer needs a system-wide VPN and the privileges
that come with it. `node-network` does something narrower: it is loaded into a
single program that you start from the application, and inside that one process it
replaces the socket calls — `connect`, `sendto`, `sendmsg` — so that program's
connections go to a SOCKS proxy on loopback, which forwards them over the P2P link.
Nothing else on the machine is affected.

The mechanism is worth stating plainly, because it is the same mechanism unpleasant
software uses and an automated scan will say so. On Linux and macOS the library is
loaded with `LD_PRELOAD` / `DYLD_INSERT_LIBRARIES`. On Windows there is no such
loader, so the child is started suspended and the library is written into it with
`VirtualAllocEx` and `CreateRemoteThread`, then the calls are patched with MinHook.

What makes it something other than a backdoor is the surrounding rules, and they are
in the source:

- It only ever enters a process **this application started**, at the user's request,
  from a list the user maintains. It is never injected into a running process, and
  there is no code that enumerates or attaches to one.
- With no proxy port configured it **fails closed**: sockets are refused rather than
  allowed out unnoticed, so a misconfiguration cannot silently leak the traffic it
  was meant to redirect (`sys_unix.rs`, `sys_windows.rs`).
- It reads no keystrokes, no screen and no files. It rewrites a destination address
  and nothing else.
- The peer on the other end must be sharing its network, and says so in its own
  settings.

## Building from source

### Get the code

Submodules are required — the workspace does not resolve without them.

```sh
git clone --recurse-submodules https://github.com/node-in-net/app.git
cd app
```

Already cloned without them? `git submodule update --init --recursive`.

### Rust

Rust **1.85 or newer** — parts of the workspace use edition 2024. With no
toolchain installed:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"
```

Already have rustup? `rustup update stable`.

### System libraries

The desktop application needs GTK4 and libadwaita; screen capture needs PipeWire
and GStreamer.

**Debian / Ubuntu**

```sh
sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev \
    libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
    libssl-dev libdbus-1-dev
```

**Fedora**

```sh
sudo dnf install gcc pkgconf-pkg-config gtk4-devel libadwaita-devel \
    gstreamer1-devel gstreamer1-plugins-base-devel openssl-devel dbus-devel
```

**Arch**

```sh
sudo pacman -S base-devel pkgconf gtk4 libadwaita gstreamer gst-plugins-base openssl
```

Sharing your screen on Linux also needs a working `xdg-desktop-portal` with a
backend for your desktop (`xdg-desktop-portal-gnome`, `-kde`, `-wlr`).

### Build and run

```sh
cargo build --release -p nodeinnet-gtk     # desktop application
cargo run   --release -p nodeinnet-gtk
```

On a machine with no display — a server, a container — build the console node
instead. It needs none of the GTK libraries above:

```sh
cargo build --release -p console-app
cargo run   --release -p console-app -- --setup
```

### Windows

The released installer is cross-compiled from Linux with the GNU toolchain and
packaged with NSIS. The installer also bundles the GTK4 runtime for Windows,
which is not part of this repository — put those DLLs in
`artifacts/gtk4-win32-x64/`, taken from an MSYS2 MinGW64 install
(`mingw-w64-x86_64-gtk4` and `mingw-w64-x86_64-libadwaita` with their
dependencies).

```sh
sudo pacman -S mingw-w64-gcc nsis          # Arch;  Debian: gcc-mingw-w64 nsis
rustup target add x86_64-pc-windows-gnu

# setup.nsi picks the binaries up from this exact directory.
export CARGO_TARGET_DIR=bin/distr/exe/target
cargo build --release --target x86_64-pc-windows-gnu \
    --manifest-path src/node-network/Cargo.toml
cargo build --release --target x86_64-pc-windows-gnu \
    --manifest-path src/gtk-app/Cargo.toml
makensis src/gtk-app/setup.nsi
```

`builder/build.sh exe` runs the same steps.

Building natively on Windows works too. Install [MSYS2](https://www.msys2.org/),
then from the **MinGW64** shell:

```sh
pacman -S mingw-w64-x86_64-gtk4 mingw-w64-x86_64-libadwaita \
          mingw-w64-x86_64-pkgconf mingw-w64-x86_64-gcc
rustup toolchain install stable-x86_64-pc-windows-gnu
cargo build --release -p nodeinnet-gtk
```

Run `cargo` from that shell so `pkg-config` finds the GTK libraries.

### macOS

```sh
brew install gtk4 libadwaita gstreamer pkg-config
cargo build --release -p nodeinnet-gtk
```

Screen capture uses ScreenCaptureKit, which needs macOS 12.3 or newer.

### Android

Needs the Android SDK and NDK:

```sh
cargo install cargo-ndk
rustup target add aarch64-linux-android
npm run build-android-node          # native library into the app project
cd src/android-app && ./gradlew assembleDebug
```

### Distributable packages

`builder/build.sh <platform> [app]` produces an installable package — `deb`,
`rpm`, `zst`, `exe`, `dmg`, `apk`. It expects the toolchains above and is what
the release pipeline runs.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
