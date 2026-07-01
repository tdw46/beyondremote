#!/bin/sh
set -eu

ROOT="/var/lib/beyondremote"
RUNNER="/usr/local/lib/beyondremote/elevated-update.sh"
PENDING="$ROOT/pending-update.txt"
SERVICE="/etc/systemd/system/beyondremote-elevated-installer.service"
PATH_UNIT="/etc/systemd/system/beyondremote-elevated-installer.path"
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"

sudo mkdir -p "$ROOT" "$(dirname "$RUNNER")"
sudo cp "$SCRIPT_DIR/elevated-update.sh" "$RUNNER"
sudo touch "$PENDING"
sudo chmod 755 "$RUNNER"
sudo chmod 666 "$PENDING"

sudo tee "$SERVICE" >/dev/null <<SERVICE
[Unit]
Description=BeyondRemote elevated installer

[Service]
Type=oneshot
ExecStart=$RUNNER
SERVICE

sudo tee "$PATH_UNIT" >/dev/null <<PATHUNIT
[Unit]
Description=Watch for BeyondRemote pending elevated updates

[Path]
PathChanged=$PENDING

[Install]
WantedBy=multi-user.target
PATHUNIT

sudo systemctl daemon-reload
sudo systemctl enable --now beyondremote-elevated-installer.path
echo "Registered systemd path: beyondremote-elevated-installer.path"
