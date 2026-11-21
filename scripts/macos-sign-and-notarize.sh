#!/usr/bin/env bash
# Signs a built r2modmac.app with a Developer ID certificate, then notarizes it.
#
# Tauri signs the main executable on its own, but not the loose binaries the
# app carries in Contents/Resources (the Apple Silicon add-on dylibs). Apple
# rejects a notarization when any binary in the bundle lacks a Developer ID
# signature, the hardened runtime or a secure timestamp. So the build runs
# without notarization credentials and this script does the rest:
#   1. signs every Mach-O file inside the app,
#   2. seals the app again,
#   3. submits it to Apple and staples the ticket, if credentials are given.
#
# Everything comes from the environment, so no secret is ever a command-line
# argument or gets written to a log:
#   APPLE_SIGNING_IDENTITY   certificate name in the keychain. Unset: the script
#                            does nothing and the app stays unsigned.
#   APPLE_API_KEY_PATH       path to the App Store Connect API key (.p8)
#   APPLE_API_KEY            that key's ID
#   APPLE_API_ISSUER         the issuer ID
# Without the three API variables the app is signed but not notarized.
#
# Usage: scripts/macos-sign-and-notarize.sh path/to/r2modmac.app

set -euo pipefail
# Never trace this script: it handles credentials.
set +x

app="${1:-}"
if [[ -z "$app" || ! -d "$app" ]]; then
  echo "usage: $0 path/to/App.app" >&2
  exit 2
fi

if [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  echo "No signing identity set: leaving the app unsigned."
  exit 0
fi

sign() {
  codesign --force --options runtime --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$1"
}

echo "Signing the binaries inside the app"
while IFS= read -r -d '' file; do
  if file -b "$file" | grep -q 'Mach-O'; then
    sign "$file" >/dev/null 2>&1
    echo "  signed ${file#"$app"/}"
  fi
done < <(find "$app/Contents" -type f -print0)

echo "Sealing the app"
sign "$app" >/dev/null 2>&1
codesign --verify --deep --strict "$app"

if [[ -z "${APPLE_API_KEY_PATH:-}" || -z "${APPLE_API_KEY:-}" || -z "${APPLE_API_ISSUER:-}" ]]; then
  echo "No notarization credentials set: the app is signed but not notarized."
  exit 0
fi
if [[ ! -f "$APPLE_API_KEY_PATH" ]]; then
  echo "The API key file does not exist." >&2
  exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
ditto -c -k --keepParent "$app" "$work/app.zip"

notarytool() {
  xcrun notarytool "$@" \
    --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER"
}

echo "Submitting to Apple for notarization (this can take a few minutes)"
result="$(notarytool submit "$work/app.zip" --wait --timeout 30m --output-format json)"
status="$(plutil -extract status raw -o - - <<<"$result")"
id="$(plutil -extract id raw -o - - <<<"$result")"
echo "Notarization $id: $status"

if [[ "$status" != "Accepted" ]]; then
  echo "Apple rejected the app. Its log:" >&2
  notarytool log "$id" >&2 || true
  exit 1
fi

xcrun stapler staple "$app" >/dev/null
xcrun stapler validate "$app" >/dev/null
echo "Notarized and stapled."
