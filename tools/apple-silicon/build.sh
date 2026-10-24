#!/usr/bin/env bash
# Builds everything the native Apple Silicon setup puts beside a game, into tools/apple-silicon/dist:
#   libjitwrite.dylib       arm64, ad-hoc signed (from jitwrite.c)
#   AppleSiliconMods.dll    Doorstop's entry point (from AppleSiliconMods/)
#   AppleSiliconMods.Straftat.dll   STRAFTAT matchmaking marker (from StraftatMarker/)
#   libdoorstop.dylib       UnityDoorstop 4.5.0, universal, pinned by SHA-256 (LGPL-2.1: LICENSE-UnityDoorstop.txt)
# Needs Xcode's command line tools and the .NET SDK. r2modmac's macOS release build bundles dist/ into the app
# (src-tauri/tauri.apple-silicon.conf.json); install.sh puts it beside one game by hand.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="$HERE/dist"
DOORSTOP_VERSION=4.5.0
DOORSTOP_ZIP_SHA256=f53e0906b60fdcaab1ed0259420863d0e9183285d2358e2edd9939a47f8c61e8
DOORSTOP_LICENSE_SHA256=20c17d8b8c48a600800dfd14f95d5cb9ff47066a9641ddeab48dc54aec96e331

rm -rf "$OUT" && mkdir -p "$OUT"
clang -arch arm64 -dynamiclib -O2 -Wall -o "$OUT/libjitwrite.dylib" "$HERE/jitwrite.c"
codesign --force --sign - "$OUT/libjitwrite.dylib"
dotnet build "$HERE/AppleSiliconMods" -c Release -o "$HERE/AppleSiliconMods/bin/build" --nologo -v quiet
cp "$HERE/AppleSiliconMods/bin/build/AppleSiliconMods.dll" "$OUT/"
dotnet build "$HERE/StraftatMarker" -c Release -o "$HERE/StraftatMarker/bin/build" --nologo -v quiet
cp "$HERE/StraftatMarker/bin/build/AppleSiliconMods.Straftat.dll" "$OUT/"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
curl -fsSL -o "$tmp/doorstop.zip" "https://github.com/NeighTools/UnityDoorstop/releases/download/v$DOORSTOP_VERSION/doorstop_macos_release_$DOORSTOP_VERSION.zip"
echo "$DOORSTOP_ZIP_SHA256  $tmp/doorstop.zip" | shasum -a 256 -c --quiet
curl -fsSL -o "$tmp/LICENSE" "https://raw.githubusercontent.com/NeighTools/UnityDoorstop/v$DOORSTOP_VERSION/LICENSE"
echo "$DOORSTOP_LICENSE_SHA256  $tmp/LICENSE" | shasum -a 256 -c --quiet
unzip -q "$tmp/doorstop.zip" -d "$tmp/doorstop"
lipo -archs "$tmp/doorstop/universal/libdoorstop.dylib" | grep -q arm64
cp "$tmp/doorstop/universal/libdoorstop.dylib" "$OUT/libdoorstop.dylib"
cp "$tmp/LICENSE" "$OUT/LICENSE-UnityDoorstop.txt"
ls -l "$OUT"
