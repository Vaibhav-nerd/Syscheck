#!/usr/bin/env bash
# Build fully static (musl) binaries + optional AppImage packaging.
#
# Requirements:
#   - rustup with the x86_64-unknown-linux-musl target (added automatically)
#   - musl-gcc:  sudo apt-get install musl-tools    (needed by ring/TLS)
#   - for AppImage: wget access to github.com; FUSE not required
#     (appimagetool runs via --appimage-extract-and-run)
set -euo pipefail
cd "$(dirname "$0")"

export PATH="$HOME/.cargo/bin:$PATH"
TARGET="x86_64-unknown-linux-musl"

# 1. Toolchain + target
if ! command -v cargo >/dev/null; then
    echo "cargo not found - install rustup first: https://rustup.rs" >&2
    exit 1
fi
rustup target add "$TARGET" >/dev/null

# 2. Static release build
echo "==> Building static binary..."
cargo build --release --target "$TARGET"
BIN="target/$TARGET/release/syscheck"

if command -v ldd >/dev/null && ldd "$BIN" 2>&1 | grep -q "lib"; then
    echo "WARNING: binary is dynamically linked:" >&2
    ldd "$BIN" >&2
fi

# 3. Copy artifact
mkdir -p dist
cp "$BIN" dist/syscheck
echo "==> Static binary: dist/syscheck ($(du -h "$BIN" | cut -f1))"
file "$BIN" | sed 's/^/    /'

# 4. Optional AppImage
APPDIR="dist/syscheck.AppDir"
ARCH="$(uname -m)"
OUT="dist/syscheck-${ARCH}.AppImage"
if command -v appimagetool >/dev/null 2>&1; then
    TOOL="appimagetool"
    EXTRACT=()
else
    TOOL="dist/appimagetool-${ARCH}.AppImage"
    EXTRACT=(--appimage-extract-and-run)
    if [ ! -f "$TOOL" ]; then
        echo "==> Downloading appimagetool..."
        wget -q -O "$TOOL" \
            "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-${ARCH}.AppImage"
        chmod +x "$TOOL"
    fi
fi

echo "==> Assembling AppImage..."
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" \
         "$APPDIR/usr/share/applications" \
         "$APPDIR/usr/share/icons/hicolor/128x128/apps"
cp dist/syscheck "$APPDIR/usr/bin/syscheck"
cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/syscheck" "$@"
EOF
chmod +x "$APPDIR/AppRun"
cp packaging/syscheck.desktop "$APPDIR/syscheck.desktop"  # appimagetool looks at root
cp packaging/syscheck.desktop "$APPDIR/usr/share/applications/"
cp packaging/syscheck.png "$APPDIR/usr/share/icons/hicolor/128x128/apps/"
cp packaging/syscheck.png "$APPDIR/syscheck.png"  # AppImage top-level icon

"$TOOL" "${EXTRACT[@]}" "$APPDIR" "$OUT"
echo "==> AppImage: $OUT ($(du -h "$OUT" | cut -f1))"
echo "All done. Copy dist/syscheck (any Linux, x86_64) or the AppImage (desktop) anywhere."
