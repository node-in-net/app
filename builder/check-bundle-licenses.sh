#!/bin/bash
set -u

# Scans the Windows bundle for GPL code before it is packed into the installer.
#
# Usage: ./builder/check-bundle-licenses.sh [bundle-dir] [--strict]
#            default bundle-dir: artifacts/gtk4-win32-x64
#            --strict: exit non-zero if anything is found
#
# This exists because "no GPL ships" was asserted three times in this project's history and
# was wrong twice. An assertion in a document rots; a check runs. It catches the two things
# that actually happened:
#
#   1. A GPL library restored into the bundle from MSYS2 (liblzo2, libjbig).
#   2. A GPL component compiled INSIDE an otherwise-LGPL library. Nothing currently matches:
#      the one hit, gdk-pixbuf's ICNS loader, declares "GPL" in a metadata field while its
#      own source header is LGPL-2.0-or-later, so it is listed as KNOWN rather than counted.
#      See assets/licenses/BUNDLED-COMPONENTS.txt for the two upstream line references.
#
# It is a tripwire, not a license audit: it finds declarations, not silence. A GPL library
# that says nothing about itself stays invisible to it. The reasoned inventory lives in
# assets/licenses/BUNDLED-COMPONENTS.txt.

BUNDLE=""
STRICT=0
for a in "$@"; do
    case "$a" in
        --strict) STRICT=1 ;;
        *) [ -z "$BUNDLE" ] && BUNDLE="$a" ;;
    esac
done
[ -n "$BUNDLE" ] || BUNDLE="$(cd "$(dirname "$0")/.." && pwd)/artifacts/gtk4-win32-x64"

[ -d "$BUNDLE" ] || { echo "no such bundle directory: $BUNDLE" >&2; exit 1; }

findings=0

# 1. GPL libraries that must never be packed. Wildcards, so a soname bump is still caught.
for pat in 'liblzo2*.dll' 'libjbig*.dll'; do
    for f in "$BUNDLE"/$pat; do
        [ -e "$f" ] || continue
        echo "FINDING  GPL library present in the bundle: $(basename "$f")"
        echo "         setup.nsi excludes it from the archive, but it should not be here."
        findings=$((findings + 1))
    done
done

# 2. A bare "GPL" declaration compiled into a library.
#
# Two traps this has to avoid. First, `strings` cannot find it: the default four-character
# minimum hides a three-letter "GPL", which is why this went unnoticed for a while — hence
# the raw byte scan. Second, several libraries carry a TABLE of licence names (GStreamer's
# list of valid plugin licences, gst-tag's list for media metadata). Those are vocabulary,
# not a declaration, so a hit surrounded by other licence names is reported as such rather
# than counted.
report=$(python3 - "$BUNDLE" <<'PY'
import glob, os, sys

KNOWN = {b"LGPL", b"GPL", b"QPL", b"GPL/QPL", b"MPL", b"MPL-2.0", b"BSD", b"MIT/X11",
         b"0BSD", b"Apache 2.0", b"Proprietary", b"unknown", b"Attribution",
         b"Attribution-NonCommercial", b"Attribution-ShareAlike"}

for path in sorted(glob.glob(os.path.join(sys.argv[1], "*.dll"))):
    data = open(path, "rb").read()
    pos = data.find(b"\x00GPL\x00")
    while pos != -1:
        window = data[max(0, pos - 120): pos + 120]
        neighbours = {s for s in window.split(b"\x00") if s and s in KNOWN}
        # A licence vocabulary lists several names together; a declaration stands alone.
        kind = "TABLE" if len(neighbours) >= 3 else "DECLARATION"
        # gdk-pixbuf's ICNS record: the string contradicts the file's own LGPL header.
        if kind == "DECLARATION" and b"icns" in window:
            kind = "KNOWN"
        # The record around a real declaration names the thing being declared.
        ctx = [s.decode("utf-8", "replace") for s in data[pos - 60:pos + 40].split(b"\x00")
               if 3 < len(s) < 40]
        print(f"{kind}\t{os.path.basename(path)}\t{' | '.join(ctx[:4])}")
        pos = data.find(b"\x00GPL\x00", pos + 1)
PY
)

while IFS=$'\t' read -r kind lib ctx; do
    [ -n "${kind:-}" ] || continue
    if [ "$kind" = "DECLARATION" ]; then
        echo "FINDING  $lib declares GPL for a component compiled into it"
        echo "         context: $ctx"
        findings=$((findings + 1))
    elif [ "$kind" = "KNOWN" ]; then
        echo "known    $lib: gdk-pixbuf ICNS metadata says GPL, its source header says LGPL-2.0+"
        echo "         ($ctx) — see assets/licenses/BUNDLED-COMPONENTS.txt"
    else
        echo "ok       $lib carries a licence-name table, not a declaration ($ctx)"
    fi
done <<< "$report"

count=$(ls "$BUNDLE"/*.dll 2>/dev/null | wc -l)
echo
if [ "$findings" -eq 0 ]; then
    echo "bundle check: $count libraries, no GPL declarations"
    exit 0
fi

echo "bundle check: $count libraries, $findings finding(s) — the installer carries GPL code."
[ "$STRICT" -eq 1 ] && exit 1
exit 0
