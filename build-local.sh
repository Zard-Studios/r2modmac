#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'
umask 022

# Always use rustup's cargo/rustc, NOT Homebrew's. Homebrew rust lacks cross-compile targets.
export PATH="$HOME/.cargo/bin:$PATH"

# Use sccache if available to speed up incremental rebuilds
if command -v sccache &>/dev/null; then
  export RUSTC_WRAPPER
  RUSTC_WRAPPER="$(command -v sccache)"
  unset CARGO_INCREMENTAL
fi


ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
DIST_DIR="$ROOT_DIR/dist-local"
SPONSOR_PROXY_URL="https://r2modmac-sponsor-production.notfy-stream.workers.dev/api/sponsor"
MODE="all"
MODE_SET=""
REQUESTED_VERSION=""
SKIP_CHECKS=0
CLEAN=0

usage() {
  cat <<'USAGE'
Usage:
  ./build-local.sh [VERSION] [--all|--macos|--linux|--linux-x64|--linux-arm64] [--skip-checks] [--clean]
  ./build-local.sh [VERSION] [all|macos|linux|linux-x64|linux-arm64] [--skip-checks] [--clean]

Build shipping artifacts into dist-local/.

  --all, all                  Build macOS on macOS, then Linux x64 and ARM64.
  --macos, macos              Build Apple Silicon and Intel macOS DMGs.
  --linux, linux              Build Linux x64 and ARM64 tarballs.
  --linux-x64, linux-x64      Build only r2modmac_linux_x64.tar.gz.
  --linux-arm64, linux-arm64  Build only r2modmac_linux_arm64.tar.gz.

Options:
  --skip-checks  Skip Rust formatting, tests, and cargo check.
  --clean        Remove dist-local/ and local Linux target caches first.
  -h, --help     Show this help.

Examples:
  ./build-local.sh 0.8.3 --all
  ./build-local.sh --all
  ./build-local.sh linux --clean
USAGE
}

log() { printf '\n\033[1;34m==> %s\033[0m\n' "$*"; }
warn() { printf '\033[1;33mwarning: %s\033[0m\n' "$*" >&2; }
die() { printf '\033[1;31merror: %s\033[0m\n' "$*" >&2; exit 1; }
require() { command -v "$1" >/dev/null 2>&1 || die "Required command not found: $1"; }

set_mode() {
  local requested="$1"
  if [[ -n "$MODE_SET" && "$MODE" != "$requested" ]]; then
    die "Choose only one build mode (received '$MODE' and '$requested')."
  fi
  MODE="$requested"
  MODE_SET=1
}

for arg in "$@"; do
  case "$arg" in
    --all|all) set_mode all ;;
    --macos|macos) set_mode macos ;;
    --windows|windows) set_mode windows ;;
    --windows-x64|windows-x64) set_mode windows-x64 ;;
    --windows-x86|windows-x86) set_mode windows-x86 ;;
    --windows-arm64|windows-arm64) set_mode windows-arm64 ;;
    --linux|linux) set_mode linux ;;
    --linux-x64|linux-x64) set_mode linux-x64 ;;
    --linux-arm64|linux-arm64) set_mode linux-arm64 ;;
    --skip-checks) SKIP_CHECKS=1 ;;
    --clean) CLEAN=1 ;;
    -h|--help) usage; exit 0 ;;
    *)
      if [[ "$arg" =~ ^v?[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)?$ ]]; then
        [[ -z "$REQUESTED_VERSION" ]] || die "Version was specified more than once."
        REQUESTED_VERSION="${arg#v}"
      else
        usage >&2
        die "Unknown argument: $arg"
      fi
      ;;
  esac
done

cd "$ROOT_DIR"
[[ -f package.json && -f src-tauri/Cargo.toml && -f src-tauri/tauri.conf.json ]] \
  || die "Run this script from the r2modmac repository root."

cleanup_paths=()
cleanup() {
  local path
  for path in "${cleanup_paths[@]:-}"; do
    [[ -n "$path" ]] && rm -rf -- "$path"
  done
}
trap cleanup EXIT INT TERM



verify_versions() {
  require node
  node - "$REQUESTED_VERSION" <<'NODE'
const fs = require('fs');
const requestedVersion = process.argv[2] || '';
const packageVersion = JSON.parse(fs.readFileSync('package.json', 'utf8')).version;
const tauriVersion = JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json', 'utf8')).version;
const cargo = fs.readFileSync('src-tauri/Cargo.toml', 'utf8');
const cargoVersion = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if (!packageVersion || packageVersion !== tauriVersion || packageVersion !== cargoVersion) {
  console.error(`Version mismatch: package=${packageVersion}, tauri=${tauriVersion}, cargo=${cargoVersion}`);
  process.exit(1);
}
if (requestedVersion && requestedVersion !== packageVersion) {
  console.error(`Requested version ${requestedVersion} does not match repository version ${packageVersion}.`);
  process.exit(1);
}
console.log(`Version ${packageVersion} is consistent across npm, Tauri, and Cargo.`);
NODE
}

verify_sponsor_endpoint() {
  require curl
  local status
  status="$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
    --connect-timeout 8 --max-time 15 "$SPONSOR_PROXY_URL")" \
    || die "The production sponsor Worker could not be reached."
  [[ "$status" == "204" ]] || die "Production sponsor Worker returned HTTP $status instead of 204."
}

install_frontend_dependencies() {
  require npm
  log "Installing locked frontend dependencies"
  npm ci --prefer-offline --no-audit --no-fund
}

run_checks() {
  require cargo
  log "Checking Rust formatting"
  cargo fmt --manifest-path src-tauri/Cargo.toml -- --check

  log "Running Rust tests"
  cargo test --manifest-path src-tauri/Cargo.toml --locked

  log "Checking the native application"
  cargo check --manifest-path src-tauri/Cargo.toml --locked
}

build_frontend() {
  log "Building the frontend"
  npm run build
}

verify_compiled_sponsor_endpoint() {
  local binary="$1"
  require grep
  LC_ALL=C grep -aFq -- "$SPONSOR_PROXY_URL" "$binary" \
    || die "Production sponsor endpoint is missing from $(basename "$binary")."
}

sha256_file() {
  local file="$1"
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file"
  else
    require sha256sum
    sha256sum "$file"
  fi
}

validate_linux_archive() {
  local archive="$1"
  require gzip
  require tar
  gzip -t "$archive"
  local listing
  listing="$(tar -tzf "$archive")"
  [[ "$listing" == "r2modmac" ]] \
    || die "Invalid Linux archive layout in $(basename "$archive"): expected only r2modmac, got: $listing"
  [[ "$listing" != *GNUSparseFile* ]] \
    || die "Sparse GNU tar metadata leaked into $(basename "$archive")."
}

build_macos_target() {
  local rust_target="$1"
  local asset_arch="$2"
  require rustup
  require create-dmg
  require ditto
  require hdiutil

  log "Building macOS $asset_arch"
  rustup target add "$rust_target"
  unset R2MODMAC_SPONSOR_PROXY_URL
  npm run tauri build -- --target "$rust_target" --bundles app

  local app_path="$ROOT_DIR/src-tauri/target/$rust_target/release/bundle/macos/r2modmac.app"
  [[ -d "$app_path" ]] || die "macOS app bundle was not produced at $app_path"
  verify_compiled_sponsor_endpoint "$app_path/Contents/MacOS/r2modmac"

  local dmg_root
  dmg_root="$(mktemp -d "${TMPDIR:-/tmp}/r2modmac-dmg.XXXXXX")"
  cleanup_paths+=("$dmg_root")
  ditto "$app_path" "$dmg_root/r2modmac.app"
  [[ -f UNVERIFIED_APP_INSTRUCTIONS.txt ]] && cp UNVERIFIED_APP_INSTRUCTIONS.txt "$dmg_root/"

  local output="$DIST_DIR/r2modmac_macos_${asset_arch}.dmg"
  rm -f -- "$output"
  local extra_icon_args=()
  if [[ -f "$dmg_root/UNVERIFIED_APP_INSTRUCTIONS.txt" ]]; then
    extra_icon_args=(--icon "UNVERIFIED_APP_INSTRUCTIONS.txt" 400 350)
  fi

  create-dmg \
    --volname "r2modmac" \
    --volicon "src-tauri/icons/icon.icns" \
    --window-pos 200 120 \
    --window-size 800 500 \
    --icon-size 100 \
    --icon "r2modmac.app" 200 190 \
    --hide-extension "r2modmac.app" \
    --app-drop-link 600 185 \
    "${extra_icon_args[@]}" \
    "$output" \
    "$dmg_root"

  hdiutil verify "$output" >/dev/null
  sha256_file "$output"
}

build_macos() {
  [[ "$(uname -s)" == "Darwin" ]] || die "macOS artifacts must be built on macOS."
  build_macos_target "aarch64-apple-darwin" "aarch64"
  build_macos_target "x86_64-apple-darwin" "x86_64"
}

# --- Linux via apple/container -----------------------------------------------
# The container system service is started on demand and shut down on script exit.

_CONTAINER_STARTED=0

start_container_system() {
  if [[ "$_CONTAINER_STARTED" == "0" ]]; then
    require container
    log "Starting apple/container service"
    container system start >/dev/null 2>&1 || true
    # Wait until the container API server is responsive (up to 30s)
    local i=0
    until container image list >/dev/null 2>&1; do
      i=$(( i + 1 ))
      if [[ "$i" -ge 30 ]]; then
        die "apple/container service did not become ready in 30s"
      fi
      sleep 1
    done
    _CONTAINER_STARTED=1
  fi
}

stop_container_system() {
  if [[ "$_CONTAINER_STARTED" == "1" ]]; then
    container prune >/dev/null 2>&1 || true
    container system stop >/dev/null 2>&1 || true
    _CONTAINER_STARTED=0
  fi
}

# Extend the existing cleanup trap to also stop the container service
_original_cleanup() {
  local path
  for path in "${cleanup_paths[@]:-}"; do
    [[ -n "$path" ]] && rm -rf -- "$path"
  done
}
cleanup() { _original_cleanup; stop_container_system; }

build_linux_target() {
  local container_arch="$1"   # arm64 or x86_64
  local asset_arch="$2"       # arm64 or x64
  local rust_target="$3"      # e.g. aarch64-unknown-linux-gnu

  local img
  if [[ "$asset_arch" == "arm64" ]]; then
    img="r2modmac-builder-arm64"
  else
    img="r2modmac-builder-x64"
  fi

  require container

  start_container_system

  if ! container image inspect "$img" &>/dev/null; then
    die "apple/container image '$img' not found. Build it first with: container build --arch $container_arch -t $img <Containerfile-dir>"
  fi

  log "Building Linux $asset_arch via apple/container ($img)"

  local archive_name="r2modmac_linux_${asset_arch}.tar.gz"
  local archive="$DIST_DIR/$archive_name"
  rm -f "$archive"

  container run --rm -i \
    --arch "$container_arch" \
    -m 8G \
    -v "$ROOT_DIR:$ROOT_DIR" \
    -v "$HOME/.cargo/registry:/root/.cargo/registry" \
    -v "$HOME/.cargo/git:/root/.cargo/git" \
    -w "$ROOT_DIR" \
    "$img" \
    /bin/bash -s -- "$rust_target" "$ROOT_DIR" "$archive_name" "$SPONSOR_PROXY_URL" <<'INNER'
#!/bin/bash
set -Eeuo pipefail
RUST_TARGET="$1"
ROOT_DIR="$2"
ARCHIVE_NAME="$3"
SPONSOR_PROXY_URL="$4"

export PATH="/root/.cargo/bin:$PATH"
export TAURI_CONFIG='{"build":{"beforeBuildCommand":"","devUrl":null}}'
export TAURI_ENV_TARGET_TRIPLE="$RUST_TARGET"

cd "$ROOT_DIR"
rm -f "src-tauri/target/$RUST_TARGET/release/r2modmac"
/root/.cargo/bin/cargo build --release --locked \
  --manifest-path src-tauri/Cargo.toml \
  --target "$RUST_TARGET"

binary="src-tauri/target/$RUST_TARGET/release/r2modmac"
test -f "$binary" || { echo "Linux binary not found: $binary" >&2; exit 1; }
LC_ALL=C grep -aFq -- "$SPONSOR_PROXY_URL" "$binary" \
  || { echo "Production sponsor endpoint is missing from Linux binary" >&2; exit 1; }

stage="$(mktemp -d)"
trap 'rm -rf -- "$stage"' EXIT
cp "$binary" "$stage/r2modmac"
chmod 0755 "$stage/r2modmac"
touch "$stage/r2modmac"
tar -C "$stage" -czf "$stage/$ARCHIVE_NAME" r2modmac
cp "$stage/$ARCHIVE_NAME" "$ROOT_DIR/dist-local/$ARCHIVE_NAME"
INNER


  [[ -f "$archive" ]] || die "Linux archive was not produced: $archive"
  validate_linux_archive "$archive"
  sha256_file "$archive"
}

build_linux_x64()   { build_linux_target "x86_64" "x64"   "x86_64-unknown-linux-gnu"; }
build_linux_arm64() { build_linux_target "arm64"  "arm64" "aarch64-unknown-linux-gnu"; }


write_checksums() {
  local output="$DIST_DIR/SHA256SUMS.txt"
  : > "$output"
  local file
  while IFS= read -r file; do
    [[ "$(basename "$file")" == "SHA256SUMS.txt" ]] && continue
    (cd "$DIST_DIR" && sha256_file "$(basename "$file")") >> "$output"
  done < <(find "$DIST_DIR" -maxdepth 1 -type f -print | LC_ALL=C sort)
  log "Checksums written to $output"
}


verify_versions
verify_sponsor_endpoint

if [[ "$CLEAN" == "1" ]]; then
  log "Cleaning local build outputs"
  rm -rf -- "$DIST_DIR" "$ROOT_DIR"/src-tauri/target-local-linux-*
fi
mkdir -p "$DIST_DIR"

install_frontend_dependencies
build_frontend
if [[ "$SKIP_CHECKS" == "0" ]]; then run_checks; else warn "Shipping checks were skipped by explicit request."; fi

# --- Windows via cargo-xwin --------------------------------------------------
SKIP_BEFORE='{"build":{"beforeBuildCommand":"","devUrl":null}}'

build_windows_target() {
  local rust_target="$1"   # e.g. x86_64-pc-windows-msvc
  local asset_arch="$2"    # x64 | x86 | arm64

  require cargo

  if ! command -v cargo-xwin &>/dev/null; then
    log "Installing cargo-xwin"
    cargo install cargo-xwin >/dev/null 2>&1
  fi

  rustup target add "$rust_target" >/dev/null 2>&1 || true

  log "Building Windows $asset_arch"
  XWIN_CROSS_COMPILER=clang \
  CPPFLAGS="-DZSTD_DISABLE_ASM=1 -DZSTD_NO_INTRINSICS=1" \
  CFLAGS="-DZSTD_DISABLE_ASM=1 -DZSTD_NO_INTRINSICS=1" \
  ZSTD_SYS_DISABLE_ASM=1 \
  ZSTD_DISABLE_ASM=1 \
    npx tauri build \
      --target "$rust_target" \
      --runner cargo-xwin \
      --no-bundle \
      --config "$SKIP_BEFORE"

  local exe="src-tauri/target/$rust_target/release/r2modmac.exe"
  [[ -f "$exe" ]] || die "Windows binary not produced: $exe"

  local zip="$DIST_DIR/r2modmac_windows_${asset_arch}.zip"
  rm -f "$zip"
  zip -j "$zip" "$exe"
  sha256_file "$zip"
}

build_windows_x64()   { build_windows_target "x86_64-pc-windows-msvc"  "x64"; }
build_windows_x86()   { build_windows_target "i686-pc-windows-msvc"    "x86"; }
build_windows_arm64() { build_windows_target "aarch64-pc-windows-msvc" "arm64"; }

build_windows() {
  build_windows_x64
  build_windows_x86
  build_windows_arm64
}

# --- Dispatch -----------------------------------------------------------------
case "$MODE" in
  all)
    if [[ "$(uname -s)" == "Darwin" ]]; then build_macos; else warn "Skipping macOS artifacts on non-macOS host."; fi
    build_windows
    build_linux_x64
    build_linux_arm64
    ;;
  macos)         build_macos ;;
  windows)       build_windows ;;
  windows-x64)   build_windows_x64 ;;
  windows-x86)   build_windows_x86 ;;
  windows-arm64) build_windows_arm64 ;;
  linux)         build_linux_x64; build_linux_arm64 ;;
  linux-x64)     build_linux_x64 ;;
  linux-arm64)   build_linux_arm64 ;;
esac

write_checksums
log "Shipping artifacts are ready in $DIST_DIR"
