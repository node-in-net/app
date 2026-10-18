#!/usr/bin/env bash
# Runs the GTK client with the phone layout (NODEINNET_MOBILE=1) on GTK's Broadway backend, in a
# phone-sized window: open http://localhost:$((8080 + N)).
#
#   src/android-app/preview.sh [mock|live] [N]
#
# mock (default): sample devices and an accept-anything login, no account needed.
set -euo pipefail
MODE="${1:-mock}"
N="${2:-6}"
APP="$(cd "$(dirname "$0")/../.." && pwd)"
STATE="$APP/bin/android-preview"
cd "$APP"
cargo build -q -p nodeinnet-gtk
gtk4-broadwayd ":$N" &
BROADWAYD=$!
trap 'kill $BROADWAYD 2>/dev/null' EXIT
sleep 0.5
echo "Phone preview: http://localhost:$((8080 + N))"
# Animations off: a background browser tab stops acking frames.
mkdir -p "$STATE/config/gtk-4.0" "$STATE/profile"
printf '[Settings]\ngtk-enable-animations=0\n' > "$STATE/config/gtk-4.0/settings.ini"
[ "$MODE" = mock ] && export NODEINNET_MOCK=1
XDG_CONFIG_HOME="$STATE/config" NODEINNET_PROFILE="$STATE/profile" NODEINNET_MOBILE=1 \
	GDK_BACKEND=broadway BROADWAY_DISPLAY=":$N" \
	dbus-run-session -- "$APP/bin/target/debug/nodeinnet-gtk"
