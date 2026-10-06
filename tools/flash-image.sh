#!/usr/bin/env bash
# ==============================================================================
# flash-image.sh: Command-Line Flash & Verification Tool for MicroSD Cards
# ==============================================================================
# Usage:
#   sudo ./tools/flash-image.sh [options] <TARGET_DEVICE> [IMAGE_PATH]
#
# Options:
#   -v, --verify-only    Skip flashing; only verify an existing card
#   -y, --yes            Skip confirmation prompt (for non-interactive use)
#   --allow-internal     Allow flashing non-removable internal drives (RM=0)
#   --allow-large-disk   Allow flashing drives larger than 64 GB
#   -h, --help           Show this help message
#
# Examples:
#   # Flash appliance image to /dev/sdX and verify (prompts for confirmation):
#   sudo ./tools/flash-image.sh /dev/sdX
#
#   # Flash specific image file with automatic confirmation:
#   sudo ./tools/flash-image.sh -y /dev/sdX ./ev3-web-motor-ready.img.xz
#
#   # Verify an already written MicroSD card without re-flashing:
#   sudo ./tools/flash-image.sh --verify-only /dev/sdX
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

DEFAULT_IMAGE="${WORKSPACE_DIR}/ev3-web-motor-ready.img.xz"
VERIFY_ONLY=false
ASSUME_YES=false
ALLOW_INTERNAL=false
ALLOW_LARGE_DISK=false
TARGET_DEV=""
IMAGE_PATH=""

# Parse options
while [[ $# -gt 0 ]]; do
    case "$1" in
        -v|--verify-only)
            VERIFY_ONLY=true
            shift
            ;;
        -y|--yes)
            ASSUME_YES=true
            shift
            ;;
        --allow-internal)
            ALLOW_INTERNAL=true
            shift
            ;;
        --allow-large-disk)
            ALLOW_LARGE_DISK=true
            shift
            ;;
        -h|--help)
            sed -n '2,24p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        -*)
            echo "ERROR: Unknown option: $1"
            echo "Run with -h or --help for usage."
            exit 1
            ;;
        *)
            if [[ -z "${TARGET_DEV}" ]]; then
                TARGET_DEV="$1"
            elif [[ -z "${IMAGE_PATH}" ]]; then
                IMAGE_PATH="$1"
            else
                echo "ERROR: Unexpected argument: $1"
                exit 1
            fi
            shift
            ;;
    esac
done

if [[ -z "${TARGET_DEV}" ]]; then
    echo "ERROR: Target block device required (for example: /dev/sdX or /dev/mmcblk0)."
    echo "Usage: sudo ./tools/flash-image.sh [options] <TARGET_DEVICE> [IMAGE_PATH]"
    exit 1
fi

if [[ -z "${IMAGE_PATH}" ]]; then
    IMAGE_PATH="${DEFAULT_IMAGE}"
fi

# 1. Root check
if [[ $EUID -ne 0 ]]; then
    echo "ERROR: This script must be run as root (or via sudo)."
    exit 1
fi

TARGET_DEV="$(readlink -f "${TARGET_DEV}")"
TARGET_REAL="${TARGET_DEV}"

# 2. Block device validation
if [[ ! -b "${TARGET_DEV}" ]]; then
    echo "ERROR: '${TARGET_DEV}' is not a valid block device."
    echo "Use 'lsblk' to identify your target MicroSD card device."
    exit 1
fi

# Check that user specified whole disk, not partition
DEV_TYPE="$(lsblk -dno TYPE "${TARGET_DEV}" 2>/dev/null || true)"
if [[ "${DEV_TYPE}" == "part" ]] || [[ "${TARGET_DEV}" =~ [0-9]$ && "${TARGET_DEV}" =~ (sd[a-z][0-9]+|nvme[0-9]+n[0-9]+p[0-9]+|mmcblk[0-9]+p[0-9]+|loop[0-9]+p[0-9]+)$ ]]; then
    echo "ERROR: '${TARGET_DEV}' appears to be a partition, not a whole disk."
    echo "Specify the whole drive (for example: /dev/sdc instead of /dev/sdc1)."
    exit 1
fi

# Gather block device metadata
DEV_SIZE_BYTES=$(lsblk -bno SIZE "${TARGET_DEV}" 2>/dev/null | head -n1 || echo "0")
DEV_SIZE_HUMAN=$(lsblk -dno SIZE "${TARGET_DEV}" 2>/dev/null | head -n1 || echo "unknown")
DEV_RM=$(lsblk -dno RM "${TARGET_DEV}" 2>/dev/null | head -n1 || echo "0")
DEV_MODEL=$(lsblk -dno MODEL "${TARGET_DEV}" 2>/dev/null | head -n1 | xargs || echo "Generic")
DEV_TRAN=$(lsblk -dno TRAN "${TARGET_DEV}" 2>/dev/null | head -n1 || echo "unknown")
DEV_VENDOR=$(lsblk -dno VENDOR "${TARGET_DEV}" 2>/dev/null | head -n1 | xargs || echo "")

# Safety Check 1: Host SWAP partition protection
if [[ -f /proc/swaps ]]; then
    while IFS= read -r swap_dev; do
        real_swap_dev="$(readlink -f "${swap_dev}" 2>/dev/null || echo "${swap_dev}")"
        if [[ -n "${real_swap_dev}" && ( "${real_swap_dev}" == "${TARGET_REAL}" || "${real_swap_dev}" =~ ^${TARGET_REAL}(p?[0-9]+)$ ) ]]; then
            echo "FATAL: '${TARGET_DEV}' (partition '${real_swap_dev}') is currently used as active host SWAP."
            echo "Operation aborted to prevent host system damage."
            exit 1
        fi
    done < <(awk 'NR>1 {print $1}' /proc/swaps 2>/dev/null || true)
fi

# Safety Check 2: Critical host filesystem mounts protection
is_critical_host_mount() {
    local pt="$1"
    case "${pt}" in
        /|/boot|/boot/*|/efi|/efi/*|/home|/home/*|/root|/root/*|/etc|/etc/*|/var|/var/*|/usr|/usr/*|/srv|/srv/*|/opt|/opt/*)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

TARGET_MOUNTS=()
if command -v lsblk >/dev/null 2>&1; then
    while IFS= read -r mnt; do
        [[ -n "${mnt}" && "${mnt}" != "[SWAP]" ]] && TARGET_MOUNTS+=("${mnt}")
    done < <(lsblk -nro MOUNTPOINT "${TARGET_REAL}" 2>/dev/null | grep -v '^$' || true)
fi

if [[ -f /proc/mounts ]]; then
    while IFS= read -r mnt_entry; do
        m_dev="$(awk '{print $1}' <<<"${mnt_entry}")"
        m_pt="$(awk '{print $2}' <<<"${mnt_entry}")"
        m_real="$(readlink -f "${m_dev}" 2>/dev/null || echo "${m_dev}")"
        if [[ -n "${m_real}" && ( "${m_real}" == "${TARGET_REAL}" || "${m_real}" =~ ^${TARGET_REAL}(p?[0-9]+)$ ) ]]; then
            if [[ -n "${m_pt}" ]]; then
                decoded_pt="$(printf '%b' "${m_pt}")"
                TARGET_MOUNTS+=("${decoded_pt}")
            fi
        fi
    done < /proc/mounts
fi

for mnt in "${TARGET_MOUNTS[@]}"; do
    if is_critical_host_mount "${mnt}"; then
        echo "FATAL: '${TARGET_DEV}' contains an active host system mountpoint: '${mnt}'."
        echo "Operation aborted to prevent damage."
        exit 1
    fi
done

# Safety Check 3: Removable drive check (blocks internal SATA / NVMe drives)
if [[ "${DEV_RM}" != "1" && "${ALLOW_INTERNAL}" != "true" ]]; then
    echo "FATAL: '${TARGET_DEV}' is marked as a non-removable internal drive (RM=${DEV_RM}, Transport: ${DEV_TRAN})."
    echo "MicroSD cards in card readers are reported as removable (RM=1)."
    echo "To protect your computer's internal storage, non-removable drives are blocked."
    echo "If you are certain this is the correct drive, pass --allow-internal."
    echo "Operation aborted."
    exit 1
fi

# Safety Check 4: Size sanity ceiling (EV3 SD cards are 4 GB to 32 GB; maximum 64 GB)
MAX_SD_BYTES=68719476736 # 64 GiB
if (( DEV_SIZE_BYTES > MAX_SD_BYTES )) && [[ "${ALLOW_LARGE_DISK}" != "true" ]]; then
    echo "FATAL: '${TARGET_DEV}' size is ${DEV_SIZE_HUMAN} (${DEV_SIZE_BYTES} bytes)."
    echo "This exceeds the 64 GB limit for EV3 MicroSD cards."
    echo "To prevent overwriting external hard drives or large storage disks, this write is blocked."
    echo "If you are certain this is the correct drive, pass --allow-large-disk."
    echo "Operation aborted."
    exit 1
fi

echo "============================================================"
echo "   EV3 Appliance Image Flasher & Verification Tool          "
echo "============================================================"
echo " Target Device: ${TARGET_DEV}"
echo " Device Model:  ${DEV_VENDOR} ${DEV_MODEL}"
echo " Transport:     ${DEV_TRAN} (Removable: ${DEV_RM})"
echo " Device Size:   ${DEV_SIZE_HUMAN}"
if [[ "${VERIFY_ONLY}" == "true" ]]; then
    echo " Mode:          Verify Only (no flash)"
else
    echo " Image Source:  ${IMAGE_PATH}"
fi
echo "============================================================"

# 3. Flashing (if not verify-only)
if [[ "${VERIFY_ONLY}" != "true" ]]; then
    if [[ ! -f "${IMAGE_PATH}" ]]; then
        echo "ERROR: Disk image file not found at: ${IMAGE_PATH}"
        echo "Please bake the image first with: sudo ./tools/bake-image.sh"
        exit 1
    fi

    if [[ "${IMAGE_PATH}" == *.zip ]]; then
        echo "ERROR: Zip archives are not supported for flashing. Please provide an uncompressed raw (.img) or .xz image."
        exit 1
    fi

    # Interactive confirmation prompt
    if [[ "${ASSUME_YES}" != "true" ]]; then
        echo ""
        echo " [!] WARNING: ALL PARTITIONS AND DATA ON ${TARGET_DEV} WILL BE PERMANENTLY ERASED!"
        echo ""
        if [[ ! -t 0 ]] && [[ ! -r /dev/tty ]]; then
            echo "ERROR: Non-interactive terminal detected. Use -y or --yes to confirm flashing."
            exit 1
        fi

        CONFIRM=""
        if [[ -r /dev/tty ]]; then
            read -r -p "Type 'yes' to proceed with overwriting ${TARGET_DEV}: " CONFIRM < /dev/tty
        else
            read -r -p "Type 'yes' to proceed with overwriting ${TARGET_DEV}: " CONFIRM
        fi

        if [[ "${CONFIRM}" != "yes" ]]; then
            echo "Flashing aborted by user."
            exit 0
        fi
    fi

    # Unmount remaining automounted partitions on target (such as /media desktop mounts)
    echo "[1/3] Unmounting existing partitions on ${TARGET_DEV}..."
    if command -v lsblk >/dev/null 2>&1; then
        while IFS= read -r mnt; do
            if [[ -n "${mnt}" && "${mnt}" != "[SWAP]" ]]; then
                echo "      Unmounting ${mnt}..."
                if ! umount "${mnt}"; then
                    echo "ERROR: Failed to unmount ${mnt} on ${TARGET_DEV}."
                    exit 1
                fi
            fi
        done < <(lsblk -nro MOUNTPOINT "${TARGET_DEV}" 2>/dev/null | grep -v '^$' || true)
    fi

    # Check /proc/mounts to catch any mounts not reported by lsblk
    if [[ -f /proc/mounts ]]; then
        while IFS= read -r mnt_entry; do
            mnt_dev="$(awk '{print $1}' <<<"${mnt_entry}")"
            mnt_pt="$(awk '{print $2}' <<<"${mnt_entry}")"
            real_mnt_dev="$(readlink -f "${mnt_dev}" 2>/dev/null || echo "${mnt_dev}")"
            if [[ "${real_mnt_dev}" == "${TARGET_REAL}" || "${real_mnt_dev}" =~ ^${TARGET_REAL}(p?[0-9]+)$ ]]; then
                if [[ -n "${mnt_pt}" ]]; then
                    decoded_mnt_pt="$(printf '%b' "${mnt_pt}")"
                    echo "      Unmounting ${decoded_mnt_pt} (${mnt_dev}) found in /proc/mounts..."
                    if ! umount "${decoded_mnt_pt}"; then
                        echo "ERROR: Failed to unmount ${decoded_mnt_pt} on ${TARGET_DEV}."
                        exit 1
                    fi
                fi
            fi
        done < /proc/mounts
    fi

    # Final safety check: ensure no partition on TARGET_REAL remains mounted anywhere
    if [[ -f /proc/mounts ]]; then
        while IFS= read -r mnt_entry; do
            mnt_dev="$(awk '{print $1}' <<<"${mnt_entry}")"
            real_mnt_dev="$(readlink -f "${mnt_dev}" 2>/dev/null || echo "${mnt_dev}")"
            if [[ "${real_mnt_dev}" == "${TARGET_REAL}" || "${real_mnt_dev}" =~ ^${TARGET_REAL}(p?[0-9]+)$ ]]; then
                echo "ERROR: Partition '${mnt_dev}' on '${TARGET_DEV}' is still mounted. Aborting write."
                exit 1
            fi
        done < /proc/mounts
    fi

    if [[ -f /proc/swaps ]]; then
        while IFS= read -r swap_dev; do
            real_swap_dev="$(readlink -f "${swap_dev}" 2>/dev/null || echo "${swap_dev}")"
            if [[ "${real_swap_dev}" == "${TARGET_REAL}" || "${real_swap_dev}" =~ ^${TARGET_REAL}(p?[0-9]+)$ ]]; then
                echo "ERROR: Partition '${swap_dev}' on '${TARGET_DEV}' is still active swap. Aborting write."
                exit 1
            fi
        done < <(awk 'NR>1 {print $1}' /proc/swaps 2>/dev/null || true)
    fi

    echo "[2/3] Writing image to ${TARGET_DEV} via command line..."
    if [[ "${IMAGE_PATH}" == *.xz ]]; then
        xzcat "${IMAGE_PATH}" | dd of="${TARGET_DEV}" bs=4M status=progress conv=fsync
    else
        dd if="${IMAGE_PATH}" of="${TARGET_DEV}" bs=4M status=progress conv=fsync
    fi
    sync
    echo "      Write completed and synced to hardware."
else
    echo "[1/3] Skipping write step (verify-only mode requested)."
fi

# 4. Partition detection
echo "[3/3] Verifying custom image installation on ${TARGET_DEV}..."
echo "      Refreshing partition table..."
partprobe "${TARGET_DEV}" 2>/dev/null || blockdev --rereadpt "${TARGET_DEV}" 2>/dev/null || sleep 2
udevadm settle 2>/dev/null || true

# Determine partition 2 naming
if [[ "${TARGET_DEV}" =~ [0-9]$ ]]; then
    PART2_DEV="${TARGET_DEV}p2"
else
    PART2_DEV="${TARGET_DEV}2"
fi

# Wait for partition 2 device node to settle
WAIT_COUNT=0
while [[ ! -b "${PART2_DEV}" && ${WAIT_COUNT} -lt 5 ]]; do
    sleep 1
    WAIT_COUNT=$(( WAIT_COUNT + 1 ))
done

if [[ ! -b "${PART2_DEV}" ]]; then
    echo "ERROR: Partition 2 device node (${PART2_DEV}) not found."
    exit 1
fi

# 5. Mount partition 2 and run verification checks
CHECK_MNT="$(mktemp -d /tmp/ev3_sd_check_XXXXXX)"
cleanup() {
    if mountpoint -q "${CHECK_MNT}"; then
        umount "${CHECK_MNT}" 2>/dev/null || umount -l "${CHECK_MNT}" 2>/dev/null || true
    fi
    rm -rf "${CHECK_MNT}"
}
trap cleanup EXIT

echo "      Mounting ${PART2_DEV} read-only..."
mount -o ro "${PART2_DEV}" "${CHECK_MNT}"

echo "      Performing post-write verification checks:"

# Check 1: Server binary
if [[ -x "${CHECK_MNT}/usr/local/bin/ev3-web-motor" ]]; then
    echo "      [PASS] /usr/local/bin/ev3-web-motor exists and is executable."
else
    echo "      [FAIL] /usr/local/bin/ev3-web-motor missing or not executable."
    exit 1
fi

# Check 2: Service file
if [[ -f "${CHECK_MNT}/etc/systemd/system/ev3-web.service" ]]; then
    echo "      [PASS] /etc/systemd/system/ev3-web.service exists."
else
    echo "      [FAIL] /etc/systemd/system/ev3-web.service missing."
    exit 1
fi

# Check 3: ev3-web.service enabled and valid
if [[ -L "${CHECK_MNT}/etc/systemd/system/multi-user.target.wants/ev3-web.service" ]]; then
    SERVICE_TARGET="$(readlink "${CHECK_MNT}/etc/systemd/system/multi-user.target.wants/ev3-web.service" || true)"
    if [[ -f "${CHECK_MNT}${SERVICE_TARGET}" ]]; then
        echo "      [PASS] ev3-web.service is enabled in multi-user.target.wants (points to ${SERVICE_TARGET})."
    else
        echo "      [FAIL] ev3-web.service symlink target missing: ${SERVICE_TARGET}."
        exit 1
    fi
else
    echo "      [FAIL] ev3-web.service is not enabled in multi-user.target.wants."
    exit 1
fi

# Check 3b: Robot home binary symlink
if [[ -L "${CHECK_MNT}/home/robot/ev3-web-motor" ]]; then
    ROBOT_TARGET="$(readlink "${CHECK_MNT}/home/robot/ev3-web-motor" || true)"
    if [[ -x "${CHECK_MNT}${ROBOT_TARGET}" ]]; then
        echo "      [PASS] /home/robot/ev3-web-motor symlink points to executable (${ROBOT_TARGET})."
    else
        echo "      [FAIL] /home/robot/ev3-web-motor target missing or not executable: ${ROBOT_TARGET}."
        exit 1
    fi
else
    echo "      [FAIL] /home/robot/ev3-web-motor symlink missing."
    exit 1
fi

# Check 4: Brickman unit masked
BRICKMAN_LINK="$(readlink "${CHECK_MNT}/etc/systemd/system/brickman.service" || true)"
if [[ "${BRICKMAN_LINK}" == "/dev/null" ]]; then
    echo "      [PASS] brickman.service is masked to /dev/null."
else
    echo "      [FAIL] brickman.service is not masked (points to '${BRICKMAN_LINK}')."
    exit 1
fi

# Check 5: Default systemd target
TARGET_LINK="$(readlink "${CHECK_MNT}/etc/systemd/system/default.target" || true)"
if [[ "${TARGET_LINK}" == *multi-user.target ]]; then
    echo "      [PASS] default.target points to multi-user.target."
else
    echo "      [FAIL] default.target points to '${TARGET_LINK}' instead of multi-user.target."
    exit 1
fi

# Check 6: Brickman binary neutralized
if [[ ! -e "${CHECK_MNT}/usr/sbin/brickman" && ! -L "${CHECK_MNT}/usr/sbin/brickman" && ! -e "${CHECK_MNT}/usr/bin/brickman" && ! -L "${CHECK_MNT}/usr/bin/brickman" ]]; then
    echo "      [PASS] Brickman binary neutralized (not executable)."
else
    echo "      [FAIL] Brickman executable still present in /usr/sbin or /usr/bin."
    exit 1
fi

# Check 7: ConnMan Wi-Fi technology enabled in [WiFi] section
if [[ -f "${CHECK_MNT}/var/lib/connman/settings" ]] && awk '/^\[WiFi\]/{flag=1;next}/^\[/{flag=0}flag && /^Enable=true/{found=1}END{exit !found}' "${CHECK_MNT}/var/lib/connman/settings"; then
    echo "      [PASS] ConnMan Wi-Fi radio enabled by default."
else
    echo "      [FAIL] ConnMan settings missing or Wi-Fi radio not enabled."
    exit 1
fi

# Check 7b: Console font
if [[ -f "${CHECK_MNT}/usr/share/consolefonts/Lat15-TerminusBold16.psf.gz" ]]; then
    echo "      [PASS] Console font Lat15-TerminusBold16.psf.gz present."
else
    echo "      [FAIL] Console font Lat15-TerminusBold16.psf.gz missing."
    exit 1
fi

# Check 8: Pre-configured Wi-Fi credentials (if present)
if [[ -f "${CHECK_MNT}/var/lib/connman/ev3_wifi.config" ]]; then
    SSID_NAME="$(grep -E "^Name = " "${CHECK_MNT}/var/lib/connman/ev3_wifi.config" | head -n1 | cut -d= -f2- | tr -d ' ' || true)"
    echo "      [INFO] Pre-configured Wi-Fi found for SSID: '${SSID_NAME}'."
fi

# Unmount cleanly
umount "${CHECK_MNT}"
trap - EXIT
rm -rf "${CHECK_MNT}"

echo ""
echo "============================================================"
echo " SUCCESS! Custom image verified on ${TARGET_DEV}."
echo "============================================================"
echo " The MicroSD card is ready."
echo " 1. Safely remove the MicroSD card from your computer."
echo " 2. Insert into LEGO Mindstorms EV3 brick and power on."
echo " 3. EV3 will boot directly into custom web motor mode."
echo "============================================================"
