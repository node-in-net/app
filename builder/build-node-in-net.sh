#!/bin/bash
set -e

# Set output directory to distr/node.in.net for the main app
export OUT_DIR="distr/node.in.net"

# Clean target directory
rm -rf ./distr
mkdir -p "./$OUT_DIR"

# ── BUILD ────────────────────────────────────────────────────────────────────
# Docker containers build the Linux/Windows/Android packages; the dmg builds
# locally on macOS. All artifacts land in $OUT_DIR. (Server upload was removed —
# these scripts only BUILD now.)
if [[ "$OSTYPE" == "darwin"* ]]; then
    ./builder/build.sh dmg gtk-app
fi

docker compose -f ./builder/nodeinnet-builder.yml run --rm exe gtk-app
docker compose -f ./builder/nodeinnet-builder.yml run --rm zst gtk-app
docker compose -f ./builder/nodeinnet-builder.yml run --rm deb gtk-app
docker compose -f ./builder/nodeinnet-builder.yml run --rm rpm gtk-app

# The apk service mounts the GTK stack: GTK_STACK (a checkout) or the release gtk-stack.lock pins,
# fetched when it is not there. Only the apk needs it, so the other packages come first.
if [ -z "${GTK_STACK_HOST:-}" ]; then
    GTK_STACK_HOST="$(source ./src/gtk-app/android/gtk-stack.bash &&
        resolve_gtk_stack ./src/gtk-app/android/fetch-gtk-stack.sh ./gtk-stack.lock arm64-v8a && echo "$GTK_STACK")"
    export GTK_STACK_HOST
fi
docker compose -f ./builder/nodeinnet-builder.yml run --rm apk android-app

echo ""
echo "=== BUILD COMPLETED ==="
if [ -f "./$OUT_DIR/md5sums.txt" ]; then
    cat "./$OUT_DIR/md5sums.txt"
fi
