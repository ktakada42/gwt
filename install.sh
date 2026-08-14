#!/bin/sh
set -e

REPO="ktakada42/gwx"
BIN="gwx"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

err() {
  echo "error: $1" >&2
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || err "required command not found: $1"
}

need curl
need tar

# Detect OS
case "$(uname -s)" in
  Darwin) OS="apple-darwin" ;;
  Linux)  OS="unknown-linux-musl" ;;
  *) err "unsupported OS: $(uname -s)" ;;
esac

# Detect arch
case "$(uname -m)" in
  x86_64)          ARCH="x86_64" ;;
  arm64 | aarch64) ARCH="aarch64" ;;
  *) err "unsupported architecture: $(uname -m)" ;;
esac

TARGET="${ARCH}-${OS}"

# Resolve latest version
VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep '"tag_name"' \
  | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')
[ -n "$VERSION" ] || err "could not determine latest release version"

ARCHIVE="${BIN}-${VERSION}-${TARGET}.tar.gz"
URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARCHIVE}"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

echo "Downloading ${BIN} ${VERSION} (${TARGET})..."
curl -fsSL "$URL" -o "${TMP}/${ARCHIVE}" || err "download failed: $URL"

tar -xzf "${TMP}/${ARCHIVE}" -C "$TMP"

mkdir -p "$INSTALL_DIR"
mv "${TMP}/${BIN}" "${INSTALL_DIR}/${BIN}"
chmod +x "${INSTALL_DIR}/${BIN}"

echo "Installed ${BIN} ${VERSION} to ${INSTALL_DIR}/${BIN}"

# Warn if INSTALL_DIR is not in PATH
case ":${PATH}:" in
  *":${INSTALL_DIR}:"*) ;;
  *) echo "note: add ${INSTALL_DIR} to your PATH to use ${BIN}" ;;
esac

echo "note: enable \`gwx cd\` and completions with: eval \"\$(${BIN} shell-init zsh)\""
