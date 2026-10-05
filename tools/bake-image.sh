#!/usr/bin/env bash
# ==============================================================================
# bake-image.sh: Pre-Bake Standalone EV3 Web Motor Appliance Image
# ==============================================================================
# Usage:
#   sudo ./tools/bake-image.sh [options]
#
# Options:
#   -s, --ssid, -ssid <SSID>        Pre-configure default Wi-Fi network name
#   -p, --password, -ssidpw <PASS>  Pre-configure Wi-Fi passphrase (8-63 chars)
#   -b, --binary <PATH>             Path to custom ev3-web-motor binary
#   -h, --help                      Show this help message
#
# Examples:
#   # Bake image with pre-configured Wi-Fi (boots directly to Ready URL):
#   sudo ./tools/bake-image.sh --ssid "MyHomeNetwork" --password "SecretPass123"
#
#   # Bake image without pre-configured Wi-Fi (uses on-brick setup):
#   sudo ./tools/bake-image.sh
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

BINARY_PATH="${WORKSPACE_DIR}/target/armv5te-unknown-linux-musleabi/release/ev3-web-motor"
SERVICE_FILE="${WORKSPACE_DIR}/ev3-web.service"
BUILD_DIR="${WORKSPACE_DIR}/build_image"
OUTPUT_IMG_XZ="${WORKSPACE_DIR}/ev3-web-motor-ready.img.xz"

WIFI_SSID=""
WIFI_PASS=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        -s|--ssid|-ssid)
            WIFI_SSID="$2"
            shift 2
            ;;
        -p|--password|--pass|-ssidpw)
            WIFI_PASS="$2"
            shift 2
            ;;
        -b|--binary)
            BINARY_PATH="$2"
            shift 2
            ;;
        -h|--help)
            sed -n '2,18p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        *)
            # If positional argument and binary path doesn't exist yet, treat as binary
            if [[ -f "$1" ]]; then
                BINARY_PATH="$1"
                shift
            else
                echo "Unknown option: $1"
                echo "Run with -h or --help for usage."
                exit 1
            fi
            ;;
    esac
done

BASE_ZIP_URL="https://github.com/ev3dev/ev3dev/releases/download/ev3dev-stretch-2020-04-10/ev3dev-stretch-ev3-generic-2020-04-10.zip"
BASE_ZIP_FILE="${BUILD_DIR}/ev3dev-stretch-ev3-generic-2020-04-10.zip"
RAW_IMG_FILE="${BUILD_DIR}/ev3dev.img"
MOUNT_DIR="${BUILD_DIR}/mnt_root"

echo "============================================================"
echo "   EV3 Web Motor Control - Appliance Disk Image Baker       "
echo "============================================================"
if [[ -n "${WIFI_SSID}" ]]; then
    echo " Pre-Baked Wi-Fi SSID: '${WIFI_SSID}'"
fi
echo "============================================================"

# 1. Validation
if [[ $EUID -ne 0 ]]; then
    echo "ERROR: This script must be run as root (or via sudo) to mount disk images."
    exit 1
fi

if [[ -n "${WIFI_SSID}" ]]; then
    if [[ -z "${WIFI_PASS}" || ${#WIFI_PASS} -lt 8 || ${#WIFI_PASS} -gt 63 ]]; then
        echo "ERROR: Wi-Fi passphrase must be between 8 and 63 characters long."
        exit 1
    fi
fi

if [[ ! -f "${BINARY_PATH}" ]]; then
    echo "ERROR: Binary not found at: ${BINARY_PATH}"
    echo "Please build first with: cargo build --target armv5te-unknown-linux-musleabi --release"
    exit 1
fi

if [[ ! -f "${SERVICE_FILE}" ]]; then
    echo "ERROR: Service file not found at: ${SERVICE_FILE}"
    exit 1
fi

mkdir -p "${BUILD_DIR}" "${MOUNT_DIR}"

# 2. Download base image if missing
if [[ ! -f "${BASE_ZIP_FILE}" ]]; then
    echo "[1/6] Downloading official ev3dev-stretch base image..."
    curl -L --fail --progress-bar "${BASE_ZIP_URL}" -o "${BASE_ZIP_FILE}"
else
    echo "[1/6] Using cached ev3dev base zip: ${BASE_ZIP_FILE}"
fi

# 3. Extract base image
echo "[2/6] Extracting raw disk image..."
unzip -p "${BASE_ZIP_FILE}" "*.img" > "${RAW_IMG_FILE}"

# 4. Calculate partition 2 offset (ext4 root)
echo "[3/6] Calculating ext4 partition offset..."
SECTOR_SIZE=$(fdisk -l "${RAW_IMG_FILE}" | grep -i "Units: sectors of" | awk '{print $8}')
if [[ -z "${SECTOR_SIZE}" ]]; then
    SECTOR_SIZE=512
fi

START_SECTOR=$(fdisk -l "${RAW_IMG_FILE}" | grep -E "${RAW_IMG_FILE}2|img2" | awk '{print $2}')
if [[ -z "${START_SECTOR}" ]]; then
    echo "ERROR: Failed to locate partition 2 start sector."
    exit 1
fi

OFFSET=$(( START_SECTOR * SECTOR_SIZE ))
echo "      Sector Size: ${SECTOR_SIZE} bytes"
echo "      Start Sector: ${START_SECTOR}"
echo "      Mount Offset: ${OFFSET} bytes"

# 5. Loopback Mount
echo "[4/6] Mounting root filesystem..."
mount -o loop,offset="${OFFSET}" "${RAW_IMG_FILE}" "${MOUNT_DIR}"

cleanup() {
    echo "Cleaning up mount point..."
    if mountpoint -q "${MOUNT_DIR}"; then
        umount "${MOUNT_DIR}" || true
    fi
}
trap cleanup EXIT

# 6. Inject files, pre-configure Wi-Fi, and configure services
echo "[5/6] Injecting binary and configuring systemd services..."

# Copy binary
cp "${BINARY_PATH}" "${MOUNT_DIR}/usr/local/bin/ev3-web-motor"
chmod 0755 "${MOUNT_DIR}/usr/local/bin/ev3-web-motor"
chown 0:0 "${MOUNT_DIR}/usr/local/bin/ev3-web-motor"

# Copy and enable ev3-web.service
cp "${SERVICE_FILE}" "${MOUNT_DIR}/etc/systemd/system/ev3-web.service"
chmod 0644 "${MOUNT_DIR}/etc/systemd/system/ev3-web.service"
chown 0:0 "${MOUNT_DIR}/etc/systemd/system/ev3-web.service"

mkdir -p "${MOUNT_DIR}/etc/systemd/system/multi-user.target.wants"
ln -sf /etc/systemd/system/ev3-web.service "${MOUNT_DIR}/etc/systemd/system/multi-user.target.wants/ev3-web.service"

# Pre-configure ConnMan Wi-Fi if provided
if [[ -n "${WIFI_SSID}" ]]; then
    echo "      Pre-configuring ConnMan Wi-Fi for SSID: '${WIFI_SSID}'..."
    mkdir -p "${MOUNT_DIR}/var/lib/connman"
    cat <<EOF > "${MOUNT_DIR}/var/lib/connman/ev3_wifi.config"
[service_ev3_wifi]
Type = wifi
Name = ${WIFI_SSID}
Passphrase = ${WIFI_PASS}
EOF
    chmod 0600 "${MOUNT_DIR}/var/lib/connman/ev3_wifi.config"
    chown 0:0 "${MOUNT_DIR}/var/lib/connman/ev3_wifi.config"
fi

# Disable brickman to free 18 MB RAM and reserve /dev/tty1 for Rust UI
rm -f "${MOUNT_DIR}/etc/systemd/system/multi-user.target.wants/brickman.service"
ln -sf /dev/null "${MOUNT_DIR}/etc/systemd/system/brickman.service"

# Mask getty on tty1 to prevent terminal login contention
ln -sf /dev/null "${MOUNT_DIR}/etc/systemd/system/getty@tty1.service"

# Mask slow blocking wait-online and daily apt services
ln -sf /dev/null "${MOUNT_DIR}/etc/systemd/system/connman-wait-online.service"
ln -sf /dev/null "${MOUNT_DIR}/etc/systemd/system/systemd-networkd-wait-online.service"
ln -sf /dev/null "${MOUNT_DIR}/etc/systemd/system/apt-daily.service"
ln -sf /dev/null "${MOUNT_DIR}/etc/systemd/system/apt-daily.timer"
ln -sf /dev/null "${MOUNT_DIR}/etc/systemd/system/apt-daily-upgrade.service"
ln -sf /dev/null "${MOUNT_DIR}/etc/systemd/system/apt-daily-upgrade.timer"

sync

echo "      Unmounting root filesystem..."
umount "${MOUNT_DIR}"
trap - EXIT

# 7. Compress into .img.xz
echo "[6/6] Compressing disk image with xz (multi-threaded)..."
xz -9 -T0 -c "${RAW_IMG_FILE}" > "${OUTPUT_IMG_XZ}"

IMAGE_SIZE=$(du -h "${OUTPUT_IMG_XZ}" | awk '{print $1}')
echo ""
echo "============================================================"
echo " SUCCESS! Appliance image ready: ${OUTPUT_IMG_XZ} (${IMAGE_SIZE})"
echo "============================================================"
echo " Next steps:"
echo " 1. Flash this .img.xz to your MicroSD card using BalenaEtcher."
echo " 2. Insert card into EV3 and boot."
if [[ -n "${WIFI_SSID}" ]]; then
    echo " 3. Wi-Fi will connect automatically to '${WIFI_SSID}'."
else
    echo " 3. Connect to Wi-Fi using the on-brick screen and buttons."
fi
echo " 4. Open http://<ip>/ in your browser."
echo "============================================================"
