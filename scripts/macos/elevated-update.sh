#!/bin/sh
set -eu

ROOT="/Library/Application Support/BeyondRemote"
PENDING="$ROOT/pending-update.txt"
LOG="$ROOT/elevated-update.log"

mkdir -p "$ROOT"
exec >>"$LOG" 2>&1

[ -f "$PENDING" ] || exit 0
UPDATE_PATH="$(tr -d '\r\n' < "$PENDING")"
[ -n "$UPDATE_PATH" ] || exit 0
[ -e "$UPDATE_PATH" ] || exit 1

install_app() {
    SRC_APP="$1"
    DST_APP="/Applications/BeyondRemote.app"
    rm -rf "$DST_APP"
    ditto "$SRC_APP" "$DST_APP"
    chown -R root:wheel "$DST_APP"
}

case "$UPDATE_PATH" in
    *.dmg)
        MOUNT="$(mktemp -d /tmp/beyondremote-dmg.XXXXXX)"
        hdiutil attach "$UPDATE_PATH" -mountpoint "$MOUNT" -nobrowse -readonly
        SRC_APP="$(find "$MOUNT" -maxdepth 2 -name 'BeyondRemote.app' -type d | head -n 1)"
        [ -n "$SRC_APP" ] || { hdiutil detach "$MOUNT"; exit 1; }
        install_app "$SRC_APP"
        hdiutil detach "$MOUNT"
        ;;
    *.app)
        install_app "$UPDATE_PATH"
        ;;
    *)
        if [ -d "$UPDATE_PATH/BeyondRemote.app" ]; then
            install_app "$UPDATE_PATH/BeyondRemote.app"
        else
            echo "Unsupported update path: $UPDATE_PATH"
            exit 1
        fi
        ;;
esac

: > "$PENDING"
CONSOLE_UID="$(stat -f '%u' /dev/console 2>/dev/null || true)"
if [ -n "$CONSOLE_UID" ] && [ "$CONSOLE_UID" != "0" ]; then
    launchctl asuser "$CONSOLE_UID" open "/Applications/BeyondRemote.app" || true
else
    open "/Applications/BeyondRemote.app" || true
fi
echo "BeyondRemote elevated update complete."
