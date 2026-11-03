#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_HOME="${CARGO_HOME:-$PWD/.cargo-local}"
folio_os="$(uname -s)"
folio_arch="$(uname -m)"
# Supported targets: macOS on Apple silicon, and x86_64 Ubuntu 24.04/26.04.
case "$folio_os/$folio_arch" in
  Darwin/arm64) folio_library=libpdfium.dylib ;;
  Linux/x86_64) folio_library=libpdfium.so ;;
  *) echo "Unsupported platform $folio_os/$folio_arch. Supported: macOS on Apple silicon (arm64), Ubuntu on x86_64." >&2; exit 1 ;;
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
# Rolling builds append build metadata (e.g. 0.9.5+git.20261103.8c7ed90) so a
# newer rolling package upgrades an older one. The base must stay the manifest's.
folio_package_version="${FOLIO_PACKAGE_VERSION:-$folio_version}"
case "$folio_package_version" in
  "$folio_version"|"$folio_version"+*) ;;
  *) echo "FOLIO_PACKAGE_VERSION must be $folio_version or $folio_version+<build>." >&2; exit 1 ;;
esac
# '+' is not safe in every download URL; file names use '-' instead.
folio_file_version="${folio_package_version//+/-}"
# PDFium's pinned macOS binary requires macOS 12+. Keep native/Rust code aligned.
if [ "$folio_os" = Darwin ]; then
  export MACOSX_DEPLOYMENT_TARGET=12.0
fi
cargo build --offline --locked --release --manifest-path desktop/Cargo.toml
# Packaging preserves a settings file someone edited. One that is still an
# exact copy of an earlier shipped default was never edited, and would pin
# every old default over the new ones, so it is replaced. A build made from
# uncommitted sources is recognised by the reference copy shipped beside it.
folio_refresh_settings() {
  [ -f "$1" ] && ! cmp -s "$1" folio.toml || return 0
  if [ -f "$1.default" ] && cmp -s "$1" "$1.default"; then
    cp folio.toml "$1"
    echo "Replaced unedited settings from an earlier build: $1"
    return 0
  fi
  for folio_commit in $(git log --format=%H -- folio.toml 2>/dev/null); do
    if git show "$folio_commit:folio.toml" 2>/dev/null | cmp -s - "$1"; then
      cp folio.toml "$1"
      echo "Replaced unedited settings from an earlier build: $1"
      return 0
    fi
  done
  echo "Kept edited settings (new defaults apply only to keys it omits): $1"
}
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
    folio_refresh_settings "$folio_bundle/Resources/folio.toml"
    if [ ! -e "$folio_bundle/Resources/folio.toml" ] && [ ! -L "$folio_bundle/Resources/folio.toml" ]; then
      cp folio.toml "$folio_bundle/Resources/folio.toml"
    fi
    cp folio.toml "$folio_bundle/Resources/folio.toml.default"
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
    folio_archive="$PWD/dist/folio-$folio_file_version-macos-$folio_arch.zip"
    # Archive before uploading: GitHub artifact transport does not retain the
    # executable modes and symlinks a macOS app bundle needs.
    rm -f "$PWD"/dist/folio-*-macos-*.zip "$PWD"/dist/folio-*-macos-*.dmg
    ditto -c -k --keepParent "$PWD/dist/Folio.app" "$folio_archive"
    # Disk image for downloads: the app beside an Applications link to drag onto.
    # It is built from a fresh copy so a developer's edited settings never ship.
    folio_dmg_root="$PWD/dist/dmg"
    rm -rf "$folio_dmg_root"
    mkdir -p "$folio_dmg_root"
    ditto "$PWD/dist/Folio.app" "$folio_dmg_root/Folio.app"
    cp folio.toml "$folio_dmg_root/Folio.app/Contents/Resources/folio.toml"
    codesign --force --sign - "$folio_dmg_root/Folio.app"
    ln -s /Applications "$folio_dmg_root/Applications"
    folio_dmg="$PWD/dist/folio-$folio_file_version-macos-$folio_arch.dmg"
    hdiutil create -quiet -volname Folio -srcfolder "$folio_dmg_root" -fs HFS+ \
      -format UDZO -ov "$folio_dmg"
    rm -rf "$folio_dmg_root"
    printf 'App: %s/dist/Folio.app\n' "$PWD"
    printf 'Archive: %s\n' "$folio_archive"
    printf 'Disk image: %s\n' "$folio_dmg"
    ;;
  Linux)
    folio_bundle="$PWD/dist/folio-linux-$folio_arch"
    mkdir -p "$folio_bundle/licenses"
    cp desktop/target/release/folio runtime/lib/libpdfium.so "$folio_bundle/"
    folio_refresh_settings "$folio_bundle/folio.toml"
    if [ ! -e "$folio_bundle/folio.toml" ] && [ ! -L "$folio_bundle/folio.toml" ]; then
      cp folio.toml "$folio_bundle/folio.toml"
    fi
    cp folio.toml "$folio_bundle/folio.toml.default"
    cp -R runtime/licenses/. "$folio_bundle/licenses/"
    cp runtime/LICENSE "$folio_bundle/licenses/PDFium-LICENSE"
    cp LICENSE "$folio_bundle/licenses/Folio-LICENSE"
    cp THIRD_PARTY.md "$folio_bundle/licenses/THIRD_PARTY.md"
    folio_archive="$PWD/dist/folio-$folio_file_version-linux-$folio_arch.tar.gz"
    rm -f "$PWD"/dist/folio-*-linux-*.tar.gz "$PWD"/dist/folio_*.deb
    tar -czf "$folio_archive" -C dist "folio-linux-$folio_arch"
    printf 'App: %s/folio\n' "$folio_bundle"
    printf 'Archive: %s\n' "$folio_archive"
    if command -v dpkg-deb >/dev/null 2>&1; then
      folio_deb_arch=amd64
      # Folio finds PDFium and an optional folio.toml beside its resolved
      # executable. No settings file is installed there: it would take
      # precedence over ~/.config/folio/folio.toml, and defaults are built in.
      folio_deb_root="$PWD/dist/deb/folio"
      rm -rf "$PWD/dist/deb"
      mkdir -p "$folio_deb_root/DEBIAN" "$folio_deb_root/usr/lib/folio" "$folio_deb_root/usr/bin" \
        "$folio_deb_root/usr/share/applications" "$folio_deb_root/usr/share/doc/folio/licenses"
      install -m 0755 desktop/target/release/folio "$folio_deb_root/usr/lib/folio/folio"
      install -m 0644 runtime/lib/libpdfium.so "$folio_deb_root/usr/lib/folio/libpdfium.so"
      ln -s ../lib/folio/folio "$folio_deb_root/usr/bin/folio"
      install -m 0644 folio.toml "$folio_deb_root/usr/share/doc/folio/folio.toml.example"
      install -m 0644 LICENSE "$folio_deb_root/usr/share/doc/folio/copyright"
      cp -R runtime/licenses/. "$folio_deb_root/usr/share/doc/folio/licenses/"
      install -m 0644 runtime/LICENSE "$folio_deb_root/usr/share/doc/folio/licenses/PDFium-LICENSE"
      install -m 0644 THIRD_PARTY.md "$folio_deb_root/usr/share/doc/folio/licenses/THIRD_PARTY.md"
      chmod -R u+rwX,go+rX,go-w "$folio_deb_root/usr/share/doc/folio"
      cat > "$folio_deb_root/usr/share/applications/folio.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Folio
Comment=LaTeX editor with a live PDF preview
Exec=folio %F
Terminal=false
Categories=Office;Publishing;TextEditor;
MimeType=text/x-tex;
DESKTOP
      cat > "$folio_deb_root/DEBIAN/control" <<CONTROL
Package: folio
Version: $folio_package_version
Architecture: $folio_deb_arch
Maintainer: Aleksei Tiulpin <aleksei@tiulpin.ai>
Section: editors
Priority: optional
Depends: libc6 (>= 2.39), libgcc-s1, libxkbcommon0, libwayland-client0, libwayland-cursor0, libegl1,
 libx11-6, libx11-xcb1, libxcursor1, libxrandr2, libxi6
Recommends: latexmk, texlive-latex-recommended, texlive-extra-utils, xdg-desktop-portal, mesa-vulkan-drivers
Suggests: texlive-xetex, biber
Installed-Size: $(du -sk "$folio_deb_root/usr" | cut -f1)
Description: LaTeX editor with a live PDF preview
 Folio edits LaTeX projects and shows the compiled PDF beside the source.
 TeX is not bundled; install a TeX distribution with latexmk and synctex.
CONTROL
      folio_deb="$PWD/dist/folio_${folio_file_version}_${folio_deb_arch}.deb"
      dpkg-deb --root-owner-group -Zxz --build "$folio_deb_root" "$folio_deb"
      rm -rf "$PWD/dist/deb"
      printf 'Debian package: %s\n' "$folio_deb"
    else
      echo 'dpkg-deb not found; skipped the Debian package.'
    fi
    ;;
  *) echo 'Supported platforms: macOS and Linux' >&2; exit 1 ;;
esac
