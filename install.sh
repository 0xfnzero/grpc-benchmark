#!/bin/bash

set -euo pipefail

REPO="${REPO:-0xfnzero/grpc-benchmark}"
VERSION="${VERSION:-latest}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/grpc-benchmark}"
TARGET="${TARGET:-}"
INSTALL_MARKER=".grpc-benchmark-install"

detect_target() {
  local os arch
  case "$(uname -s)" in
    Darwin) os="apple-darwin" ;;
    Linux) os="unknown-linux-gnu" ;;
    *)
      echo "Unsupported OS: $(uname -s). Supported: macOS, Linux." >&2
      exit 1
      ;;
  esac

  case "$(uname -m)" in
    x86_64|amd64) arch="x86_64" ;;
    arm64|aarch64) arch="aarch64" ;;
    *)
      echo "Unsupported arch: $(uname -m). Supported: x86_64, aarch64/arm64." >&2
      exit 1
      ;;
  esac

  echo "${arch}-${os}"
}

download() {
  local url="$1"
  local out="$2"
  if command -v curl >/dev/null 2>&1; then
    curl -fL --retry 3 -o "$out" "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget -q --show-progress -O "$out" "$url"
  else
    echo "curl or wget is required" >&2
    exit 1
  fi
}

validate_install_dir() {
  case "$INSTALL_DIR" in
    ""|"/"|"/."|"."|".."|"$HOME"|"$HOME/")
      echo "Refusing unsafe INSTALL_DIR: ${INSTALL_DIR}" >&2
      exit 1
      ;;
  esac
}

validate_existing_install_dir() {
  if [[ ! -e "$INSTALL_DIR" ]]; then
    return 0
  fi

  if [[ ! -d "$INSTALL_DIR" ]]; then
    echo "INSTALL_DIR exists but is not a directory: ${INSTALL_DIR}" >&2
    exit 1
  fi

  if [[ -f "$INSTALL_DIR/$INSTALL_MARKER" ]]; then
    return 0
  fi

  if [[ -n "$(find "$INSTALL_DIR" -mindepth 1 -maxdepth 1 -print -quit)" && "${FORCE_INSTALL:-0}" != "1" ]]; then
    echo "Refusing to replace non-managed directory: ${INSTALL_DIR}" >&2
    echo "Use INSTALL_DIR=/path/to/grpc-benchmark or set FORCE_INSTALL=1 to back it up and replace it." >&2
    exit 1
  fi
}

install_package() {
  local package_dir="$1"

  if [[ ! -f "$package_dir/$INSTALL_MARKER" ]]; then
    echo "Invalid package: missing ${INSTALL_MARKER}" >&2
    exit 1
  fi

  local parent base tmp_install backup
  parent="$(dirname "$INSTALL_DIR")"
  base="$(basename "$INSTALL_DIR")"
  tmp_install="${parent}/.${base}.tmp.$$"
  backup=""

  mkdir -p "$parent"
  rm -rf "$tmp_install"
  cp -R "$package_dir" "$tmp_install"

  if [[ -e "$INSTALL_DIR" ]]; then
    backup="${INSTALL_DIR}.backup.$(date +%Y%m%d%H%M%S)"
    mv "$INSTALL_DIR" "$backup"
  fi

  if ! mv "$tmp_install" "$INSTALL_DIR"; then
    if [[ -n "$backup" && -e "$backup" ]]; then
      mv "$backup" "$INSTALL_DIR"
    fi
    echo "Install failed while replacing ${INSTALL_DIR}" >&2
    exit 1
  fi

  if [[ -n "$backup" ]]; then
    if [[ -f "$backup/$INSTALL_MARKER" ]]; then
      rm -rf "$backup"
    else
      echo "Previous non-managed directory was backed up to: $backup"
    fi
  fi
}

verify_checksum() {
  local sums_file="$1"
  local archive_file="$2"

  if [[ ! -s "$sums_file" ]]; then
    return 0
  fi

  local expected
  expected="$(awk -v file="$ARCHIVE" '$2 == file {print $1; exit}' "$sums_file")"
  if [[ -z "$expected" ]]; then
    echo "Checksum entry not found for ${ARCHIVE}" >&2
    exit 1
  fi

  local actual
  if command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "$archive_file" | awk '{print $1}')"
  elif command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$archive_file" | awk '{print $1}')"
  else
    echo "shasum or sha256sum is required to verify release checksums" >&2
    exit 1
  fi

  if [[ "$actual" != "$expected" ]]; then
    echo "Checksum mismatch for ${ARCHIVE}" >&2
    exit 1
  fi
}

if [[ -z "$TARGET" ]]; then
  TARGET="$(detect_target)"
fi

if [[ -n "${BASE_URL:-}" ]]; then
  BASE_URL="${BASE_URL%/}"
elif [[ "$VERSION" == "latest" ]]; then
  BASE_URL="https://github.com/${REPO}/releases/latest/download"
else
  BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"
fi

ARCHIVE="grpc-benchmark-${TARGET}.tar.gz"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
validate_install_dir
validate_existing_install_dir

echo "=========================================="
echo "gRPC / Shred benchmark installer"
echo "=========================================="
echo "repo: ${REPO}"
echo "version: ${VERSION}"
echo "target: ${TARGET}"
echo "install dir: ${INSTALL_DIR}"
echo ""

download "${BASE_URL}/${ARCHIVE}" "${TMP_DIR}/${ARCHIVE}"
if download "${BASE_URL}/SHA256SUMS" "${TMP_DIR}/SHA256SUMS"; then
  verify_checksum "${TMP_DIR}/SHA256SUMS" "${TMP_DIR}/${ARCHIVE}"
else
  echo "SHA256SUMS not available; skipping checksum verification"
fi
tar -xzf "${TMP_DIR}/${ARCHIVE}" -C "$TMP_DIR"

install_package "${TMP_DIR}/grpc-benchmark"

chmod +x "$INSTALL_DIR"/grpc-comparison
chmod +x "$INSTALL_DIR"/shred-comparison
chmod +x "$INSTALL_DIR"/grpc-vs-shred
chmod +x "$INSTALL_DIR"/latency-test
chmod +x "$INSTALL_DIR"/shred
chmod +x "$INSTALL_DIR"/shred-benchmark
chmod +x "$INSTALL_DIR"/benchmark-jito
chmod +x "$INSTALL_DIR"/run-*.sh

echo ""
echo "Installed."
echo ""
echo "Comparison benchmarks:"
echo "  cd \"$INSTALL_DIR\""
echo "  ./run-grpc-comparison.sh"
echo "  ./run-shred-comparison.sh"
echo "  ./run-grpc-vs-shred.sh"
echo ""
echo "Single-source benchmarks and tools:"
echo "  ./run-latency-test.sh"
echo "  ./run-shred.sh"
echo "  ./run-shred-benchmark.sh"
echo "  ./run-benchmark-jito.sh"
echo ""
echo "Help:"
echo "  ./grpc-comparison --help"
echo "  ./shred-comparison --help"
echo "  ./grpc-vs-shred --help"
