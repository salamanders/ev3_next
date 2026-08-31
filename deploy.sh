#!/usr/bin/env bash
# ============================================================
# EV3 Web Motor Control - 1-Click Deployment Script (Bash)
# ============================================================
set -euo pipefail

TARGET_IP="${1:-192.168.2.2}"
USER="${2:-robot}"

echo "==> [1/4] Cross-compiling ev3-web-motor for ARMv5te..."
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if command -v cargo-zigbuild &> /dev/null; then
    cargo zigbuild --target armv5te-unknown-linux-musleabi --release
elif command -v cross &> /dev/null; then
    cross build --target armv5te-unknown-linux-musleabi --release
else
    chmod +x "$SCRIPT_DIR/zig-lld-arm.sh" 2>/dev/null || true
    RUSTFLAGS="-C linker=$SCRIPT_DIR/zig-lld-arm.sh" cargo build --target armv5te-unknown-linux-musleabi --release
fi

BINARY_PATH="target/armv5te-unknown-linux-musleabi/release/ev3-web-motor"

if [ ! -f "$BINARY_PATH" ]; then
    echo "[FATAL] Binary not found at $BINARY_PATH"
    exit 1
fi

echo "==> [2/4] Verifying EV3 connectivity at $TARGET_IP..."
ping -c 1 -W 2 "$TARGET_IP" || true

echo "==> [3/4] Uploading binary to EV3 via SCP..."
ssh -o StrictHostKeyChecking=no "$USER@$TARGET_IP" "sudo systemctl stop ev3-web.service 2>/dev/null || true"
scp -o StrictHostKeyChecking=no "$BINARY_PATH" "$USER@$TARGET_IP:/home/robot/ev3-web-motor"

echo "==> [4/4] Setting permissions and restarting service on EV3..."
ssh -o StrictHostKeyChecking=no "$USER@$TARGET_IP" "sudo chmod +x /home/robot/ev3-web-motor && sudo systemctl restart ev3-web.service"

echo "============================================================"
echo "  SUCCESS! EV3 Web Control is LIVE at http://$TARGET_IP/    "
echo "============================================================"
