#!/bin/bash
# Puts Checken's website together with the latest installers, ready for
# GitHub Pages (the Build workflow runs this on every push to main).
#
#   ./app/site/assemble.sh <folder with the built installers> <output folder>
#
# The installers get fixed names, so the site's Download buttons always point
# at the newest build:
#   downloads/Checken-mac.dmg               (from Checken-<version>.dmg)
#   downloads/Checken-Setup-windows.exe     (from Checken-Setup-<version>.exe)
#   downloads/checken-linux-amd64.deb       (from checken_<version>_amd64.deb)
#   downloads/Checken-linux-x86_64.tar.gz   (from Checken-<version>-linux-x86_64.tar.gz)
# and downloads.json records the version, build time, commit, sizes and
# SHA-256 of each, which the page shows.
#
# Try it locally: ./app/site/assemble.sh app/dist /tmp/checken-site
# then open /tmp/checken-site/index.html.
set -euo pipefail

FROM="${1:?folder with the built installers}"
OUT="${2:?output folder}"
SITE="$(cd "$(dirname "$0")" && pwd)"
APP="$(dirname "$SITE")"

if [ -e "$OUT" ] && [ -n "$(ls -A "$OUT" 2>/dev/null)" ]; then
  echo "The output folder $OUT isn't empty; give a new or empty one." >&2
  exit 1
fi
mkdir -p "$OUT/downloads"

# The site's own files (not this script).
for f in "$SITE"/*; do
  case "$(basename "$f")" in assemble.sh) ;; *) cp -R "$f" "$OUT/" ;; esac
done
touch "$OUT/.nojekyll"

VERSION="$(grep -m1 '^version' "$APP/Cargo.toml" | sed -E 's/version *= *"([^"]+)"/\1/')"
COMMIT="${GITHUB_SHA:-$(git -C "$APP" rev-parse HEAD 2>/dev/null || echo "")}"
BUILT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

sha() { if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }
size() { wc -c < "$1" | tr -d ' '; }

ENTRIES=()
place() { # key, pattern, fixed name
  local key="$1" pattern="$2" name="$3" src
  src="$(find "$FROM" -type f -name "$pattern" | sort | tail -n 1)"
  if [ -z "$src" ]; then
    echo "  (no $pattern in $FROM: the $key download is left out)"
    return
  fi
  cp "$src" "$OUT/downloads/$name"
  ENTRIES+=("\"$key\": {\"file\": \"downloads/$name\", \"size\": $(size "$src"), \"sha256\": \"$(sha "$src")\"}")
  echo "  $key: $(basename "$src") → downloads/$name"
}

echo "==> Checken $VERSION website → $OUT"
place mac     '*.dmg'                    Checken-mac.dmg
place windows 'Checken-Setup-*.exe'      Checken-Setup-windows.exe
place deb     'checken_*_amd64.deb'      checken-linux-amd64.deb
place tar     '*-linux-x86_64.tar.gz'    Checken-linux-x86_64.tar.gz

{
  printf '{\n  "version": "%s",\n  "built": "%s",\n  "commit": "%s",\n  "files": {' "$VERSION" "$BUILT" "$COMMIT"
  sep=""
  for e in "${ENTRIES[@]+"${ENTRIES[@]}"}"; do printf '%s\n    %s' "$sep" "$e"; sep=","; done
  printf '\n  }\n}\n'
} > "$OUT/downloads.json"

echo "==> Done: $(find "$OUT" -type f | wc -l | tr -d ' ') files."
