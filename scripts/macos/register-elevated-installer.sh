#!/bin/sh
set -eu

LABEL="us.beyondstudios.beyondremote.elevated-installer"
ROOT="/Library/Application Support/BeyondRemote"
RUNNER="$ROOT/elevated-update.sh"
PENDING="$ROOT/pending-update.txt"
PLIST="/Library/LaunchDaemons/$LABEL.plist"
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"

sudo mkdir -p "$ROOT"
sudo cp "$SCRIPT_DIR/elevated-update.sh" "$RUNNER"
sudo touch "$PENDING"
sudo chown root:wheel "$RUNNER" "$PLIST" 2>/dev/null || true
sudo chmod 755 "$RUNNER"
sudo chmod 666 "$PENDING"

sudo tee "$PLIST" >/dev/null <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>$LABEL</string>
  <key>ProgramArguments</key>
  <array>
    <string>$RUNNER</string>
  </array>
  <key>WatchPaths</key>
  <array>
    <string>$PENDING</string>
  </array>
  <key>StandardOutPath</key>
  <string>$ROOT/elevated-update.log</string>
  <key>StandardErrorPath</key>
  <string>$ROOT/elevated-update.log</string>
</dict>
</plist>
PLIST

sudo chown root:wheel "$PLIST"
sudo chmod 644 "$PLIST"
sudo launchctl bootout system "$PLIST" 2>/dev/null || true
sudo launchctl bootstrap system "$PLIST"
echo "Registered LaunchDaemon: $LABEL"
