#!/bin/bash
# Builds Checken for Windows and Linux and makes their installers, using
# Docker (a throwaway Debian container with Rust), so nothing is installed on
# this Mac:
#   dist/Checken-Setup-<version>.exe       Windows installer (Start menu, desktop shortcut, uninstaller)
#   dist/checken_<version>_amd64.deb       Linux package for Debian / Ubuntu (sudo apt install ./checken_….deb)
#   dist/Checken-<version>-linux-x86_64.tar.gz   Linux program to unpack and run anywhere
#
# Run from the app folder:   ./scripts/make-windows-linux.sh
# Docker must be running (with Colima: `colima start`). It builds one piece at a
# time (CARGO_BUILD_JOBS=1), which fits in a small Docker machine (2 GB); set
# CARGO_BUILD_JOBS higher on a bigger machine (the GitHub build uses 4).
set -euo pipefail

cd "$(dirname "$0")/.."
APP_DIR="$(pwd)"
mkdir -p dist

if ! docker info >/dev/null 2>&1; then
  echo "Docker isn't running. Start it (for Colima: colima start) and try again." >&2
  exit 1
fi

# Reuse this Mac's downloaded crates so the container doesn't fetch them again.
REGISTRY="${CARGO_HOME:-$HOME/.cargo}/registry"
mkdir -p "$REGISTRY"

docker run --rm \
  -v "$APP_DIR":/src \
  -v "$REGISTRY":/usr/local/cargo/registry \
  -e CARGO_TARGET_DIR=/src/target/cross \
  -e CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}" \
  -w /src \
  rust:1-bookworm \
  bash /src/scripts/build-in-container.sh

echo
echo "Done:"
ls -1 dist/ | grep -E "Setup|\.deb|linux" | sed 's/^/  dist\//'
