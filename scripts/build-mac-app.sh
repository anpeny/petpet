#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
APP_NAME="Q JK 桌宠"
APP_DIR="$ROOT_DIR/dist/mac/$APP_NAME.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
BINARY="$ROOT_DIR/rust/native-pet/target/release/native-pet"

cd "$ROOT_DIR"
cargo build --release --manifest-path rust/native-pet/Cargo.toml

rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR/assets/icons"
cp "$BINARY" "$MACOS_DIR/native-pet"
cp -R "$ROOT_DIR/assets/frames" "$RESOURCES_DIR/assets/frames"
cp "$ROOT_DIR/assets/icons/tray-32.png" "$RESOURCES_DIR/assets/icons/tray-32.png"
cp "$ROOT_DIR/assets/icons/AppIcon.icns" "$RESOURCES_DIR/AppIcon.icns"

cat > "$CONTENTS_DIR/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>zh_CN</string>
  <key>CFBundleExecutable</key>
  <string>native-pet</string>
  <key>CFBundleIdentifier</key>
  <string>local.q-jk-desktop-pet</string>
  <key>CFBundleName</key>
  <string>Q JK 桌宠</string>
  <key>CFBundleDisplayName</key>
  <string>Q JK 桌宠</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.2</string>
  <key>CFBundleVersion</key>
  <string>3</string>
  <key>LSUIElement</key>
  <true/>
  <key>NSAppleEventsUsageDescription</key>
  <string>用于桌宠托盘和系统交互。</string>
</dict>
</plist>
PLIST

if command -v codesign >/dev/null 2>&1; then
  codesign --force --deep --sign - "$APP_DIR" >/dev/null 2>&1 || true
fi

du -sh "$APP_DIR"
echo "$APP_DIR"
