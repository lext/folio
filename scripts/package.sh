#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_HOME="${CARGO_HOME:-$PWD/.cargo-local}"
folio_os="$(uname -s)"
folio_arch="$(uname -m)"
case "$folio_os" in
  Darwin) folio_library=libpdfium.dylib ;;
  Linux) folio_library=libpdfium.so ;;
  *) echo 'Supported platforms: macOS and Linux' >&2; exit 1 ;;
esac
if [ ! -f "runtime/lib/$folio_library" ] || [ ! -f runtime/LICENSE ] || [ ! -d runtime/licenses ]; then
  echo 'PDFium runtime or licenses are missing. Run scripts/setup-pdfium.sh first.' >&2
  exit 1
fi
# The Cargo manifest is the release version's source of truth.
folio_version="$(sed -n 's/^version = "\([^"]*\)"$/\1/p' desktop/Cargo.toml)"
if [ -z "$folio_version" ]; then
  echo 'Cannot read the desktop version from desktop/Cargo.toml.' >&2
  exit 1
fi
# PDFium's pinned macOS binary requires macOS 12+. Keep native/Rust code aligned.
if [ "$folio_os" = Darwin ]; then
  export MACOSX_DEPLOYMENT_TARGET=12.0
fi
cargo build --offline --locked --release --manifest-path desktop/Cargo.toml
case "$folio_os" in
  Darwin)
    folio_bundle="$PWD/dist/Folio.app/Contents"
    mkdir -p "$folio_bundle/MacOS" "$folio_bundle/Resources/licenses"
    cp desktop/target/release/folio runtime/lib/libpdfium.dylib "$folio_bundle/MacOS/"
    # macOS treats regular files in MacOS/ as code. Keep the editable data in
    # Resources/ with a relative link beside the executable for normal lookup.
    # Migrate an existing regular file without overwriting another config.
    if [ -f "$folio_bundle/MacOS/folio.toml" ] && [ ! -L "$folio_bundle/MacOS/folio.toml" ]; then
      if [ -e "$folio_bundle/Resources/folio.toml" ]; then
        echo 'Both MacOS/folio.toml and Resources/folio.toml exist; retain one settings file before packaging.' >&2
        exit 1
      fi
      mv "$folio_bundle/MacOS/folio.toml" "$folio_bundle/Resources/folio.toml"
    fi
    if [ ! -e "$folio_bundle/Resources/folio.toml" ] && [ ! -L "$folio_bundle/Resources/folio.toml" ]; then
      cp folio.toml "$folio_bundle/Resources/folio.toml"
    fi
    if [ ! -e "$folio_bundle/MacOS/folio.toml" ] && [ ! -L "$folio_bundle/MacOS/folio.toml" ]; then
      ln -s ../Resources/folio.toml "$folio_bundle/MacOS/folio.toml"
    fi
    cp -R runtime/licenses/ "$folio_bundle/Resources/licenses/"
    cp runtime/LICENSE "$folio_bundle/Resources/licenses/PDFium-LICENSE"
    cp LICENSE "$folio_bundle/Resources/licenses/Folio-LICENSE"
    cp vendor/block/LICENSE "$folio_bundle/Resources/licenses/block-LICENSE"
    cp THIRD_PARTY.md "$folio_bundle/Resources/licenses/THIRD_PARTY.md"
    cat > "$folio_bundle/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Folio</string>
<key>CFBundleDisplayName</key><string>Folio</string>
<key>CFBundleIdentifier</key><string>dev.folio.editor</string>
<key>CFBundleVersion</key><string>$folio_version</string>
<key>CFBundleShortVersionString</key><string>$folio_version</string>
<key>CFBundleExecutable</key><string>folio</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSMinimumSystemVersion</key><string>12.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
    codesign --force --sign - "$folio_bundle/MacOS/libpdfium.dylib"
    codesign --force --sign - "$PWD/dist/Folio.app"
    codesign --verify --deep --strict "$PWD/dist/Folio.app"
    folio_archive="$PWD/dist/folio-$folio_version-macos-$folio_arch.zip"
    # Archive before uploading: GitHub artifact transport does not retain the
    # executable modes and symlinks a macOS app bundle needs.
    ditto -c -k --keepParent "$PWD/dist/Folio.app" "$folio_archive"
    printf 'App: %s/dist/Folio.app\n' "$PWD"
    printf 'Archive: %s\n' "$folio_archive"
    ;;
  Linux)
    folio_bundle="$PWD/dist/folio-linux-$folio_arch"
    mkdir -p "$folio_bundle/licenses"
    cp desktop/target/release/folio runtime/lib/libpdfium.so "$folio_bundle/"
    if [ ! -e "$folio_bundle/folio.toml" ] && [ ! -L "$folio_bundle/folio.toml" ]; then
      cp folio.toml "$folio_bundle/folio.toml"
    fi
    cp -R runtime/licenses/. "$folio_bundle/licenses/"
    cp runtime/LICENSE "$folio_bundle/licenses/PDFium-LICENSE"
    cp LICENSE "$folio_bundle/licenses/Folio-LICENSE"
    cp THIRD_PARTY.md "$folio_bundle/licenses/THIRD_PARTY.md"
    folio_archive="$PWD/dist/folio-$folio_version-linux-$folio_arch.tar.gz"
    tar -czf "$folio_archive" -C dist "folio-linux-$folio_arch"
    printf 'App: %s/folio\n' "$folio_bundle"
    printf 'Archive: %s\n' "$folio_archive"
    ;;
  *) echo 'Supported platforms: macOS and Linux' >&2; exit 1 ;;
esac
