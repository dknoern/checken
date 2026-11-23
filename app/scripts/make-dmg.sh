#!/bin/bash
# Builds Checken and packages it as an installable disk image:
#   dist/Checken.dmg  — open it and drag Checken into Applications.
#
# Run from anywhere:   ./scripts/make-dmg.sh      (in the app folder)
# UNIVERSAL=1 ./scripts/make-dmg.sh  makes one app for both Apple silicon and
# Intel Macs (as the GitHub build does).
#
# Steps: build the release program, make Checken.app (program, Info.plist,
# the icon from assets/icon.png as an .icns), sign it for this Mac (ad hoc),
# then put the app and an Applications shortcut into a compressed .dmg.
# Uses only tools that come with macOS (sips, iconutil, codesign, hdiutil)
# and cargo.
set -euo pipefail

cd "$(dirname "$0")/.."
APP_DIR="$(pwd)"
export PATH="$HOME/.cargo/bin:$PATH"

NAME="Checken"
VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/version *= *"([^"]+)"/\1/')"
DIST="$APP_DIR/dist"
WORK="$DIST/work"
BUNDLE="$WORK/$NAME.app"

if [ "${UNIVERSAL:-0}" = "1" ]; then
  echo "==> Building $NAME $VERSION for Apple silicon and Intel (release)…"
  rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
  cargo build --release --target aarch64-apple-darwin
  cargo build --release --target x86_64-apple-darwin
  mkdir -p target/universal
  lipo -create -output "target/universal/$NAME" \
    "target/aarch64-apple-darwin/release/$NAME" "target/x86_64-apple-darwin/release/$NAME"
  BIN="target/universal/$NAME"
else
  echo "==> Building $NAME $VERSION (release)…"
  cargo build --release
  BIN="target/release/$NAME"
fi

echo "==> Making $NAME.app…"
rm -rf "$WORK"
mkdir -p "$BUNDLE/Contents/MacOS" "$BUNDLE/Contents/Resources"
cp "$BIN" "$BUNDLE/Contents/MacOS/$NAME"

# The icon: the app's own picture (with the macOS clean-up Checken applies),
# at every size macOS asks for.
ICONSET="$WORK/$NAME.iconset"
mkdir -p "$ICONSET"
"$BIN" --write-icon "$WORK/icon-1024.png"
for size in 16 32 128 256 512; do
  sips -z $size $size "$WORK/icon-1024.png" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z $double $double "$WORK/icon-1024.png" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$BUNDLE/Contents/Resources/$NAME.icns"

cat > "$BUNDLE/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key><string>$NAME</string>
	<key>CFBundleDisplayName</key><string>$NAME</string>
	<key>CFBundleExecutable</key><string>$NAME</string>
	<key>CFBundleIdentifier</key><string>app.checken.Checken</string>
	<key>CFBundleIconFile</key><string>$NAME</string>
	<key>CFBundlePackageType</key><string>APPL</string>
	<key>CFBundleShortVersionString</key><string>$VERSION</string>
	<key>CFBundleVersion</key><string>$VERSION</string>
	<key>LSApplicationCategoryType</key><string>public.app-category.finance</string>
	<key>LSMinimumSystemVersion</key><string>11.0</string>
	<key>NSHighResolutionCapable</key><true/>
	<key>NSPrincipalClass</key><string>NSApplication</string>
</dict>
</plist>
PLIST

echo "==> Signing (ad hoc, for this Mac)…"
codesign --force --deep --sign - "$BUNDLE"

echo "==> Making $NAME.dmg…"
STAGE="$WORK/dmg"
mkdir -p "$STAGE"
cp -R "$BUNDLE" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
DMG="$DIST/$NAME-$VERSION.dmg"
rm -f "$DMG"
hdiutil create -volname "$NAME" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$WORK"

echo
echo "Done: $DMG"
echo "Open it and drag $NAME into Applications."
