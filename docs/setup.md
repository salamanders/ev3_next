# EV3 Appliance Setup Guide

This guide describes how to bake, flash, and boot the standalone EV3 Web Motor Control appliance image.

---

## 1. Step 1: Configure `wifi.txt` Once

Create a `wifi.txt` file in the project root directory (this file is git-ignored so your credentials are never committed):

```text
<SSID>
<PASSWORD>
```

---

## 2. Step 2: Bake, Flash, and Verify in One Command

Plug in your MicroSD card and run:

```bash
sudo ./tools/flash-image.sh -y
```

*(Tip: Install `bmap-tools` via `sudo apt install bmap-tools` on Linux for 2–3x faster flashing that skips empty disk blocks.)*

### What `flash-image.sh` Does Automatically:
1. Calls [`tools/bake-image.sh`](file:///Users/benhill/Desktop/hobbies/ev3_next/tools/bake-image.sh), which reads `./wifi.txt` (or accepts `--ssid "<SSID>" --password "<PASSWORD>"` overrides).
2. Downloads the official `ev3dev-stretch` base image once and extracts a clean `build_image/ev3dev.img`.
3. Mounts the ext4 root filesystem, injects `/usr/local/bin/ev3-web-motor`, enables `ev3-web.service`, writes `/var/lib/connman/ev3_wifi.config`, and disables `brickman`.
4. Generates a block map (`build_image/ev3dev.img.bmap`) using `bmaptool create` and skips slow `xz` compression (pass `--xz` to `bake-image.sh` only when building a compressed release archive).
5. Auto-detects your removable USB MicroSD card (`RM=1`, 2 GB–64 GB), or uses the device path you specify (for example, `sudo ./tools/flash-image.sh -y /dev/sdc`).
6. Writes the uncompressed image using `bmaptool copy --bmap` (falling back to `dd` if `bmaptool` is not installed) and verifies the flashed root partition.

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
