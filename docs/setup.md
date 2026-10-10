# EV3 Appliance Setup Guide

This guide describes how to bake, flash, and boot the standalone EV3 Web Motor Control appliance image.

---

## 1. Step 1: Bake the Appliance Image

The bake script creates a custom bootable disk image from the base `ev3dev-stretch` release. It pre-installs the cross-compiled Rust binary, installs systemd service units, and configures Wi-Fi credentials:

```bash
sudo ./tools/bake-image.sh --ssid "<SSID>" --password "<PASSWORD>"
```

### What the Script Does:
1. Downloads the official `ev3dev-stretch` base image if not cached locally.
2. Mounts the root filesystem partition.
3. Injects the ARMv5te release binary into `/usr/local/bin/ev3-web-motor`.
4. Installs the `ev3-web.service` systemd unit file.
5. Writes the ConnMan Wi-Fi configuration file (`/var/lib/connman/ev3_wifi.config`).
6. Masks slow boot services to reduce boot time to 10–15 seconds.
7. Compresses the output image into `ev3-web-motor-ready.img.xz`.

---

## 2. Step 2: Verify Your Target MicroSD Card

> [!CAUTION]
> Always verify the drive device name before writing. Writing to an incorrect disk will cause permanent data loss.

Confirm your target card device name:

```bash
# Verify drive connection type and size (look for TRAN=usb):
lsblk -d -o NAME,SIZE,MODEL,TRAN

# Verify existing partition labels:
lsblk -f /dev/sdX
```

---

## 3. Step 3: Flash and Verify the Image

Flash the compressed image to your MicroSD card using the verified device name (for example, `/dev/sdb`):

```bash
sudo ./tools/flash-image.sh /dev/sdX
```

### Manual Command Alternative:
```bash
xzcat ev3-web-motor-ready.img.xz | sudo dd of=/dev/sdX bs=4M status=progress conv=fsync
sudo sync
sudo ./tools/flash-image.sh --verify-only /dev/sdX
```

*Note: The `conv=fsync` parameter ensures all written blocks flush to physical storage before the process exits.*

---

## 4. Step 4: First Boot and Wi-Fi Connection

1. Insert the MicroSD card into the EV3 MicroSD slot.
2. Insert a Linux 4.14 compatible USB Wi-Fi dongle into the EV3 side host port.
   - Compatible chipsets: Realtek `RTL8188CUS`, `RTL8188EU`, Atheros `AR9271`, Ralink `RT5370`.
3. Press the **Center Button** to power on the EV3 brick.
4. The EV3 will boot in approximately 10 to 15 seconds.
5. The brick automatically connects to your configured Wi-Fi network.
6. The on-brick LCD screen displays the assigned web address: `http://<ip>/`.
7. Open that address in any modern web browser on your phone, tablet, or PC.

---

## 5. Alternative: Manual `wifi.txt` Provisioning

If you flashed an image without pre-configured Wi-Fi:
1. Mount the FAT boot partition or root partition on your computer.
2. Create a file named `wifi.txt` in the root folder with two lines:
   ```text
   <SSID>
   <PASSWORD>
   ```
3. Boot the EV3. The server reads `wifi.txt` on startup, writes the ConnMan configuration, and deletes the plain text file automatically.
