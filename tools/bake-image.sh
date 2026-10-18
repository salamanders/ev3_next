#!/usr/bin/env bash
# ==============================================================================
# bake-image.sh: Pre-Bake Standalone EV3 Web Motor Appliance Image
# ==============================================================================
#   Downloads the official ev3dev-stretch disk image once, mounts its ext4 root
#   partition via loopback, injects the compiled ev3-web-motor binary, systemd
#   service, and Wi-Fi credentials, and creates a block map (.bmap) for fast SD
#   card flashing.
#
#   Allows building and inspecting the disk image (or creating a compressed
#   .img.xz release artifact with --xz) without a physical MicroSD card plugged
#   in. Note: flash-image.sh calls this script automatically by default.
#
# Usage:
#   sudo ./tools/bake-image.sh [options]
#
# Options:
#   -s, --ssid <SSID>        Pre-configure default Wi-Fi network name (overrides wifi.txt)
#   -p, --password <PASS>    Pre-configure Wi-Fi passphrase (8-63 chars, overrides wifi.txt)
#   -b, --binary <PATH>      Path to custom ev3-web-motor binary
#   --xz                     Also compress output into ev3-web-motor-ready.img.xz for releases
#   -f, --flash [DEVICE]     Flash to MicroSD card immediately after baking (auto-detects SD card)
#   -y, --yes                Skip confirmation prompt when used with --flash
#   -h, --help               Show this help message
#
# Wi-Fi Auto-Discovery:
#   If --ssid is not passed and ./wifi.txt exists in the repository root, line 1
#   is used as the SSID and line 2 is used as the passphrase.
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

BINARY_PATH="${WORKSPACE_DIR}/target/armv5te-unknown-linux-musleabi/release/ev3-web-motor"
SERVICE_FILE="${WORKSPACE_DIR}/ev3-web.service"
BUILD_DIR="${WORKSPACE_DIR}/build_image"
RAW_IMG_FILE="${BUILD_DIR}/ev3dev.img"
BMAP_FILE="${BUILD_DIR}/ev3dev.img.bmap"
OUTPUT_IMG_XZ="${WORKSPACE_DIR}/ev3-web-motor-ready.img.xz"
LOCAL_WIFI_TXT="${WORKSPACE_DIR}/wifi.txt"

WIFI_SSID=""
WIFI_PASS=""
BUILD_XZ=false
DO_FLASH=false
FLASH_DEV=""
FLASH_YES=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        -s|--ssid)
            WIFI_SSID="$2"
            shift 2
            ;;
        -p|--password)
            WIFI_PASS="$2"
            shift 2
            ;;
        -b|--binary)
            BINARY_PATH="$2"
            shift 2
            ;;
        --xz)
            BUILD_XZ=true
            shift
            ;;
        -f|--flash)
            DO_FLASH=true
            if [[ $# -gt 1 && "$2" != -* ]]; then
                FLASH_DEV="$2"
                shift 2
            else
                shift
            fi
            ;;
        -y|--yes)
            FLASH_YES=true
            shift
            ;;
        -h|--help)
            sed -n '2,29p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Run with -h or --help for usage."
            exit 1
            ;;
    esac
done

# Auto-load Wi-Fi credentials from workspace ./wifi.txt if --ssid was not passed
if [[ -z "${WIFI_SSID}" && -f "${LOCAL_WIFI_TXT}" ]]; then
    WIFI_SSID="$(sed -n '1p' "${LOCAL_WIFI_TXT}" | tr -d '\r')"
    WIFI_PASS="$(sed -n '2p' "${LOCAL_WIFI_TXT}" | tr -d '\r')"
    echo "Loaded Wi-Fi credentials from ${LOCAL_WIFI_TXT} (SSID: '${WIFI_SSID}')."
fi

BASE_ZIP_URL="https://github.com/ev3dev/ev3dev/releases/download/ev3dev-stretch-2020-04-10/ev3dev-stretch-ev3-generic-2020-04-10.zip"
BASE_ZIP_FILE="${BUILD_DIR}/ev3dev-stretch-ev3-generic-2020-04-10.zip"
MOUNT_DIR="${BUILD_DIR}/mnt_root"

echo "============================================================"
echo "   EV3 Web Motor Control - Appliance Disk Image Baker       "
echo "============================================================"
if [[ -n "${WIFI_SSID}" ]]; then
    echo " Pre-Baked Wi-Fi SSID: '${WIFI_SSID}'"
else
    echo " Pre-Baked Wi-Fi SSID: (none - create ./wifi.txt or pass --ssid)"
fi
echo "============================================================"

# 1. Validation
if [[ $EUID -ne 0 ]]; then
    echo "ERROR: This script must be run as root (or via sudo) to mount disk images."
    exit 1
fi

if [[ -z "${WIFI_SSID}" && -n "${WIFI_PASS}" ]]; then
    echo "ERROR: Wi-Fi password provided without --ssid."
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

cleanup() {
    echo "Cleaning up mount points and temporary files..."
    if mountpoint -q "${MOUNT_DIR}"; then
        umount "${MOUNT_DIR}" 2>/dev/null || umount -l "${MOUNT_DIR}" 2>/dev/null || true
    fi
    if [[ -n "${TMP_IMG:-}" && -f "${TMP_IMG}" ]]; then
        rm -f "${TMP_IMG}" 2>/dev/null || true
    fi
    if [[ -n "${TMP_OUTPUT_XZ:-}" && -f "${TMP_OUTPUT_XZ}" ]]; then
        rm -f "${TMP_OUTPUT_XZ}" 2>/dev/null || true
    fi
}
trap cleanup EXIT

# 2. Download base image if missing
if [[ ! -f "${BASE_ZIP_FILE}" ]]; then
    echo "[1/6] Downloading official ev3dev-stretch base image..."
    curl -L --fail --progress-bar "${BASE_ZIP_URL}" -o "${BASE_ZIP_FILE}"
else
    echo "[1/6] Using cached ev3dev base zip: ${BASE_ZIP_FILE}"
fi

# 3. Always extract clean base image from zip
echo "[2/6] Extracting clean raw disk image from zip..."
TMP_IMG="${RAW_IMG_FILE}.tmp.$$"
rm -f "${TMP_IMG}" "${RAW_IMG_FILE}"
unzip -p "${BASE_ZIP_FILE}" "*.img" > "${TMP_IMG}"
mv -f "${TMP_IMG}" "${RAW_IMG_FILE}"
unset TMP_IMG

# Helper: Find first numeric start sector after partition identifier, ignoring boot flag (*)
get_partition_start_sector() {
    local img="$1"
    local part_num="$2"
    if command -v partx >/dev/null 2>&1; then
        local start
        start=$(partx -g -o START -n "${part_num}" "${img}" 2>/dev/null | tr -d ' ' || true)
        if [[ -n "${start}" && "${start}" =~ ^[0-9]+$ ]]; then
            echo "${start}"
            return 0
        fi
    fi
    local base
    base="$(basename "${img}")"
    fdisk -l "${img}" | grep -E "(${img}|${base})p?${part_num}([[:space:]]+|\*)" | awk '{ for (i=2; i<=NF; i++) if ($i ~ /^[0-9]+$/) { print $i; exit } }'
}

# 4. Calculate partition 2 offset (ext4 root)
echo "[3/6] Calculating ext4 partition offset..."
SECTOR_SIZE=$(fdisk -l "${RAW_IMG_FILE}" | awk -F'[ =]+' '/Units: sectors of/ { for (i=NF; i>=1; i--) if ($i ~ /^[0-9]+$/) { print $i; exit } }')
if [[ -z "${SECTOR_SIZE}" || "${SECTOR_SIZE}" -lt 512 ]]; then
    SECTOR_SIZE=512
fi

START_SECTOR=$(get_partition_start_sector "${RAW_IMG_FILE}" 2)
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

# 6. Inject files, pre-configure Wi-Fi, and configure services
echo "[5/6] Injecting binary and configuring systemd services..."

# Copy binary to /usr/local/bin and link from /home/robot
cp "${BINARY_PATH}" "${MOUNT_DIR}/usr/local/bin/ev3-web-motor"
chmod 0755 "${MOUNT_DIR}/usr/local/bin/ev3-web-motor"
chown 0:0 "${MOUNT_DIR}/usr/local/bin/ev3-web-motor"
mkdir -p "${MOUNT_DIR}/home/robot"
ln -sf /usr/local/bin/ev3-web-motor "${MOUNT_DIR}/home/robot/ev3-web-motor"
chown -h 1000:1000 "${MOUNT_DIR}/home/robot/ev3-web-motor" 2>/dev/null || true

# Copy web_assets (index.html, style.css, app.js, and widgets/*.js) into /home/robot/web_assets
mkdir -p "${MOUNT_DIR}/home/robot/web_assets/widgets"
cp -f "${WORKSPACE_DIR}/web_assets/"*.html "${WORKSPACE_DIR}/web_assets/"*.css "${WORKSPACE_DIR}/web_assets/"*.js "${MOUNT_DIR}/home/robot/web_assets/"
cp -f "${WORKSPACE_DIR}/web_assets/widgets/"*.js "${MOUNT_DIR}/home/robot/web_assets/widgets/"
chmod 0755 "${MOUNT_DIR}/home/robot/web_assets" "${MOUNT_DIR}/home/robot/web_assets/widgets"
chown -R 1000:1000 "${MOUNT_DIR}/home/robot/web_assets" 2>/dev/null || true

# Copy and enable ev3-web.service
cp "${SERVICE_FILE}" "${MOUNT_DIR}/etc/systemd/system/ev3-web.service"
chmod 0644 "${MOUNT_DIR}/etc/systemd/system/ev3-web.service"
chown 0:0 "${MOUNT_DIR}/etc/systemd/system/ev3-web.service"

mkdir -p "${MOUNT_DIR}/etc/systemd/system/multi-user.target.wants"
ln -sf /etc/systemd/system/ev3-web.service "${MOUNT_DIR}/etc/systemd/system/multi-user.target.wants/ev3-web.service"

# Force default systemd target to multi-user (text console) instead of graphical.target
ln -sf /lib/systemd/system/multi-user.target "${MOUNT_DIR}/etc/systemd/system/default.target"

# Configure ConnMan to power on Wi-Fi technology by default without overwriting existing settings
mkdir -p "${MOUNT_DIR}/var/lib/connman"
CONNMAN_SETTINGS="${MOUNT_DIR}/var/lib/connman/settings"

if [[ ! -f "${CONNMAN_SETTINGS}" ]]; then
    cat <<EOF > "${CONNMAN_SETTINGS}"
[global]
OfflineMode=false

[WiFi]
Enable=true
Tethering=false
EOF
else
    # Ensure OfflineMode=false under [global]
    if grep -q "^\[global\]" "${CONNMAN_SETTINGS}"; then
        if grep -q "^OfflineMode=" "${CONNMAN_SETTINGS}"; then
            sed -i 's/^OfflineMode=.*/OfflineMode=false/' "${CONNMAN_SETTINGS}"
        else
            sed -i '/^\[global\]/a OfflineMode=false' "${CONNMAN_SETTINGS}"
        fi
    else
        sed -i '1i [global]\nOfflineMode=false\n' "${CONNMAN_SETTINGS}"
    fi

    # Ensure [WiFi] section exists with Enable=true
    if grep -q "^\[WiFi\]" "${CONNMAN_SETTINGS}"; then
        if awk '/^\[WiFi\]/{flag=1;next}/^\[/{flag=0}flag && /^Enable=/{found=1}END{exit !found}' "${CONNMAN_SETTINGS}"; then
            sed -i '/^\[WiFi\]/,/^\[/ s/^Enable=.*/Enable=true/' "${CONNMAN_SETTINGS}"
        else
            sed -i '/^\[WiFi\]/a Enable=true' "${CONNMAN_SETTINGS}"
        fi
    else
        cat <<EOF >> "${CONNMAN_SETTINGS}"

[WiFi]
Enable=true
Tethering=false
EOF
    fi
fi
chmod 0600 "${CONNMAN_SETTINGS}"
chown 0:0 "${CONNMAN_SETTINGS}"

# Pre-configure ConnMan Wi-Fi if provided (matches WifiManager::format_connman_config in src/sysfs/wifi.rs)
if [[ -n "${WIFI_SSID}" ]]; then
    echo "      Pre-configuring ConnMan Wi-Fi for SSID: '${WIFI_SSID}'..."
    cat <<EOF > "${MOUNT_DIR}/var/lib/connman/ev3_wifi.config"
[global]
Name = EV3_WiFi
Description = EV3 Auto-Connect Wi-Fi

[service_ev3_wifi]
Type = wifi
Name = ${WIFI_SSID}
Passphrase = ${WIFI_PASS}
IPv4 = dhcp
AutoConnect = true
EOF
    chmod 0600 "${MOUNT_DIR}/var/lib/connman/ev3_wifi.config"
    chown 0:0 "${MOUNT_DIR}/var/lib/connman/ev3_wifi.config"

    # Also write fallback wifi.txt for Rust WifiManager
    cat <<EOF > "${MOUNT_DIR}/home/robot/wifi.txt"
${WIFI_SSID}
${WIFI_PASS}
EOF
    chmod 0644 "${MOUNT_DIR}/home/robot/wifi.txt"
    cp "${MOUNT_DIR}/home/robot/wifi.txt" "${MOUNT_DIR}/wifi.txt"
fi

# Disable brickman completely: remove all target wants, mask service, and disable executable
rm -f "${MOUNT_DIR}/etc/systemd/system/multi-user.target.wants/brickman.service"
rm -f "${MOUNT_DIR}/etc/systemd/system/graphical.target.wants/brickman.service"
rm -f "${MOUNT_DIR}/etc/systemd/system/default.target.wants/brickman.service"
rm -f "${MOUNT_DIR}/lib/systemd/system/graphical.target.wants/brickman.service"
rm -f "${MOUNT_DIR}/lib/systemd/system/multi-user.target.wants/brickman.service"
ln -sf /dev/null "${MOUNT_DIR}/etc/systemd/system/brickman.service"

# Check brickman.service ExecStart in /lib/systemd/system if present
if [[ -f "${MOUNT_DIR}/lib/systemd/system/brickman.service" ]]; then
    BRICKMAN_EXEC=$(grep -E "^ExecStart=" "${MOUNT_DIR}/lib/systemd/system/brickman.service" | head -n1 | cut -d= -f2- | awk '{print $1}')
    BRICKMAN_EXEC="${BRICKMAN_EXEC#[-@!+]}"
    if [[ -n "${BRICKMAN_EXEC}" && ( -e "${MOUNT_DIR}${BRICKMAN_EXEC}" || -L "${MOUNT_DIR}${BRICKMAN_EXEC}" ) ]]; then
        echo "      Disabling brickman service executable at ${BRICKMAN_EXEC}..."
        mv "${MOUNT_DIR}${BRICKMAN_EXEC}" "${MOUNT_DIR}${BRICKMAN_EXEC}.disabled"
    fi
fi

# Also check standard binary paths
for b_path in "${MOUNT_DIR}/usr/sbin/brickman" "${MOUNT_DIR}/usr/bin/brickman"; do
    if [[ -e "${b_path}" || -L "${b_path}" ]]; then
        echo "      Disabling brickman binary at ${b_path#${MOUNT_DIR}}..."
        mv "${b_path}" "${b_path}.disabled"
    fi
done

# Mask getty on tty1 to prevent terminal login contention
ln -sf /dev/null "${MOUNT_DIR}/etc/systemd/system/getty@tty1.service"
rm -f "${MOUNT_DIR}/etc/systemd/system/getty.target.wants/getty@tty1.service"

# Mask slow blocking wait-online and daily apt services
ln -sf /dev/null "${MOUNT_DIR}/etc/systemd/system/connman-wait-online.service"
ln -sf /dev/null "${MOUNT_DIR}/etc/systemd/system/systemd-networkd-wait-online.service"
ln -sf /dev/null "${MOUNT_DIR}/etc/systemd/system/apt-daily.service"
ln -sf /dev/null "${MOUNT_DIR}/etc/systemd/system/apt-daily.timer"
ln -sf /dev/null "${MOUNT_DIR}/etc/systemd/system/apt-daily-upgrade.service"
ln -sf /dev/null "${MOUNT_DIR}/etc/systemd/system/apt-daily-upgrade.timer"

# Configure readable 8x16 console font (Lat15-TerminusBold16 gives 22x8 grid)
if [[ -f "${MOUNT_DIR}/etc/default/console-setup" ]]; then
    sed -i "s/^FONT=.*/FONT='Lat15-TerminusBold16.psf.gz'/" "${MOUNT_DIR}/etc/default/console-setup"
    sed -i "s/^FONTFACE=.*/FONTFACE=\"TerminusBold\"/" "${MOUNT_DIR}/etc/default/console-setup"
    sed -i "s/^FONTSIZE=.*/FONTSIZE=\"8x16\"/" "${MOUNT_DIR}/etc/default/console-setup"
fi

# Update fake-hwclock data with bake timestamp to avoid 2020 clock skew
echo "      Updating fake-hwclock timestamp to bake time..."
mkdir -p "${MOUNT_DIR}/etc"
date -u '+%Y-%m-%d %H:%M:%S' > "${MOUNT_DIR}/etc/fake-hwclock.data"
chmod 0644 "${MOUNT_DIR}/etc/fake-hwclock.data"

# Update root CA certificates bundle from build host if available
if [[ -f "/etc/ssl/certs/ca-certificates.crt" ]]; then
    echo "      Updating root CA certificates bundle from build host..."
    mkdir -p "${MOUNT_DIR}/etc/ssl/certs"
    cp "/etc/ssl/certs/ca-certificates.crt" "${MOUNT_DIR}/etc/ssl/certs/ca-certificates.crt"
    chmod 0644 "${MOUNT_DIR}/etc/ssl/certs/ca-certificates.crt"
fi

# Verification checks before unmount
echo "      Verifying modifications in ext4 root..."
[[ -f "${MOUNT_DIR}/etc/fake-hwclock.data" ]] || { echo "ERROR: fake-hwclock.data missing"; exit 1; }
[[ -x "${MOUNT_DIR}/usr/local/bin/ev3-web-motor" ]] || { echo "ERROR: Binary missing or not executable"; exit 1; }
[[ -d "${MOUNT_DIR}/home/robot/web_assets/widgets" ]] || { echo "ERROR: Baseline web_assets/widgets missing"; exit 1; }
[[ -f "${MOUNT_DIR}/etc/systemd/system/ev3-web.service" ]] || { echo "ERROR: ev3-web.service missing"; exit 1; }

# Verify service symlinks are active and point to existing units
[[ -L "${MOUNT_DIR}/etc/systemd/system/multi-user.target.wants/ev3-web.service" ]] || { echo "ERROR: ev3-web.service symlink missing"; exit 1; }
SERVICE_TARGET=$(readlink "${MOUNT_DIR}/etc/systemd/system/multi-user.target.wants/ev3-web.service")
[[ -f "${MOUNT_DIR}${SERVICE_TARGET}" ]] || { echo "ERROR: ev3-web.service symlink target missing: ${SERVICE_TARGET}"; exit 1; }

# Verify /home/robot/ev3-web-motor symlink points to executable
[[ -L "${MOUNT_DIR}/home/robot/ev3-web-motor" ]] || { echo "ERROR: /home/robot/ev3-web-motor symlink missing"; exit 1; }
ROBOT_BIN_TARGET=$(readlink "${MOUNT_DIR}/home/robot/ev3-web-motor")
[[ -x "${MOUNT_DIR}${ROBOT_BIN_TARGET}" ]] || { echo "ERROR: /home/robot/ev3-web-motor target missing or not executable: ${ROBOT_BIN_TARGET}"; exit 1; }

# Verify default target points to multi-user
[[ "$(readlink "${MOUNT_DIR}/etc/systemd/system/default.target")" == *multi-user.target ]] || { echo "ERROR: default.target not redirected to multi-user.target"; exit 1; }

# Verify brickman is masked to /dev/null
[[ "$(readlink "${MOUNT_DIR}/etc/systemd/system/brickman.service")" == "/dev/null" ]] || { echo "ERROR: brickman.service not masked to /dev/null"; exit 1; }

# Verify brickman binary is not executable
[[ ! -e "${MOUNT_DIR}/usr/sbin/brickman" && ! -L "${MOUNT_DIR}/usr/sbin/brickman" && ! -e "${MOUNT_DIR}/usr/bin/brickman" && ! -L "${MOUNT_DIR}/usr/bin/brickman" ]] || { echo "ERROR: brickman binary still present"; exit 1; }

# Verify ConnMan settings
[[ -f "${MOUNT_DIR}/var/lib/connman/settings" ]] || { echo "ERROR: ConnMan settings missing"; exit 1; }
awk '/^\[WiFi\]/{flag=1;next}/^\[/{flag=0}flag && /^Enable=true/{found=1}END{exit !found}' "${MOUNT_DIR}/var/lib/connman/settings" || { echo "ERROR: ConnMan Wi-Fi technology not enabled in [WiFi] section"; exit 1; }

if [[ -n "${WIFI_SSID}" ]]; then
    [[ -f "${MOUNT_DIR}/var/lib/connman/ev3_wifi.config" ]] || { echo "ERROR: ConnMan Wi-Fi config missing"; exit 1; }
    [[ -f "${MOUNT_DIR}/home/robot/wifi.txt" ]] || { echo "ERROR: Fallback /home/robot/wifi.txt missing"; exit 1; }
    [[ -f "${MOUNT_DIR}/wifi.txt" ]] || { echo "ERROR: Fallback /wifi.txt missing"; exit 1; }
else
    [[ ! -f "${MOUNT_DIR}/var/lib/connman/ev3_wifi.config" ]] || { echo "ERROR: Stale ConnMan Wi-Fi config found"; exit 1; }
    [[ ! -f "${MOUNT_DIR}/home/robot/wifi.txt" ]] || { echo "ERROR: Stale /home/robot/wifi.txt found"; exit 1; }
    [[ ! -f "${MOUNT_DIR}/wifi.txt" ]] || { echo "ERROR: Stale /wifi.txt found"; exit 1; }
fi
[[ -f "${MOUNT_DIR}/usr/share/consolefonts/Lat15-TerminusBold16.psf.gz" ]] || { echo "ERROR: Font file /usr/share/consolefonts/Lat15-TerminusBold16.psf.gz missing"; exit 1; }
if [[ -f "${MOUNT_DIR}/etc/default/console-setup" ]]; then
    grep -q "Lat15-TerminusBold16" "${MOUNT_DIR}/etc/default/console-setup" || { echo "ERROR: Console font not configured in console-setup"; exit 1; }
fi
echo "      All verification checks passed!"

sync

echo "      Unmounting root filesystem..."
umount "${MOUNT_DIR}" || umount -l "${MOUNT_DIR}"

# 7. Generate bmap file (skips unallocated blocks during flash) and optionally compress .xz
if command -v bmaptool >/dev/null 2>&1; then
    echo "[6/6] Generating block map (.bmap) for fast flashing..."
    rm -f "${BMAP_FILE}"
    bmaptool create -o "${BMAP_FILE}" "${RAW_IMG_FILE}"
else
    echo "[6/6] Skipping .bmap generation (install 'bmap-tools' for 2-3x faster SD flashing)."
    rm -f "${BMAP_FILE}"
fi

if [[ "${BUILD_XZ}" == "true" ]]; then
    echo "      Compressing release disk image with xz (multi-threaded)..."
    TMP_OUTPUT_XZ="${OUTPUT_IMG_XZ}.tmp.$$"
    rm -f "${TMP_OUTPUT_XZ}"
    xz -9 -T0 -c "${RAW_IMG_FILE}" > "${TMP_OUTPUT_XZ}"
    mv -f "${TMP_OUTPUT_XZ}" "${OUTPUT_IMG_XZ}"
    unset TMP_OUTPUT_XZ
fi

IMAGE_SIZE=$(du -h "${RAW_IMG_FILE}" | awk '{print $1}')
echo ""
echo "============================================================"
echo " SUCCESS! Raw appliance image ready: ${RAW_IMG_FILE} (${IMAGE_SIZE})"
if [[ -f "${BMAP_FILE}" ]]; then
    echo " Block map ready:                    ${BMAP_FILE}"
fi
if [[ "${BUILD_XZ}" == "true" ]]; then
    echo " Compressed release archive ready:   ${OUTPUT_IMG_XZ}"
fi
echo "============================================================"

# Disarm cleanup trap on successful completion
trap - EXIT

if [[ "${DO_FLASH}" == "true" ]]; then
    FLASH_ARGS=(--no-bake)
    [[ "${FLASH_YES}" == "true" ]] && FLASH_ARGS+=(-y)
    [[ -n "${FLASH_DEV}" ]] && FLASH_ARGS+=("${FLASH_DEV}")
    exec "${SCRIPT_DIR}/flash-image.sh" "${FLASH_ARGS[@]}"
fi

