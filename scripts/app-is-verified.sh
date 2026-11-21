#!/usr/bin/env bash
# Exit 0 when a macOS app opens without a Gatekeeper warning, exit 1 otherwise.
#
# That takes two things, and a Developer ID signature alone is not enough:
#   1. a valid Developer ID Application signature (an ad-hoc one, which Tauri
#      adds on Apple Silicon, does not count), and
#   2. a stapled notarization ticket. A signed app that Apple never notarized
#      still gets the "cannot check it for malicious software" block.
#
# The DMG scripts use this to decide whether to ship
# UNVERIFIED_APP_INSTRUCTIONS.txt: it is only for apps that fail this check.
#
# Usage: scripts/app-is-verified.sh path/to/r2modmac.app

set -u

app="${1:-}"
if [[ -z "$app" || ! -d "$app" ]]; then
  echo "usage: $0 path/to/App.app" >&2
  exit 2
fi

# codesign writes its report to stderr.
details="$(codesign --display --verbose=4 "$app" 2>&1)" || exit 1

grep -q '^Authority=Developer ID Application' <<<"$details" || exit 1
codesign --verify --deep --strict "$app" >/dev/null 2>&1 || exit 1
xcrun stapler validate "$app" >/dev/null 2>&1 || exit 1
