#!/usr/bin/env bash
# BepInEx 5 only. Lets a BepInEx 5 (Unity Mono) game launched from r2modmac run natively on Apple Silicon instead of
# under Rosetta, by hand. r2modmac release builds do this by themselves at launch; this is for builds without it.
# Puts beside the game, from build.sh's dist/:
#   libdoorstop.dylib                    UnityDoorstop 4.5.0, universal (packs often ship only an x86_64 loader)
#   apple_silicon/AppleSiliconMods.dll   apple_silicon/libjitwrite.dylib   apple_silicon/AppleSiliconMods.Straftat.dll
# and clears r2modmac's "use Rosetta" note for the game, so the next launch tries native again.
# Usage: install.sh <game folder>      install.sh --remove <game folder>   (back to Rosetta)
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
remove=false
if [[ "${1:-}" == "--remove" ]]; then remove=true; shift; fi
GAME="${1:?Usage: install.sh [--remove] <game folder>}"
[[ -d "$GAME" ]] || { echo "No folder $GAME" >&2; exit 1; }

if $remove; then
  rm -rf "$GAME/apple_silicon" "$GAME/libdoorstop.dylib" "$GAME/.r2modmac_force_x64_state"
  echo "Removed: the game's mods run under Rosetta again."
  exit 0
fi

[[ "$(uname -m)" == "arm64" ]] || { echo "This Mac isn't Apple Silicon" >&2; exit 1; }
[[ -f "$HERE/dist/AppleSiliconMods.dll" && -f "$HERE/dist/libjitwrite.dylib" && -f "$HERE/dist/AppleSiliconMods.Straftat.dll" && -f "$HERE/dist/libdoorstop.dylib" ]] || { echo "Run build.sh first" >&2; exit 1; }
[[ -f "$GAME/BepInEx/core/MonoMod.RuntimeDetour.dll" ]] || echo "Note: no BepInEx/core/MonoMod.RuntimeDetour.dll in $GAME yet (BepInEx 5 is needed; r2modmac installs it with the profile)." >&2
app="$(find "$GAME" -maxdepth 1 -name '*.app' -print -quit)"
if [[ -n "$app" ]]; then
  exe="$app/Contents/MacOS/$(defaults read "$app/Contents/Info" CFBundleExecutable)"
  lipo -archs "$exe" | grep -q arm64 || { echo "$(basename "$app") has no arm64 build: it can only run under Rosetta" >&2; exit 1; }
fi

cp "$HERE/dist/libdoorstop.dylib" "$GAME/libdoorstop.dylib"
mkdir -p "$GAME/apple_silicon"
cp "$HERE/dist/AppleSiliconMods.dll" "$HERE/dist/libjitwrite.dylib" "$HERE/dist/AppleSiliconMods.Straftat.dll" "$GAME/apple_silicon/"
rm -f "$GAME/.r2modmac_force_x64_state"
echo "Installed. Launch the game with mods from r2modmac; apple_silicon/apple_silicon.log says whether it ran natively."
