#!/bin/zsh
# Build Rink Replay and install it as a Mac app, with `rink` on your PATH.
#
#   scripts/install-mac.sh               # ~/Applications/Rink Replay.app, rink in ~/.local/bin
#   APP_DIR=/Applications BIN_DIR=/usr/local/bin scripts/install-mac.sh
#
# Run it again after pulling or changing the code. Quit the app first.
set -euo pipefail

REPO=${0:a:h:h}
APP_DIR=${APP_DIR:-$HOME/Applications}
BIN_DIR=${BIN_DIR:-$HOME/.local/bin}
APP="$APP_DIR/Rink Replay.app"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO/Cargo.toml" | head -1)

cd "$REPO"
cargo build --release -p rink-app -p rink-cli

if pgrep -qf "$APP/Contents/MacOS/rink-replay"; then
  echo "Rink Replay is running; quit it (Cmd-Q) and run this again." >&2
  exit 1
fi

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$BIN_DIR"
# `rink open` starts the rink-replay next to it, so both go in the bundle.
cp target/release/rink-replay target/release/rink "$APP/Contents/MacOS/"
cp assets/AppIcon.icns "$APP/Contents/Resources/"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Rink Replay</string>
  <key>CFBundleDisplayName</key><string>Rink Replay</string>
  <key>CFBundleIdentifier</key><string>net.vilbergs.rink-replay</string>
  <key>CFBundleExecutable</key><string>rink-replay</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
# Ad-hoc signature: enough to run on this Mac.
codesign --force --deep --sign - "$APP" >/dev/null 2>&1
ln -sf "$APP/Contents/MacOS/rink" "$BIN_DIR/rink"
# Tell Finder and the Dock about the new icon.
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$APP" >/dev/null 2>&1 || true

echo "Installed $APP"
echo "Linked $BIN_DIR/rink"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) echo "Add $BIN_DIR to your PATH to run rink from anywhere." ;;
esac
