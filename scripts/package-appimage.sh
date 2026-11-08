#!/usr/bin/env bash
# Builds a release binary and packages it as an AppImage.
#
# Usage: ./scripts/package-appimage.sh [version]
# Requires: rust toolchain, curl, linuxdeploy and appimagetool (downloaded on
# first run into .tools/). The engines (ffmpeg, LibreOffice, pandoc) are NOT
# bundled; the setup wizard installs them from the system repositories.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -n1)}"
BIN="$ROOT/target/release/donustur"
APPDIR="$ROOT/AppDir"
DIST="$ROOT/dist"
TOOLS="$ROOT/.tools"
ARCH="${ARCH:-x86_64}"

mkdir -p "$DIST" "$TOOLS"

echo "==> Derleniyor (release)"
cargo build --release --manifest-path "$ROOT/Cargo.toml"

if [ ! -x "$TOOLS/linuxdeploy-$ARCH.AppImage" ]; then
    echo "==> linuxdeploy indiriliyor"
    curl -fL --retry 3 -o "$TOOLS/linuxdeploy-$ARCH.AppImage" \
        "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-$ARCH.AppImage"
    chmod +x "$TOOLS/linuxdeploy-$ARCH.AppImage"
fi

if [ ! -x "$TOOLS/appimagetool-$ARCH.AppImage" ]; then
    echo "==> appimagetool indiriliyor"
    curl -fL --retry 3 -o "$TOOLS/appimagetool-$ARCH.AppImage" \
        "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage"
    chmod +x "$TOOLS/appimagetool-$ARCH.AppImage"
fi

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin"
cp "$BIN" "$APPDIR/usr/bin/donustur"
install -Dm644 "$ROOT/assets/donustur.desktop" "$APPDIR/usr/share/applications/donustur.desktop"
install -Dm644 "$ROOT/assets/donustur.png" "$APPDIR/usr/share/icons/hicolor/256x256/apps/donustur.png"
install -Dm644 "$ROOT/assets/donustur.png" "$APPDIR/donustur.png"
install -Dm644 "$ROOT/assets/donustur.desktop" "$APPDIR/donustur.desktop"

export APPIMAGE_EXTRACT_AND_RUN=1
"$TOOLS/linuxdeploy-$ARCH.AppImage" \
    --appdir "$APPDIR" \
    -e "$BIN" \
    -i "$ROOT/assets/donustur.png" \
    -d "$ROOT/assets/donustur.desktop"

OUT="$DIST/Donustur-$VERSION-$ARCH.AppImage"
"$TOOLS/appimagetool-$ARCH.AppImage" "$APPDIR" "$OUT"

echo "==> Hazır: $OUT"
