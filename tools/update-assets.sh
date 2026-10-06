#!/bin/bash
set -e

ASSETS_DIR="${WEB_ASSETS_DIR:-/home/robot/web_assets}"
TMP_DIR="${ASSETS_DIR}.tmp"
VERSION_FILE="${ASSETS_DIR}/.version"
GITHUB_API="${GITHUB_API:-https://api.github.com/repos/salamanders/ev3_next/commits/main}"
RAW_BASE="${RAW_BASE:-https://raw.githubusercontent.com/salamanders/ev3_next}"
WAIT_SECS="${WAIT_FOR_NETWORK_SECS:-60}"

# 1. Wait for network connectivity and fetch remote commit SHA (fail fast if timeout)
ELAPSED=0
RESPONSE=""

while [ ${ELAPSED} -le ${WAIT_SECS} ]; do
    RESPONSE=$(curl -k -s -m 2 -f -A "ev3-update-assets" "${GITHUB_API}" 2>/dev/null || true)
    if [ -n "${RESPONSE}" ]; then
        break
    fi
    if [ ${WAIT_SECS} -eq 0 ]; then
        break
    fi
    sleep 3
    ELAPSED=$(( ELAPSED + 3 ))
done

if [ -z "${RESPONSE}" ]; then
    exit 0
fi

REMOTE_SHA=$(echo "${RESPONSE}" | grep -o '"sha": *"[a-f0-9]\{40\}"' | head -n1 | cut -d'"' -f4 || true)
if [ -z "${REMOTE_SHA}" ] || [ ${#REMOTE_SHA} -ne 40 ]; then
    exit 0
fi

# 2. Check if already up-to-date and all asset files exist
if [ -f "${VERSION_FILE}" ] && [ -s "${ASSETS_DIR}/index.html.gz" ] && [ -s "${ASSETS_DIR}/style.css.gz" ] && [ -s "${ASSETS_DIR}/app.js.gz" ]; then
    LOCAL_SHA=$(tr -d '[:space:]' < "${VERSION_FILE}" 2>/dev/null || true)
    if [ "${LOCAL_SHA}" = "${REMOTE_SHA}" ]; then
        exit 0
    fi
fi

# 3. Remote SHA differs: download and pre-gzip into temporary directory
trap 'rm -rf "${TMP_DIR}" 2>/dev/null || true' INT TERM HUP

rm -rf "${TMP_DIR}"
mkdir -p "${TMP_DIR}"

for file in index.html style.css app.js; do
    if ! curl -k -s -m 5 -f "${RAW_BASE}/${REMOTE_SHA}/web_assets/${file}" -o "${TMP_DIR}/${file}" 2>/dev/null || [ ! -s "${TMP_DIR}/${file}" ]; then
        rm -rf "${TMP_DIR}"
        exit 0
    fi

    if ! gzip -9 -c "${TMP_DIR}/${file}" > "${TMP_DIR}/${file}.gz" 2>/dev/null; then
        rm -rf "${TMP_DIR}"
        exit 0
    fi
    rm -f "${TMP_DIR}/${file}"
done

echo "${REMOTE_SHA}" > "${TMP_DIR}/.version"

# 4. If all 3 .gz files exist: set permissions, clear target and atomically move
if [ -f "${TMP_DIR}/index.html.gz" ] && [ -f "${TMP_DIR}/style.css.gz" ] && [ -f "${TMP_DIR}/app.js.gz" ]; then
    chmod 0755 "${TMP_DIR}" 2>/dev/null || true
    chmod 0644 "${TMP_DIR}"/* "${TMP_DIR}/.version" 2>/dev/null || true
    chown -R 1000:1000 "${TMP_DIR}" 2>/dev/null || true
    rm -rf "${ASSETS_DIR}"
    mv -T "${TMP_DIR}" "${ASSETS_DIR}"
    sync -f "${ASSETS_DIR}" 2>/dev/null || sync 2>/dev/null || true
    trap - INT TERM HUP

    # Restart ev3-web.service so it loads the updated assets into RAM
    if command -v systemctl >/dev/null 2>&1; then
        systemctl restart ev3-web.service 2>/dev/null || true
    fi
else
    rm -rf "${TMP_DIR}"
fi

exit 0

