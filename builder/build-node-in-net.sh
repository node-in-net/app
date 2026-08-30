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
docker compose -f ./builder/nodeinnet-builder.yml run --rm apk android-app

echo ""
echo "=== BUILD COMPLETED ==="
if [ -f "./$OUT_DIR/md5sums.txt" ]; then
    cat "./$OUT_DIR/md5sums.txt"
fi
