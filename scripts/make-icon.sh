#!/bin/zsh
# Regenerate assets/AppIcon.icns from scripts/make-icon.swift.
set -euo pipefail
REPO=${0:a:h:h}
TMP=$(mktemp -d)
swift "$REPO/scripts/make-icon.swift" "$TMP/icon.png"
mkdir "$TMP/AppIcon.iconset"
for s in 16 32 128 256 512; do
  sips -z $s $s "$TMP/icon.png" --out "$TMP/AppIcon.iconset/icon_${s}x${s}.png" >/dev/null
  sips -z $((s * 2)) $((s * 2)) "$TMP/icon.png" --out "$TMP/AppIcon.iconset/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$TMP/AppIcon.iconset" -o "$REPO/assets/AppIcon.icns"
rm -rf "$TMP"
echo "Wrote assets/AppIcon.icns"
