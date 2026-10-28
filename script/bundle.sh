#!/bin/sh
# Builds a release Burrdisk.app and packs it as a disk image and a zip.
#
# The app is signed ad hoc, not with a Developer ID, so on first launch macOS
# asks to confirm opening it (right-click > Open, or System Settings > Privacy).
#
#   script/bundle.sh    writes target/release/Burrdisk.app and dist/Burrdisk-<version>-<arch>.{dmg,zip}
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
ARCH=$(uname -m)
cargo build --release

APP="target/release/Burrdisk.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/burrdisk "$APP/Contents/MacOS/burrdisk"

# App icon, built by script/icons/build.sh
cp assets/macos/Burrdisk.icns "$APP/Contents/Resources/Burrdisk.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>burrdisk</string>
    <key>CFBundleIdentifier</key><string>dev.burrdisk.Burrdisk</string>
    <key>CFBundleName</key><string>Burrdisk</string>
    <key>CFBundleDisplayName</key><string>Burrdisk</string>
    <key>CFBundleIconFile</key><string>Burrdisk</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
codesign --force --sign - --identifier dev.burrdisk.Burrdisk "$APP"

mkdir -p dist
ZIP="dist/Burrdisk-$VERSION-$ARCH.zip"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"
echo "$ZIP"

# The disk image opens to the app beside an Applications shortcut, to drag across
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
DMG="dist/Burrdisk-$VERSION-$ARCH.dmg"
rm -f "$DMG"
hdiutil create -volname "Burrdisk" -srcfolder "$STAGE" -fs HFS+ -format UDZO -ov "$DMG" >/dev/null
echo "$DMG"
