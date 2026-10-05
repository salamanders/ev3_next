#!/usr/bin/env bash
# ============================================================
# EV3 Web Motor Control - 1-Click Deployment Script (Bash)
# ============================================================
set -euo pipefail

TARGET_IP="${1:-192.168.2.2}"
USER="${2:-robot}"

echo "==> [1/4] Cross-compiling ev3-web-motor for ARMv5te..."
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
chmod +x "$SCRIPT_DIR/zig-lld-arm.sh"
CARGO_TARGET_ARMV5TE_UNKNOWN_LINUX_MUSLEABI_LINKER="$SCRIPT_DIR/zig-lld-arm.sh" cargo build --target armv5te-unknown-linux-musleabi --release

BINARY_PATH="target/armv5te-unknown-linux-musleabi/release/ev3-web-motor"

if [ ! -f "$BINARY_PATH" ]; then
    echo "[FATAL] Binary not found at $BINARY_PATH"
    exit 1
fi

echo "==> [2/4] Verifying EV3 connectivity at $TARGET_IP..."
if ! ping -c 1 -W 2 "$TARGET_IP" >/dev/null 2>&1; then
    echo "[WARN] Ping to $TARGET_IP timed out; proceeding to SSH..."
fi

echo "==> [3/4] Uploading binary to EV3 via SCP (/tmp staging)..."
scp -o StrictHostKeyChecking=no "$BINARY_PATH" "$USER@$TARGET_IP:/tmp/ev3-web-motor.new"

echo "==> [4/4] Installing binary and restarting service on EV3..."
ssh -o StrictHostKeyChecking=no "$USER@$TARGET_IP" "sudo mv /tmp/ev3-web-motor.new /home/robot/ev3-web-motor && sudo chmod +x /home/robot/ev3-web-motor && sudo systemctl restart ev3-web.service"

echo "==> Verifying service status..."
if ssh -o StrictHostKeyChecking=no "$USER@$TARGET_IP" "systemctl is-active --quiet ev3-web.service"; then
    echo "============================================================"
    echo "  SUCCESS! EV3 Web Control is LIVE at http://$TARGET_IP/    "
    echo "============================================================"
else
    echo "[ERROR] ev3-web.service failed to enter active state."
    exit 1
fi
