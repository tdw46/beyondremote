#!/bin/sh
set -eu

ROOT="/var/lib/beyondremote"
PENDING="$ROOT/pending-update.txt"
LOG="$ROOT/elevated-update.log"
INSTALL_DIR="/opt/beyondremote"

mkdir -p "$ROOT"
exec >>"$LOG" 2>&1

[ -f "$PENDING" ] || exit 0
UPDATE_PATH="$(tr -d '\r\n' < "$PENDING")"
[ -n "$UPDATE_PATH" ] || exit 0
[ -e "$UPDATE_PATH" ] || exit 1

if command -v systemctl >/dev/null 2>&1; then
    systemctl stop beyondremote.service 2>/dev/null || true
fi

case "$UPDATE_PATH" in
    *.deb)
        dpkg -i "$UPDATE_PATH"
        ;;
    *.AppImage)
        mkdir -p "$INSTALL_DIR"
        cp "$UPDATE_PATH" "$INSTALL_DIR/BeyondRemote.AppImage"
        chmod 755 "$INSTALL_DIR/BeyondRemote.AppImage"
        ;;
    *)
        if [ -d "$UPDATE_PATH" ]; then
            rm -rf "$INSTALL_DIR.new"
            mkdir -p "$INSTALL_DIR.new"
            cp -a "$UPDATE_PATH/." "$INSTALL_DIR.new/"
            rm -rf "$INSTALL_DIR"
            mv "$INSTALL_DIR.new" "$INSTALL_DIR"
        else
            echo "Unsupported update path: $UPDATE_PATH"
            exit 1
        fi
        ;;
esac

: > "$PENDING"
if command -v systemctl >/dev/null 2>&1; then
    systemctl start beyondremote.service 2>/dev/null || true
fi

APP_BIN=""
if [ -x "$INSTALL_DIR/BeyondRemote.AppImage" ]; then
    APP_BIN="$INSTALL_DIR/BeyondRemote.AppImage"
elif [ -x "$INSTALL_DIR/BeyondRemote" ]; then
    APP_BIN="$INSTALL_DIR/BeyondRemote"
elif command -v beyondremote >/dev/null 2>&1; then
    APP_BIN="$(command -v beyondremote)"
elif command -v rustdesk >/dev/null 2>&1; then
    APP_BIN="$(command -v rustdesk)"
fi

if [ -n "$APP_BIN" ]; then
    if command -v loginctl >/dev/null 2>&1; then
        loginctl list-users --no-legend 2>/dev/null | while read -r USER_ID USER_NAME REST; do
            [ -n "$USER_ID" ] || continue
            RUNTIME="/run/user/$USER_ID"
            [ -d "$RUNTIME" ] || continue
            su - "$USER_NAME" -c "XDG_RUNTIME_DIR='$RUNTIME' DISPLAY='${DISPLAY:-:0}' nohup '$APP_BIN' --tray >/dev/null 2>&1 &" || true
        done
    else
        nohup "$APP_BIN" --tray >/dev/null 2>&1 || true
    fi
fi
echo "BeyondRemote elevated update complete."
