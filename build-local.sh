#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'
umask 022

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

verify_clean_tree() {
  require git
  if [[ "${ALLOW_DIRTY_BUILD:-0}" != "1" ]] && [[ -n "$(git status --porcelain --untracked-files=no)" ]]; then
    die "Tracked files are modified. Commit or stash them, or set ALLOW_DIRTY_BUILD=1 explicitly."
  fi
}

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
  require strings
  strings -a "$binary" | grep -Fq "$SPONSOR_PROXY_URL" \
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

write_linux_builder_dockerfile() {
  local dockerfile="$1"
  cat > "$dockerfile" <<'DOCKERFILE'
FROM ubuntu:22.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential ca-certificates curl file gzip pkg-config tar binutils \
    libssl-dev libgtk-3-dev libwebkit2gtk-4.1-dev librsvg2-dev \
    libayatana-appindicator3-dev libxdo-dev patchelf \
 && rm -rf /var/lib/apt/lists/*
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain stable
ENV PATH=/root/.cargo/bin:${PATH}
WORKDIR /work
DOCKERFILE
}

build_linux_target() {
  local platform="$1"
  local asset_arch="$2"
  local expected_machine="$3"
  require docker

  local builder_dir
  builder_dir="$(mktemp -d "${TMPDIR:-/tmp}/r2modmac-linux-builder.XXXXXX")"
  cleanup_paths+=("$builder_dir")
  write_linux_builder_dockerfile "$builder_dir/Dockerfile"

  local image="r2modmac-linux-builder:${asset_arch}"
  log "Preparing Linux $asset_arch builder"
  docker buildx build --platform "$platform" --load --tag "$image" "$builder_dir"

  log "Building Linux $asset_arch"
  mkdir -p "$ROOT_DIR/src-tauri/target-local-linux-${asset_arch}"
  docker volume create r2modmac-cargo-registry >/dev/null
  docker volume create r2modmac-cargo-git >/dev/null
  local archive_name="r2modmac_linux_${asset_arch}.tar.gz"

  docker run --rm \
    --platform "$platform" \
    --volume "$ROOT_DIR:/work" \
    --volume r2modmac-cargo-registry:/root/.cargo/registry \
    --volume r2modmac-cargo-git:/root/.cargo/git \
    --env "CARGO_TARGET_DIR=/work/src-tauri/target-local-linux-${asset_arch}" \
    --env "ARCHIVE_NAME=$archive_name" \
    --env "EXPECTED_MACHINE=$expected_machine" \
    --env "SPONSOR_PROXY_URL=$SPONSOR_PROXY_URL" \
    --env "HOST_UID=$(id -u)" \
    --env "HOST_GID=$(id -g)" \
    "$image" \
    bash -lc '
      set -Eeuo pipefail
      cd /work
      unset R2MODMAC_SPONSOR_PROXY_URL
      cargo build --manifest-path src-tauri/Cargo.toml --release --locked
      binary="$CARGO_TARGET_DIR/release/r2modmac"
      test -f "$binary" || { echo "Linux binary not found: $binary" >&2; exit 1; }
      strings -a "$binary" | grep -Fq "$SPONSOR_PROXY_URL" || { echo "Production sponsor endpoint is missing from Linux binary" >&2; exit 1; }
      machine="$(readelf -h "$binary" | awk -F: "/Machine:/ { gsub(/^[[:space:]]+/, \"\", \$2); print \$2 }")"
      case "$machine" in
        *"$EXPECTED_MACHINE"*) ;;
        *) echo "Unexpected ELF architecture: $machine" >&2; exit 1 ;;
      esac
      stage="$(mktemp -d)"
      trap "rm -rf -- \"$stage\"" EXIT
      cp --reflink=never --sparse=never "$binary" "$stage/r2modmac"
      chmod 0755 "$stage/r2modmac"
      tar --format=ustar --sort=name --mtime=@0 \
        --owner=0 --group=0 --numeric-owner \
        -C "$stage" -cf - r2modmac \
        | gzip -9n > "/work/dist-local/$ARCHIVE_NAME"
      chown "$HOST_UID:$HOST_GID" "/work/dist-local/$ARCHIVE_NAME" 2>/dev/null || true
    '

  local archive="$DIST_DIR/$archive_name"
  [[ -f "$archive" ]] || die "Linux archive was not produced: $archive"
  validate_linux_archive "$archive"
  sha256_file "$archive"
}

build_linux_x64() { build_linux_target "linux/amd64" "x64" "X86-64"; }
build_linux_arm64() { build_linux_target "linux/arm64" "arm64" "AArch64"; }

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

verify_clean_tree
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

case "$MODE" in
  all)
    if [[ "$(uname -s)" == "Darwin" ]]; then build_macos; else warn "Skipping macOS artifacts on non-macOS host."; fi
    build_linux_x64
    build_linux_arm64
    ;;
  macos) build_macos ;;
  linux) build_linux_x64; build_linux_arm64 ;;
  linux-x64) build_linux_x64 ;;
  linux-arm64) build_linux_arm64 ;;
esac

write_checksums
log "Shipping artifacts are ready in $DIST_DIR"
