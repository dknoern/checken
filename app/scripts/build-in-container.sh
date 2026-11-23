#!/bin/bash
# Runs inside the Debian + Rust container started by make-windows-linux.sh.
# Builds the Linux and Windows programs and packages them into dist/.
set -euo pipefail
cd /src

VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/version *= *"([^"]+)"/\1/')"
OUT=/src/dist
T=/src/target/cross
mkdir -p "$OUT"

echo "==> Installing build tools in the container…"
apt-get update -qq
apt-get install -y -qq --no-install-recommends \
  gcc-mingw-w64-x86-64 nsis icoutils imagemagick \
  pkg-config libx11-dev libxkbcommon-dev libwayland-dev libgl1-mesa-dev \
  xvfb xauth libgl1-mesa-dri >/dev/null

# ------------------------------------------------------------------ Linux --
echo "==> Building for Linux…"
cargo build --release --target x86_64-unknown-linux-gnu
LINUX_BIN="$T/x86_64-unknown-linux-gnu/release/Checken"

# The icon picture, from the program itself (assets/icon.png, cleaned up).
WORK=$(mktemp -d)
"$LINUX_BIN" --write-icon "$WORK/icon.png"

# .deb package: the program, a menu entry and the icon.
PKG="$WORK/deb"
mkdir -p "$PKG/DEBIAN" "$PKG/usr/bin" "$PKG/usr/share/applications" "$PKG/usr/share/icons/hicolor/256x256/apps" "$PKG/usr/share/icons/hicolor/512x512/apps"
install -m 755 "$LINUX_BIN" "$PKG/usr/bin/checken"
convert "$WORK/icon.png" -resize 256x256 "$PKG/usr/share/icons/hicolor/256x256/apps/checken.png"
convert "$WORK/icon.png" -resize 512x512 "$PKG/usr/share/icons/hicolor/512x512/apps/checken.png"
cat > "$PKG/usr/share/applications/checken.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Checken
Comment=Personal finance: accounts, transactions, budgets and reports
Exec=checken
Icon=checken
Categories=Office;Finance;
Terminal=false
DESKTOP
cat > "$PKG/DEBIAN/control" <<CONTROL
Package: checken
Version: $VERSION
Section: office
Priority: optional
Architecture: amd64
Depends: libc6, libgl1, libxkbcommon0, curl
Maintainer: Checken
Description: Checken personal finance
 Keep accounts, enter transactions, plan budgets and read reports.
CONTROL
dpkg-deb --build --root-owner-group "$PKG" "$OUT/checken_${VERSION}_amd64.deb" >/dev/null

# Plain archive for other Linux systems.
TAR="$WORK/Checken-$VERSION"
mkdir -p "$TAR"
install -m 755 "$LINUX_BIN" "$TAR/checken"
cp "$PKG/usr/share/applications/checken.desktop" "$TAR/"
cp "$PKG/usr/share/icons/hicolor/256x256/apps/checken.png" "$TAR/"
printf 'Run ./checken to start Checken.\n' > "$TAR/README.txt"
tar -C "$WORK" -czf "$OUT/Checken-$VERSION-linux-x86_64.tar.gz" "Checken-$VERSION"

# A quick look that the Linux program starts and draws (on a virtual screen).
if xvfb-run -a -s "-screen 0 1280x800x24" env LIBGL_ALWAYS_SOFTWARE=1 HOME="$WORK/home" \
     "$LINUX_BIN" --screenshot "$OUT/linux-check.png" >/dev/null 2>&1; then
  echo "    (the Linux program started and drew its window)"
else
  echo "    (couldn't run the Linux program on a virtual screen here; the package is still made)"
fi

# ---------------------------------------------------------------- Windows --
echo "==> Building for Windows…"
rustup target add x86_64-pc-windows-gnu >/dev/null 2>&1
# The program's icon, built in through a Windows resource (see build.rs).
mkdir -p /src/target/cross/windows
for s in 16 32 48 64 128 256; do convert "$WORK/icon.png" -resize ${s}x${s} "$WORK/icon-$s.png"; done
icotool -c -o /src/target/cross/windows/checken.ico "$WORK"/icon-{16,32,48,64,128,256}.png
export CHECKEN_WINDOWS_ICON=/src/target/cross/windows/checken.ico
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
cargo build --release --target x86_64-pc-windows-gnu
WIN_EXE="$T/x86_64-pc-windows-gnu/release/Checken.exe"

# The installer, made with NSIS.
NSI="$WORK/checken.nsi"
cp "$WIN_EXE" "$WORK/Checken.exe"
cp /src/target/cross/windows/checken.ico "$WORK/checken.ico"
cat > "$NSI" <<NSIS
Unicode true
Name "Checken"
OutFile "$OUT/Checken-Setup-$VERSION.exe"
InstallDir "\$PROGRAMFILES64\\Checken"
InstallDirRegKey HKLM "Software\\Checken" "InstallDir"
RequestExecutionLevel admin
Icon "$WORK/checken.ico"
UninstallIcon "$WORK/checken.ico"
SetCompressor /SOLID lzma

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "Checken"
  SetOutPath "\$INSTDIR"
  File "$WORK/Checken.exe"
  File "$WORK/checken.ico"
  WriteRegStr HKLM "Software\\Checken" "InstallDir" "\$INSTDIR"
  WriteUninstaller "\$INSTDIR\\Uninstall Checken.exe"
  CreateDirectory "\$SMPROGRAMS\\Checken"
  CreateShortcut "\$SMPROGRAMS\\Checken\\Checken.lnk" "\$INSTDIR\\Checken.exe" "" "\$INSTDIR\\checken.ico"
  CreateShortcut "\$SMPROGRAMS\\Checken\\Uninstall Checken.lnk" "\$INSTDIR\\Uninstall Checken.exe"
  CreateShortcut "\$DESKTOP\\Checken.lnk" "\$INSTDIR\\Checken.exe" "" "\$INSTDIR\\checken.ico"
  WriteRegStr HKLM "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Checken" "DisplayName" "Checken"
  WriteRegStr HKLM "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Checken" "DisplayVersion" "$VERSION"
  WriteRegStr HKLM "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Checken" "DisplayIcon" "\$INSTDIR\\checken.ico"
  WriteRegStr HKLM "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Checken" "UninstallString" '"\$INSTDIR\\Uninstall Checken.exe"'
SectionEnd

Section "Uninstall"
  Delete "\$INSTDIR\\Checken.exe"
  Delete "\$INSTDIR\\checken.ico"
  Delete "\$INSTDIR\\Uninstall Checken.exe"
  RMDir "\$INSTDIR"
  Delete "\$SMPROGRAMS\\Checken\\Checken.lnk"
  Delete "\$SMPROGRAMS\\Checken\\Uninstall Checken.lnk"
  RMDir "\$SMPROGRAMS\\Checken"
  Delete "\$DESKTOP\\Checken.lnk"
  DeleteRegKey HKLM "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Checken"
  DeleteRegKey HKLM "Software\\Checken"
SectionEnd
NSIS
makensis -V2 "$NSI"

rm -rf "$WORK"
echo "==> Packages are in dist/."
