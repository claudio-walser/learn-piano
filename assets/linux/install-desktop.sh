#!/bin/sh
# Installs OpenEtude into the app menu of the current user (icon in the dock / window list),
# for the binary built in this checkout. Remove with: install-desktop.sh --uninstall
set -eu
ID=org.openetude.OpenEtude
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
DATA=${XDG_DATA_HOME:-$HOME/.local/share}
APPS=$DATA/applications
ICONS=$DATA/icons/hicolor/scalable/apps

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$APPS/$ID.desktop" "$ICONS/$ID.svg"
    echo "OpenEtude removed from the app menu."
    exit 0
fi

BIN=$ROOT/target/release/openetude
[ -x "$BIN" ] || { echo "$BIN not found, build it first: cargo build --release" >&2; exit 1; }
mkdir -p "$APPS" "$ICONS"
cp "$ROOT/assets/icons/openetude.svg" "$ICONS/$ID.svg"
# Run from the checkout so the bundled library and sound are found.
sed -e "s|^Exec=.*|Exec=\"$BIN\"|" -e "/^Exec=/a Path=$ROOT" "$HERE/$ID.desktop" > "$APPS/$ID.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$APPS" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$DATA/icons/hicolor" 2>/dev/null || true
echo "OpenEtude installed in the app menu ($APPS/$ID.desktop)."
