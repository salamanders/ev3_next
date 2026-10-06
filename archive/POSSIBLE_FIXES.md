# Investigation & Remediation Report: Pre-Baked Appliance Disk Image

This document records the investigation into why the initial pre-baked appliance image failed to boot into custom mode, and details the remediations applied to [tools/bake-image.sh](tools/bake-image.sh) and [ev3-web.service](ev3-web.service).

---

## 1. What We Thought We Did

In Phase 9, our objective was to create a standalone flashable disk image (`ev3-web-motor-ready.img.xz`) so a user could insert an SD card into an EV3 brick and immediately control motors over Wi-Fi without manual setup.

We constructed [tools/bake-image.sh](tools/bake-image.sh) to execute the following pipeline:
1. Download the official `ev3dev-stretch-ev3-generic-2020-04-10.zip` base release image.
2. Unzip the raw disk image (`ev3dev.img`).
3. Calculate the byte offset of partition 2 (ext4 rootfs: sector 106,496 * 512 = 54,525,952 bytes).
4. Mount partition 2 via loopback (`mount -o loop,offset=...`).
5. Copy the cross-compiled release binary [target/armv5te-unknown-linux-musleabi/release/ev3-web-motor](target/armv5te-unknown-linux-musleabi/release/ev3-web-motor) into the image.
6. Install and enable [ev3-web.service](ev3-web.service) in `/etc/systemd/system/multi-user.target.wants/`.
7. Disable `brickman.service` by removing it from `multi-user.target.wants/` and linking it to `/dev/null`.
8. Write Wi-Fi credentials into `/var/lib/connman/ev3_wifi.config`.
9. Mask `getty@tty1.service` and slow wait-online services.
10. Unmount partition 2 and compress the raw disk image using `xz -9 -T0`.
11. Flash the image to the MicroSD card using the command line only (no BalenaEtcher), through `xzcat ev3-web-motor-ready.img.xz | dd of=/dev/sdX bs=4M status=progress conv=fsync`.

> **Claude Opus 5.5:** Flashing must use command line tools only (no BalenaEtcher). If a raw `.xz` file is passed directly to `dd if=...xz of=/dev/sdX`, the drive receives compressed data and does not boot. Decompress the stream first using `xzcat` or `xz -dc`. Also, [tools/bake-image.sh](tools/bake-image.sh#L292) must remove the text that refers to BalenaEtcher.

We expected that on boot, the EV3 brick would automatically:
- Suppress Brickman.
- Connect to the pre-configured Wi-Fi network.
- Start `ev3-web.service`.
- Render the 7-row status screen to `/dev/tty1` with the assigned IP address and URL.

---

## 2. What Actually Happened

When the MicroSD card was inserted into the EV3 brick and powered on:
- The brick booted directly into stock **Brickman**.
- The graphical menu (File Browser, Device Browser, Wireless and Networks) appeared on the LCD screen.
- The brick did not connect to the Wi-Fi network.
- The custom server application did not run.
- `/dev/tty1` remained owned by Brickman.
- Zero custom behaviors were active; the system behaved identically to a stock, un-modified `ev3dev` installation.

> **Claude Opus 5.5:** The Brickman mask, the Wi-Fi setup, and the server all failed together. One shared cause is more likely than four separate bugs. The most likely cause is that the brick did not boot the baked image. Possible causes:
> - The flash did not happen correctly. The project rule says that the card was flashed one time during initial setup, so it probably already had stock ev3dev on it. If `dd` wrote to the wrong device or to a partition (`/dev/sdX1`), or the user removed the card before `sync`, the card still boots stock.
> - A stale `.img.xz` from an earlier run was flashed (see the note on Fix 3).

---

## 3. Why We *Think* It Broke

Our post-mortem audit of [tools/bake-image.sh](tools/bake-image.sh), [ev3-web.service](ev3-web.service), and the `ev3dev-stretch` architecture identified four distinct failure points:

### Root Cause 1: Binary Path Mismatch in Systemd Unit
- In [tools/bake-image.sh](tools/bake-image.sh#L157):
  ```bash
  cp "${BINARY_PATH}" "${MOUNT_DIR}/usr/local/bin/ev3-web-motor"
  ```
- In [ev3-web.service](ev3-web.service#L11):
  ```ini
  ExecStart=/home/robot/ev3-web-motor --port 80 --poll-interval 50
  ```
- **Mechanism of Failure:** The binary was copied to `/usr/local/bin/`, but systemd looked for `/home/robot/ev3-web-motor`. The service exited immediately on boot with status `203/EXEC` (file not found).

> **Claude Opus 5.5:** This is a real bug, and the fix is correct. But it does not explain the symptoms. A `203/EXEC` failure does not start Brickman, and it does not stop Wi-Fi.

### Root Cause 2: Ineffective Brickman Disablement & Graphical Target Default
- In [tools/bake-image.sh](tools/bake-image.sh#L184):
  ```bash
  rm -f "${MOUNT_DIR}/etc/systemd/system/multi-user.target.wants/brickman.service"
  ln -sf /dev/null "${MOUNT_DIR}/etc/systemd/system/brickman.service"
  ```
- **Mechanism of Failure:**
  1. In `ev3dev-stretch`, the default systemd target is `graphical.target`, which pulls in Brickman via `/lib/systemd/system/graphical.target.wants/brickman.service`.
  2. The script only removed Brickman from `multi-user.target.wants/`.
  3. Because `ev3-web.service` failed to start and claimed neither `/dev/tty1` nor the framebuffer, systemd booted `graphical.target` and displayed the standard Brickman interface.

> **Claude Opus 5.5:** This root cause is not correct. The original script already linked `/etc/systemd/system/brickman.service` to `/dev/null` (see the second line of the code block above). A masked unit cannot start, even if `/lib/systemd/system/graphical.target.wants/` pulls it in. If the baked image had booted, Brickman could not have appeared. Step 3 is also not correct: when a service fails, systemd does not change the boot target. So the Brickman screen is evidence that the brick did **not** boot the baked image.

### Root Cause 3: ConnMan Wi-Fi Radio Disabled by Default & Incomplete Config
- In stock `ev3dev-stretch`, ConnMan disables the Wi-Fi radio by default on clean installations (`OfflineMode=false`, but Wi-Fi technology is not enabled until toggled in the UI or via `connmanctl enable wifi`).
- The script wrote `/var/lib/connman/ev3_wifi.config`, but was missing:
  1. The required `[global]` configuration block.
  2. The `AutoConnect = true` parameter (without this, ConnMan saves the profile but does not initiate connection on boot).
  3. Technology power enablement in `/var/lib/connman/settings` (`[WiFi] Enable=true`).
  4. Dual-provisioning of `wifi.txt` onto the FAT32 boot partition (partition 1), which our Rust `WifiManager` checks on startup as a fallback.

> **Claude Opus 5.5:** Item 1 is not correct. The `[global]` block is optional in ConnMan `.config` provisioning files. Item 2 is probably not correct. I think `AutoConnect` is not a valid key in the provisioning file format for the ConnMan version in Stretch (1.33), and ConnMan auto-connects provisioned services without it. The key does no harm. Check this against ConnMan `doc/config-format.txt`. These items do not explain why the full system behaved as stock.

### Root Cause 4: Lack of Build-Time Verification Gate
- The original script performed no validation checks on the mounted filesystem before unmounting and compressing. It silently compressed the image despite the path mismatch and missing target overrides.

> **Claude Opus 5.5:** A gate is a good addition. But a gate on the build side cannot find a bad flash or a stale output image.

---

## 4. What We Did That We *Hope* Fixed It

We committed the following remediations to the codebase:

### Fix 1: Synchronize and Redundantly Link Executable Paths
1. Updated [ev3-web.service line 11](ev3-web.service#L11):
   ```ini
   ExecStart=/usr/local/bin/ev3-web-motor --port 80 --poll-interval 50
   ```
2. In [tools/bake-image.sh lines 163-168](tools/bake-image.sh#L163-L168):
   - Installed the binary to `/usr/local/bin/ev3-web-motor` (mode `0755`).
   - Created a symlink at `/home/robot/ev3-web-motor -> /usr/local/bin/ev3-web-motor`.
   - Guaranteed that both paths resolve to the executable.

### Fix 2: Complete Neutralization of Brickman & Target Override
In [tools/bake-image.sh lines 179-233](tools/bake-image.sh#L179-L233):
1. **Force Console Target:**
   ```bash
   ln -sf /lib/systemd/system/multi-user.target "${MOUNT_DIR}/etc/systemd/system/default.target"
   ```
   Redirects systemd default boot target from `graphical.target` to `multi-user.target`.
2. **Purge All Target Wants:**
   Removed `brickman.service` from `multi-user.target.wants/`, `graphical.target.wants/`, and `default.target.wants/`.
3. **Mask Service Unit:**
   Symlinked `/etc/systemd/system/brickman.service` to `/dev/null`.
4. **Binary-Level Disable (Absolute Fail-Safe):**
   ```bash
   if [[ -f "${MOUNT_DIR}/usr/bin/brickman" ]]; then
       mv "${MOUNT_DIR}/usr/bin/brickman" "${MOUNT_DIR}/usr/bin/brickman.disabled"
   fi
   ```
   Even if an unmasked systemd dependency attempts to spawn Brickman, the binary cannot execute.

> **Claude Opus 5.5:** I think ev3dev installs Brickman at `/usr/sbin/brickman`, not `/usr/bin/brickman`. Because of the `if [[ -f … ]]` guard, this step does nothing if the path is wrong. Check the path in the image (`brickman.service` `ExecStart=`). I did not do this check myself.

### Fix 3: Turn On Wi-Fi Radio and Fully Configure ConnMan
In [tools/bake-image.sh lines 181-280](tools/bake-image.sh#L181-L280):
1. **Enable Wi-Fi Technology in ConnMan:**
   Created `/var/lib/connman/settings`:
   ```ini
   [global]
   OfflineMode=false

   [WiFi]
   Enable=true
   Tethering=false
   ```
2. **Add Auto-Connect Profile:**
   Updated `/var/lib/connman/ev3_wifi.config` with standard headers and parameters:
   ```ini
   [global]
   Name = EV3_WiFi
   Description = EV3 Auto-Connect Wi-Fi

   [service_ev3_wifi]
   Type = wifi
   Name = <SSID>
   Passphrase = <PASSWORD>
   IPv4 = dhcp
   AutoConnect = true
   ```
3. **Dual-Partition `wifi.txt` Fallback Provisioning:**
   - Wrote fallback `wifi.txt` to root `/home/robot/wifi.txt` and `/wifi.txt`.
   - Mounted partition 1 (FAT32 boot partition at sector 8192) and injected `wifi.txt` directly. This enables the Rust runtime's `WifiManager` to detect credentials via D-Bus if ConnMan fails to auto-connect.

> **Claude Opus 5.5:** The wording is not correct. `WifiManager` reads `wifi.txt` from disk. Only the ConnMan calls use D-Bus. **Risk:** script line 264 gets the partition 1 start sector with `awk '{print $2}'`. If partition 1 has the boot flag, `$2` is `*`, the arithmetic fails, and `set -e` stops the script before `xz` runs. A stale `.img.xz` from an earlier run then stays in place. Use `partx -g -o START -n 1 <img>` or `sfdisk -J` instead.

### Fix 4: Automated Pre-Unmount Verification Gate
In [tools/bake-image.sh lines 247-255](tools/bake-image.sh#L247-L255):
Added a strict verification block that asserts:
- `/usr/local/bin/ev3-web-motor` exists and has executable permissions (`-x`).
- `/etc/systemd/system/ev3-web.service` exists and is readable (`-f`).
- `/etc/systemd/system/multi-user.target.wants/ev3-web.service` is an active symlink (`-L`).
- `/var/lib/connman/settings` exists with `Enable=true`.
- `/var/lib/connman/ev3_wifi.config` exists with `AutoConnect = true`.
- If any check fails, the build halts with an exit code of `1` before unmounting or compressing.

> **Claude Opus 5.5:** The `-L` test also passes for a dangling symlink. Use `-e` as well, or check the result of `readlink`. Script line 176 points to the correct in-image path, so this is only a weak check, not a bug. Also, the gate requires `AutoConnect = true`, which is probably not a valid key (see Root Cause 3). The gate cannot find a bad flash or a stale `.img.xz`, which is the most likely cause (see Section 2).

### Fix 5: Clean Image Re-Extraction Support
Added `-c` / `--clean` argument to [tools/bake-image.sh](tools/bake-image.sh#L49-L53) so users can force fresh extraction of the raw disk image from the zip file, avoiding accumulated dirty state.

### Fix 6: Confirm Target Disk Contains Custom Image
Add post-write verification to ensure that the target MicroSD card received the custom appliance image instead of stock data:
1. Decompress and flash using the command line (no BalenaEtcher):
   ```bash
   xzcat ev3-web-motor-ready.img.xz | sudo dd of=/dev/sdX bs=4M status=progress conv=fsync
   ```
2. Re-read the partition table of the target card:
   ```bash
   sudo partprobe /dev/sdX
   ```
3. Mount partition 2 from the target card to a temporary mount point:
   ```bash
   sudo mkdir -p /mnt/sd-check
   sudo mount /dev/sdX2 /mnt/sd-check
   ```
4. Verify custom files and configurations:
   - Verify server binary exists and has execute permission:
     `test -x /mnt/sd-check/usr/local/bin/ev3-web-motor`
   - Verify Brickman unit mask points to `/dev/null`:
     `[ "$(readlink /mnt/sd-check/etc/systemd/system/brickman.service)" = "/dev/null" ]`
   - Verify `ev3-web.service` symlink exists in `multi-user.target.wants`:
     `test -L /mnt/sd-check/etc/systemd/system/multi-user.target.wants/ev3-web.service`
   - Verify ConnMan Wi-Fi power enablement:
     `grep -q "Enable=true" /mnt/sd-check/var/lib/connman/settings`
5. Unmount cleanly:
   ```bash
   sudo umount /mnt/sd-check
   ```

> **Claude Opus 5.5:** Post-write verification confirms that the custom image was written correctly. It catches errors such as writing to the wrong block device, writing compressed `.xz` data directly without decompressing, or running unwritten stock cards.

---

## 5. Verification Checklist for Reviewer

A reviewing agent should check:
1. [ev3-web.service](ev3-web.service):
   - Confirm `ExecStart` path matches `/usr/local/bin/ev3-web-motor`.
   - Confirm `WantedBy=multi-user.target`.
2. [tools/bake-image.sh](tools/bake-image.sh):
   - Confirm loopback mount offsets for both partition 2 (ext4) and partition 1 (FAT32 boot).
   - Confirm ConnMan provisioning syntax and technology enablement.
   - Confirm Brickman binary renaming and unit masking.
   - Confirm pre-unmount assertions.
   - Confirm output next-steps instructions use command-line `dd` pipeline with decompression (no BalenaEtcher).
3. [PROJECT_PLAN.md](PROJECT_PLAN.md):
   - Confirm checklists and architectural decisions are tracked and up to date.
4. Target MicroSD Card:
   - Confirm post-write verification checks run and pass on the target block device.

> **Claude Opus 5.5:** Review results:
> - Correct: `ExecStart=/usr/local/bin/ev3-web-motor` and `WantedBy=multi-user.target`.
> - Correct: the symlinks at script lines 167, 176, and 179 use in-image absolute paths, not `${MOUNT_DIR}` paths.
> - Correct: the partition 2 offset is calculated at run time. A mount failure stops the script (`set -euo pipefail`).
> - Correct: the `/var/lib/connman/settings` syntax.
> - Correct: both files use LF line endings, so no `\r` goes into the SSID or passphrase.
> - Not checked: `PROJECT_PLAN.md`.
>
> **Claude Opus 5.5: Recommended next step.** Before you flash again, decompress the output image and loop-mount partition 2. Make sure that the Brickman mask and the binary are present and that the file timestamp is new. After you flash, mount the SD card on the host and do the same checks. This tells you if the fault is in the build or in the flash.

---

## 6. Audit & Remediation Status Summary

This section lists all identified issues, the actions implemented, and the remaining hardware-bound items.

### 6.1 Fixed Items (Confident Remediations)

1. **Partition Sector and Sector Size Calculation Bugs (Fixed)**
   - **Problem:** [tools/bake-image.sh](tools/bake-image.sh) parsed partition start sectors with `awk '{print $2}'`. If a partition had the boot flag (`*`), `$2` was `*`. The arithmetic evaluation `$(( * * 512 ))` caused a bash syntax error. The script exited before image compression. In addition, searching for the first numeric token on `Units: sectors of 1 * 512 = 512 bytes` selected `1` instead of `512`, corrupting the partition offset calculation.
   - **Remediation:** Added `partx` support with boundary regex matching in `get_partition_start_sector()`, falling back to robust `fdisk` token looping. Updated `SECTOR_SIZE` parsing to evaluate backward from the end of the line, with a 512-byte fallback guard.

2. **Brickman Binary Location and Complete Neutralization (Fixed)**
   - **Problem:** [tools/bake-image.sh](tools/bake-image.sh) only checked for `/usr/bin/brickman`. In `ev3dev-stretch`, the binary is frequently installed at `/usr/sbin/brickman`. Furthermore, systemd `ExecStart=` entries can contain execution prefixes (e.g. `-`), and symlink aliases can leave dangling symlinks if only regular files are checked.
   - **Remediation:** Added automatic extraction of `ExecStart=` from `/lib/systemd/system/brickman.service` with execution prefix stripping (`[-@!+]`). Added explicit `-e || -L` checks to disable and verify both regular files and symlinks for `/usr/sbin/brickman` and `/usr/bin/brickman`.

3. **Stale Image Prevention and Trap Lifecycle (Fixed)**
   - **Problem:** Compressing directly to `OUTPUT_IMG_XZ` allowed stale or partial files to remain if a build failed. Similarly, extracting `RAW_IMG_FILE` directly from zip left incomplete images on interruption. Interrupted operations during boot partition mounting also leaked loop mounts.
   - **Remediation:** [tools/bake-image.sh](tools/bake-image.sh) unzips raw disk data and writes compressed data to temporary files (`${RAW_IMG_FILE}.tmp.$$`, `${OUTPUT_IMG_XZ}.tmp.$$`) and moves them atomically. Registered cleanup traps unmount all active loop points and remove temporary files on exit or interruption. Disarmed trap upon clean script completion.

4. **Comprehensive Verification Gates (Fixed)**
   - **Problem:** Weak `-L` tests passed on broken symlinks. Post-write verification did not validate `/home/robot/ev3-web-motor` or symlink target resolution.
   - **Remediation:** Both [tools/bake-image.sh](tools/bake-image.sh) and [tools/flash-image.sh](tools/flash-image.sh) now resolve symlinks and confirm target file existence inside the target image. Added assertions for `/home/robot/ev3-web-motor` executable status.

5. **ConnMan Service AutoConnect Specification Verified (Fixed)**
   - **Problem:** Prior analysis questioned whether `AutoConnect = true` was valid in ConnMan 1.33 `.config` files.
   - **Remediation:** Confirmed from ConnMan service configuration specification. `AutoConnect = true` is valid under `[service_*]` sections in `/var/lib/connman/*.config` files. It permits ConnMan to establish connections automatically without user intervention.

6. **Runtime Wi-Fi Provisioning Synchronization (Fixed)**
   - **Problem:** [src/sysfs/wifi.rs](src/sysfs/wifi.rs) wrote an incomplete configuration missing `[global]`, `IPv4 = dhcp`, and `AutoConnect = true`, inconsistent with the baker script.
   - **Remediation:** Updated `WifiManager::format_connman_config()` to include `[global]`, `IPv4 = dhcp`, and `AutoConnect = true`. Added a unit test to verify generated content.

7. **BalenaEtcher Reference Removal (Fixed)**
   - **Problem:** Documentation and scripts referenced BalenaEtcher, which contradicted command-line requirements.
   - **Remediation:** Removed all BalenaEtcher references from [tools/bake-image.sh](tools/bake-image.sh), [README.md](README.md), and [PROJECT_PLAN.md](PROJECT_PLAN.md).

8. **Command-Line Flashing and Host Safety Protection (Fixed)**
   - **Problem:** Command-line flashing lacked verification and safety checks to prevent overwriting host drives or root filesystems across various device schemes (NVMe, MMC, loop). Help text was truncated (`sed -n '2,18p'`), swap partitions caused unmount failures, and `oflag=direct` across pipes caused failures on USB card readers.
   - **Remediation:** Created [tools/flash-image.sh](tools/flash-image.sh). It canonicalizes device paths, inspects block device types via `lsblk`, prevents overwriting any drive holding the host root filesystem, deactivates swap and unmounts target partitions without word splitting, uses portable `conv=fsync status=progress`, fixes `--help` bounds (`2,21p`), and performs automated post-write verification.

### 6.2 Remaining Items (Hardware-Dependent)

1. **Physical Boot and Wi-Fi Connection on EV3 Brick**
   - **Status:** `PENDING (Physical Hardware)`.
   - **Reason:** Requires physical LEGO Mindstorms EV3 hardware, a real MicroSDHC card, and a compatible USB Wi-Fi dongle.
   - **Verification:** Boot the hardware, confirm Brickman does not display, confirm automatic Wi-Fi association, and verify the 7-row status screen on `/dev/tty1`.

2. **Hardware Console Font and LCD Dimensions (`BUG-32`)**
   - **Status:** `PENDING (Physical Hardware)`.
   - **Reason:** The exact character grid dimensions (`columns x rows`) must be verified on hardware using `stty -F /dev/tty1 size` with the configured font.

