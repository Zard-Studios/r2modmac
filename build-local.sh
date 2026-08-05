#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
CORE_SCRIPT="$ROOT_DIR/.build-local-core.sh"
REQUESTED_VERSION=""
NORMALIZED_ARGS=()
MODE_SEEN=""

usage() {
  cat <<'USAGE'
Usage:
  ./build-local.sh [VERSION] [--all|--macos|--linux|--linux-x64|--linux-arm64] [--skip-checks] [--clean]
  ./build-local.sh [VERSION] [all|macos|linux|linux-x64|linux-arm64] [--skip-checks] [--clean]

Examples:
  ./build-local.sh 0.8.3 --all
  ./build-local.sh --all
  ./build-local.sh linux --clean

VERSION is optional. When supplied, it must match package.json, tauri.conf.json, and Cargo.toml.
USAGE
}

die() {
  printf '\033[1;31merror: %s\033[0m\n' "$*" >&2
  exit 1
}

set_mode() {
  local mode="$1"
  if [[ -n "$MODE_SEEN" && "$MODE_SEEN" != "$mode" ]]; then
    die "Choose only one build mode (received '$MODE_SEEN' and '$mode')."
  fi
  MODE_SEEN="$mode"
}

for arg in "$@"; do
  case "$arg" in
    --all) set_mode all ;;
    --macos) set_mode macos ;;
    --linux) set_mode linux ;;
    --linux-x64) set_mode linux-x64 ;;
    --linux-arm64) set_mode linux-arm64 ;;
    all|macos|linux|linux-x64|linux-arm64) set_mode "$arg" ;;
    --skip-checks|--clean) NORMALIZED_ARGS+=("$arg") ;;
    -h|--help) usage; exit 0 ;;
    v[0-9]*.[0-9]*.[0-9]*|[0-9]*.[0-9]*.[0-9]*)
      [[ -z "$REQUESTED_VERSION" ]] || die "Version was specified more than once."
      REQUESTED_VERSION="${arg#v}"
      ;;
    *) usage >&2; die "Unknown argument: $arg" ;;
  esac
done

[[ -x "$CORE_SCRIPT" ]] || die "Missing executable build helper: $CORE_SCRIPT"

if [[ -n "$REQUESTED_VERSION" ]]; then
  command -v node >/dev/null 2>&1 || die "Required command not found: node"
  ACTUAL_VERSION="$(cd "$ROOT_DIR" && node - <<'NODE'
const fs = require('fs');
const packageVersion = JSON.parse(fs.readFileSync('package.json', 'utf8')).version;
const tauriVersion = JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json', 'utf8')).version;
const cargo = fs.readFileSync('src-tauri/Cargo.toml', 'utf8');
const cargoVersion = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if (!packageVersion || packageVersion !== tauriVersion || packageVersion !== cargoVersion) {
  console.error(`Version mismatch: package=${packageVersion}, tauri=${tauriVersion}, cargo=${cargoVersion}`);
  process.exit(1);
}
process.stdout.write(packageVersion);
NODE
)"
  [[ "$ACTUAL_VERSION" == "$REQUESTED_VERSION" ]] \
    || die "Requested version $REQUESTED_VERSION does not match repository version $ACTUAL_VERSION."
fi

if [[ -n "$MODE_SEEN" ]]; then
  NORMALIZED_ARGS=("$MODE_SEEN" "${NORMALIZED_ARGS[@]}")
fi

exec "$CORE_SCRIPT" "${NORMALIZED_ARGS[@]}"
