#!/usr/bin/env bash
# Fetch the PDFium version matching desktop/Cargo.toml. No system install needed.
set -euo pipefail
folio_root="$(cd "$(dirname "$0")/.." && pwd)"
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) folio_platform=mac-arm64 ;;
  Darwin-x86_64) folio_platform=mac-x64 ;;
  Linux-x86_64) folio_platform=linux-x64 ;;
  Linux-aarch64|Linux-arm64) folio_platform=linux-arm64 ;;
  *) echo 'Supported: macOS/Linux on arm64 or x86_64.' >&2; exit 1 ;;
esac
folio_archive="$(mktemp)"
trap 'rm -f "$folio_archive"' EXIT
folio_release='chromium%2F7881'
folio_url="https://github.com/bblanchon/pdfium-binaries/releases/download/$folio_release/pdfium-$folio_platform.tgz"
echo "Downloading PDFium 7881 for $folio_platform"
curl --fail --location --retry 2 --connect-timeout 15 "$folio_url" --output "$folio_archive"
mkdir -p "$folio_root/runtime"
tar -xzf "$folio_archive" -C "$folio_root/runtime"
echo "PDFium installed in $folio_root/runtime/lib (licenses retained in runtime/)."
