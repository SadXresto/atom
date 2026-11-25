#!/usr/bin/env bash
# Build ATOM's Linux desktop installers (.deb + AppImage) from source.
#
# Requirements:
#   - Rust (https://rustup.rs), Bun (https://bun.sh)
#   - libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev
#     librsvg2-dev patchelf  (Debian/Ubuntu)
#   - Fedora: webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel
#     librsvg2-devel patchelf
#
# Usage: scripts/build-desktop.sh [deb,appimage,rpm]
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bundles="${1:-deb,appimage}"

if ! pkg-config --exists webkit2gtk-4.1; then
  echo "error: WebKitGTK 4.1 development files are missing." >&2
  echo "       Ubuntu/Debian: sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf" >&2
  exit 1
fi

echo "==> Building the interface"
cd "$root/frontend"
bun install --frozen-lockfile
bun run build

echo "==> Compiling the desktop app and bundling: $bundles"
cd "$root/apps/desktop"
npx --yes @tauri-apps/cli@2 build --bundles "$bundles"

echo
echo "Artifacts:"
find "$root/apps/desktop/target/release/bundle" -maxdepth 3 -type f \
  \( -name '*.deb' -o -name '*.AppImage' -o -name '*.rpm' \) -printf '  %p\n'
