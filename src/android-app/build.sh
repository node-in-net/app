#!/usr/bin/env bash
# Builds libnodeinnet_android.so against the GTK stack release that gtk-stack.lock pins
# (android-rust-gtk4-adwaita), into ../../bin/android-gtk-jniLibs/arm64-v8a/ for the Gradle module.
#
#   src/android-app/build.sh [release|dev|check|clippy]
#
# dev: the android-dev profile (thin LTO), for iterating; release: the smallest library.
#
# GTK_STACK: a checkout of that repository, relative to nodeinnet/app; without it the release is
# fetched into ~/.cache/android-rust-gtk4-adwaita/<version>/ (npm run fetch-gtk-stack).
set -euo pipefail

CMD="${1:-release}"
ABI=arm64-v8a
HERE="$(cd "$(dirname "$0")" && pwd)"
APP="$(cd "$HERE/../.." && pwd)"
source "$HERE/gtk-stack.bash"
resolve_gtk_stack "$HERE/fetch-gtk-stack.sh" "$APP/gtk-stack.lock" "$ABI"
ROOT="$GTK_STACK/bin/gtk-stack/root-aarch64"
[ -d "$ROOT/lib/$ABI/pkgconfig" ] || { echo "No $ROOT — build the GTK stack in $GTK_STACK first." >&2; exit 1; }

# Target-scoped: the host build scripts (glib-build-tools links the host GIO) keep the host pkg-config.
gtk_stack_cargo_env "$ABI" "$APP"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-${ANDROID_HOME:-/opt/android-sdk}/ndk/27.0.12077973}"
export PATH="$HOME/.cargo/bin:$PATH"

cd "$APP"
OUT="$APP/bin/android-gtk-jniLibs"
# openh264 (remote desktop decoding) is C++: libc++_shared.so goes next to the library.
case "$CMD" in
	release) cargo ndk --platform 31 -t "$ABI" --link-libcxx-shared -o "$OUT" build -p nodeinnet-android --profile android ;;
	dev) cargo ndk --platform 31 -t "$ABI" --link-libcxx-shared -o "$OUT" build -p nodeinnet-android --profile android-dev ;;
	*) cargo ndk --platform 31 -t "$ABI" "$CMD" -p nodeinnet-android --profile android-dev ;;
esac
case "$CMD" in
	release | dev) check_build_paths "$OUT/$ABI/libnodeinnet_android.so" ;;
esac
