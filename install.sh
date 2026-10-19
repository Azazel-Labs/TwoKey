#!/bin/sh
# Install twokey from GitHub Releases. No crates.io, no Rust toolchain needed.
#
#   curl -fsSL https://raw.githubusercontent.com/Azazel-Labs/twokey/main/install.sh | sh
#   curl -fsSL .../install.sh | sh -s -- --version v0.1.0 --to /usr/local/bin
#
# Options (or env vars):
#   --version vX.Y.Z   TWOKEY_VERSION   release tag (default: latest release)
#   --to DIR           TWOKEY_INSTALL_DIR  install directory (default: ~/.local/bin)
#   --sha256 HEX       TWOKEY_SHA256    expected archive checksum; pin this in CI
#   --source                            build from the vendored source bundle
#                                       (offline: needs cargo, not crates.io)
#   --repo OWNER/NAME  TWOKEY_REPO      where to download from
#                      TWOKEY_GITHUB    GitHub base URL (default https://github.com;
#                                       set for GitHub Enterprise or a mirror)
#
# Every download is checked against the release's SHA256SUMS; with --sha256 it
# must also match the checksum you pinned (protects against a replaced release).
set -eu

NAME="twokey"
REPO="${TWOKEY_REPO:-Azazel-Labs/twokey}"
GH="${TWOKEY_GITHUB:-https://github.com}"
VERSION="${TWOKEY_VERSION:-}"
DEST="${TWOKEY_INSTALL_DIR:-$HOME/.local/bin}"
PIN="${TWOKEY_SHA256:-}"
FROM_SOURCE=0

while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --to) DEST="$2"; shift 2 ;;
    --sha256) PIN="$2"; shift 2 ;;
    --repo) REPO="$2"; shift 2 ;;
    --source) FROM_SOURCE=1; shift ;;
    -h|--help) sed -n '2,20p' "$0" 2>/dev/null || true; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

say() { printf '%s\n' "$*" >&2; }
die() { say "error: $*"; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "$1 is required"; }

if command -v curl >/dev/null 2>&1; then
  fetch() { curl -fsSL --retry 3 -o "$2" "$1"; }
  final_url() { curl -fsSLI -o /dev/null -w '%{url_effective}' "$1"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget -q -O "$2" "$1"; }
  final_url() { wget -q --max-redirect=5 -S -O /dev/null "$1" 2>&1 | awk '/^  Location: /{u=$2} END{print u}'; }
else
  die "curl or wget is required"
fi

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | cut -d' ' -f1
  else die "sha256sum or shasum is required"; fi
}

# Latest release tag without the GitHub API (no rate limits): follow the redirect.
if [ -z "$VERSION" ]; then
  url=$(final_url "$GH/$REPO/releases/latest") || die "can't reach $GH"
  VERSION="${url##*/}"
  case "$VERSION" in v[0-9]*) ;; *) die "couldn't determine the latest release (got '$VERSION'); pass --version" ;; esac
fi

if [ "$FROM_SOURCE" = 1 ]; then
  asset="$NAME-$VERSION-vendored-src.tar.gz"
else
  os=$(uname -s); arch=$(uname -m)
  case "$arch" in x86_64|amd64) arch=x86_64 ;; arm64|aarch64) arch=aarch64 ;; *) die "no prebuilt binary for $arch; use --source" ;; esac
  case "$os" in
    Linux) target="$arch-unknown-linux-musl"; ext=tar.gz ;;   # static: any distro
    Darwin) target="$arch-apple-darwin"; ext=tar.gz ;;
    MINGW*|MSYS*|CYGWIN*) target="$arch-pc-windows-msvc"; ext=zip ;;
    *) die "no prebuilt binary for $os; use --source" ;;
  esac
  asset="$NAME-$VERSION-$target.$ext"
fi

base="$GH/$REPO/releases/download/$VERSION"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

say "downloading $asset ($REPO $VERSION)"
fetch "$base/$asset" "$tmp/$asset" || die "download failed: $base/$asset"
fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" || die "download failed: $base/SHA256SUMS"

got=$(sha256 "$tmp/$asset")
want=$(awk -v f="$asset" '$2 == f || $2 == "*"f {print $1}' "$tmp/SHA256SUMS")
[ -n "$want" ] || die "$asset is not listed in SHA256SUMS"
[ "$got" = "$want" ] || die "checksum mismatch for $asset (got $got, SHA256SUMS says $want)"
if [ -n "$PIN" ] && [ "$got" != "$PIN" ]; then
  die "checksum mismatch for $asset (got $got, pinned $PIN)"
fi
say "checksum ok ($got)"

mkdir -p "$DEST"
if [ "$FROM_SOURCE" = 1 ]; then
  need cargo
  tar -xzf "$tmp/$asset" -C "$tmp"
  src=$(find "$tmp" -mindepth 1 -maxdepth 1 -type d | head -n1)
  say "building from vendored source (offline)"
  cargo install --quiet --path "$src" --offline --locked --root "$tmp/root" >&2
  cp "$tmp/root/bin/$NAME" "$DEST/$NAME"
else
  case "$asset" in
    *.zip) need unzip; unzip -q -o "$tmp/$asset" -d "$tmp/x"; bin="$tmp/x/$NAME.exe"; out="$DEST/$NAME.exe" ;;
    *) mkdir -p "$tmp/x"; tar -xzf "$tmp/$asset" -C "$tmp/x"; bin="$tmp/x/$NAME"; out="$DEST/$NAME" ;;
  esac
  [ -f "$bin" ] || die "archive doesn't contain $NAME"
  cp "$bin" "$out"
  chmod 755 "$out"
fi

say "installed $("$DEST/$NAME" --version 2>/dev/null || echo "$NAME $VERSION") to $DEST"
case ":$PATH:" in *":$DEST:"*) ;; *) say "note: $DEST is not on your PATH" ;; esac
