# LEGO Mindstorms EV3 Web Motor Control

A fast, lightweight web server in Rust for the LEGO Mindstorms EV3 hardware (Texas Instruments Sitara AM1808, ARMv5te @ 300MHz, 64MB DRAM).

```bash
sudo ./tools/bake-image.sh --ssid "<your wifi name>" --password "<your wifi password>" && sudo ./tools/flash-image.sh /dev/sdX
```

---

## Quick Links & Workflows

Choose the setup that fits your goal:
- **[Standalone Appliance Mode (Recommended)](#fast-setup-standalone-pre-baked-image-recommended):** Flash one pre-baked image with Wi-Fi credentials. Boot the EV3 and control it from any browser.
- **[Self-Updating Web Assets](#self-updating-web-assets):** Web resources update automatically from GitHub at boot with offline self-healing.
- **[Host Simulation Mode](#quickstart-host-simulation-testing):** Test the full web user interface and motor physics on your computer without EV3 hardware.
- **[USB Developer Workflow](#step-by-step-hardware-setup--deployment-usb-developer-mode):** Connect via Mini-USB cable to cross-compile and deploy updates in seconds.

---

## Fast Setup: Standalone Pre-Baked Image (Recommended)

Use this method to create a flashable MicroSD card in one step. The pre-baked image starts automatically, connects to Wi-Fi, and serves the web dashboard.

```bash
sudo ./tools/bake-image.sh --ssid "<your wifi name>" --password "<your wifi password>" && sudo ./tools/flash-image.sh /dev/sdX
```

### 1. Bake the Appliance Image
Run the baker script with your Wi-Fi credentials on Linux:
```bash
sudo ./tools/bake-image.sh --ssid "<your wifi name>" --password "<your wifi password>"
```
This downloads the base image, injects the native ARM binary, installs `ev3-web.service`, configures ConnMan Wi-Fi, and outputs `ev3-web-motor-ready.img.xz`.

### 2. Verify Your MicroSD Card Device
> [!CAUTION]
> Always verify the drive name before you write. Writing to the wrong disk will destroy data.

Run these commands to confirm your target drive:
```bash
# 1. Verify drive connection type and capacity (look for TRAN=usb and card size):
lsblk -d -o NAME,SIZE,MODEL,TRAN

# 2. Verify existing volume labels (e.g., EV3DEV_BOOT):
lsblk -f /dev/sdX
```

### 3. Flash and Verify the Image
Write and verify the compressed image to your MicroSD card using the command-line tool (replace `/dev/sdX` with your verified device, for example `/dev/sdb`):
```bash
sudo ./tools/flash-image.sh /dev/sdX
```
Or execute the manual command pipeline:
```bash
xzcat ev3-web-motor-ready.img.xz | sudo dd of=/dev/sdX bs=4M status=progress conv=fsync
sudo sync
sudo ./tools/flash-image.sh --verify-only /dev/sdX
```
*Note: The `conv=fsync` option ensures all data writes to physical media before the command exits. Flashing uses command-line tools only (no BalenaEtcher).*

### 4. Boot the EV3 Brick
1. Insert the MicroSD card into the EV3 slot.
2. Insert a compatible USB Wi-Fi dongle into the EV3 side host port.
3. Press the **Center Button** to power on.
4. The EV3 will connect to your Wi-Fi network automatically.
5. The LCD screen will display the assigned web address: `http://<ip>/`.
6. Open that address on any smartphone, tablet, or PC on the same Wi-Fi network.

---

## Key Features

- **Low Latency:** Direct communication with the Linux kernel `/sys/class/tacho-motor/` sysfs interface (< 5ms latency, **TARGET**).
- **Low Memory Footprint:** Statically linked Rust binary that uses less than 3 MB RAM (**TARGET**). This prevents Out-Of-Memory errors on the 64 MB EV3 brick.
- **Self-Updating Web Assets:** The web dashboard updates from GitHub when the brick boots. Pre-compressed gzip assets load into RAM (~9 KB total). The server reads zero files from the SD card during web traffic.
- **Self-Healing Fallback:** Baseline gzip assets are embedded directly in the binary at compile time. If the brick is offline or files are damaged, the server restores baseline files automatically.
- **Host Simulation Mode (`--mock`):** Run and test simulated motors, LEDs, battery telemetry, and web dashboard on Windows, macOS, or Linux without physical EV3 hardware.
- **No SD Card Swapping:** The `deploy.sh` (macOS/Linux) and `deploy.ps1` (Windows) scripts upload new builds over the USB or Wi-Fi network in ~4 seconds (**TARGET**).
- **Robot Drive Controls & Safety Watchdog:** Directional D-Pad, keyboard shortcuts (`WASD` and Arrow keys), per-port polarity inversion, and a two-tier safety watchdog (400 ms Tank Drive heartbeat timeout with brake action; 1000 ms browser disconnect timeout).
- **On-Brick LCD Status Display & Wi-Fi Auto-Provisioning:** Native 7-row compact status display on `/dev/tty1` refreshing every 5 seconds with live IP and battery voltage, plus automatic Wi-Fi setup via root `wifi.txt`.
- **Background Telemetry Thread:** Non-blocking 50ms polling thread caches motor encoder data in memory. HTTP requests read from memory in less than 0.05ms (**TARGET**).

---

## Self-Updating Web Assets

The web interface updates automatically from GitHub when the EV3 brick boots. You do not need to flash the MicroSD card or recompile the binary to update the web dashboard.

### How It Works

1. **Network Update Service (`ev3-update-assets.service`):**
   - After boot, the background service `ev3-update-assets.service` waits for network connectivity.
   - It runs `/usr/local/bin/ev3-update-assets.sh` and queries the GitHub API for the latest commit SHA on the `main` branch.
   - If the brick is offline or the commit matches, the service exits cleanly.

2. **Atomic Download & Service Reload:**
   - If a new commit exists on GitHub, the script downloads `index.html`, `style.css`, and `app.js`.
   - The script compresses each file with `gzip -9` into a temporary folder.
   - If all three files download and compress successfully, the script moves the files atomically into `/home/robot/web_assets/` and restarts `ev3-web.service` to reload new assets into memory.

3. **In-Memory Serving (Zero SD Card Wear):**
   - The server loads the `.gz` files into memory during startup (total size ~12 KB).
   - HTTP responses serve pre-compressed gzip content with `Cache-Control: no-cache`.
   - The server serves all web traffic directly from memory. It does not read from the MicroSD card during HTTP requests.

4. **Self-Healing Fallback:**
   - The binary includes baseline `.gz` files at compile time with `include_bytes!`.
   - If the brick is offline, the server uses existing local assets.
   - If local files are missing, empty, or corrupted, the server restores the baseline files automatically.

---

## Quickstart: Host Simulation Testing

You can test the user interface and simulated motors on your PC without EV3 hardware:

```bash
# 1. Start the simulation server on port 8080:
cargo run -- --mock --port 8080

# 2. Open this address in your web browser:
http://localhost:8080/
```

### Keyboard Controls
- <kbd>W</kbd> / <kbd>▲</kbd> : Forward
- <kbd>S</kbd> / <kbd>▼</kbd> : Reverse
- <kbd>A</kbd> / <kbd>◄</kbd> : Turn Left
- <kbd>D</kbd> / <kbd>►</kbd> : Turn Right
- <kbd>Spacebar</kbd> : **Emergency Stop (Stops all motors immediately with brake)**

---

## Step-by-Step Hardware Setup & Deployment (USB Developer Mode)

Follow this setup **one time**. After initial setup, you never remove the MicroSD card again.

### 1. Prepare the MicroSD Card
1. Use a **MicroSDHC card between 4 GB and 32 GB** (Class 10 recommended).
   > Do not use cards larger than 32 GB (SDXC). The EV3 hardware cannot read them.
2. Download the official operating system release:
   - **Release Page:** [ev3dev GitHub Releases (2020-04-10)](https://github.com/ev3dev/ev3dev/releases/tag/ev3dev-stretch-2020-04-10)
   - **Direct Download:** [`ev3dev-stretch-ev3-generic-2020-04-10.zip`](https://github.com/ev3dev/ev3dev/releases/download/ev3dev-stretch-2020-04-10/ev3dev-stretch-ev3-generic-2020-04-10.zip)
3. Flash the base image completely via the command line (no BalenaEtcher):
   ```bash
   unzip -p ev3dev-stretch-ev3-generic-2020-04-10.zip "*.img" | sudo dd of=/dev/sdX bs=4M status=progress conv=fsync
   sudo sync
   ```
4. *(Optional for Automatic Wi-Fi)*: Create a text file named `wifi.txt` in the root of the flashed SD card. Line 1 is the Wi-Fi SSID; Line 2 is the Wi-Fi password.

### 2. First Boot, USB Connection & Optional Wi-Fi Dongle
1. Insert the MicroSD card into the EV3 brick.
2. *(Optional for Wireless Control)* Insert a Linux 4.14 compatible USB Wi-Fi dongle into the EV3 side **USB Host port** (for example: Edimax EW-7811Un V1 / Realtek `RTL8188CUS`, `RTL8188EU`, Atheros `AR9271`, or Ralink `RT5370`).
3. Press the **Center Button** to power on. Wait 1 to 2 minutes for the initial boot.
4. Connect the Mini-USB cable between the EV3 **PC port** and your host computer.
5. Verify the USB network connection:
   ```bash
   ping 192.168.2.2
   ```

> [!TIP]
> **macOS USB Network Setup (`CDC Composite Gadget`):**
> 1. `ev3dev-stretch` exposes a CDC-ECM USB network interface that macOS supports without third-party drivers.
> 2. Open **System Settings -> Network** on macOS.
> 3. Select **CDC Composite Gadget** (add it with the `+` button if not listed) and configure IPv4 using **DHCP** (or manually set IP `192.168.2.1` and Subnet Mask `255.255.255.0`).
>
> **Windows USB Driver Fix (if ping fails):**
> 1. Open Windows **Device Manager** (`Win + X` -> `Device Manager`).
> 2. Right-click `RNDIS/Ethernet Gadget` -> **Update driver**.
> 3. Select **Browse my computer for drivers** -> **Let me pick from a list**.
> 4. Select **Network adapters** -> Manufacturer: **Microsoft** -> Model: **USB Ethernet/RNDIS Gadget**.
> 5. Complete driver setup.

### 3. One-Time Boot Acceleration & Service Setup
Connect to the EV3 with SSH (default password is `maker`):
```bash
ssh robot@192.168.2.2
```

Run these commands to decrease future boot times to **10–15 seconds** (**TARGET**), free 20 MB of RAM, and reserve `/dev/tty1` for the on-brick LCD status display:
```bash
sudo su

# 1. Disable the heavy LCD GUI and tty1 login prompt:
systemctl disable --now brickman.service
systemctl mask getty@tty1.service

# 2. Mask blocking startup services:
systemctl mask connman-wait-online.service
systemctl mask systemd-networkd-wait-online.service
# Note: Keep systemd-fsck-root active to repair unclean shutdowns and prevent read-only mounts
systemctl mask apt-daily.service apt-daily.timer
systemctl mask apt-daily-upgrade.service apt-daily-upgrade.timer

exit
exit
```

Install the auto-start background service:
```bash
scp ./ev3-web.service robot@192.168.2.2:/tmp/
ssh robot@192.168.2.2 "sudo mv /tmp/ev3-web.service /etc/systemd/system/ && sudo systemctl daemon-reload && sudo systemctl enable ev3-web.service"
```

### 4. Deploy the Application (1-Click)
Run the deployment script from the project directory:

- **macOS / Linux:**
  ```bash
  ./archive/deploy.sh 192.168.2.2
  ```
- **Windows PowerShell:**
  ```powershell
  .\archive\deploy.ps1 -TargetIp 192.168.2.2
  ```
*This cross-compiles for ARMv5te, uploads the binary via `/tmp` staging, and restarts the service in ~4 seconds (**TARGET**).*

### 5. Access the Web Dashboard (USB or Wi-Fi)
- Over USB, open `http://192.168.2.2/` in your web browser.
- Over Wi-Fi, if `wifi.txt` was provisioned, check the EV3 LCD screen for the active IP address (`http://<wifi-ip>/`).

---

## REST API Reference

| Method | Endpoint | Payload | Description |
| :--- | :--- | :--- | :--- |
| `GET` | `/` | None | Returns the embedded web dashboard. |
| `GET` | `/api/status` | None | Returns telemetry data for all 4 motor ports. |
| `GET` | `/api/battery` | None | Returns battery voltage and current. |
| `POST` | `/api/motor/{port}/run-forever` | `{"speed": 500}` | Runs motor continuously at target speed. |
| `POST` | `/api/motor/{port}/run-timed` | `{"speed": 500, "time_ms": 1000, "stop_action": "brake"}` | Runs motor for a specified duration in milliseconds. |
| `POST` | `/api/motor/{port}/run-to-rel-pos`| `{"speed": 400, "position_sp": 360, "stop_action": "hold"}` | Rotates motor by relative degree count. |
| `POST` | `/api/motor/{port}/run-direct` | `{"duty_cycle": 75}` | Sets direct PWM duty cycle (-100% to +100%). |
| `POST` | `/api/motor/{port}/stop` | `{"action": "coast" \| "brake" \| "hold"}` | Stops motor with specified stop mode. |
| `POST` | `/api/motor/{port}/reset` | None | Resets relative encoder count to 0. |
| `POST` | `/api/motor/{port}/polarity` | `{"polarity": "normal" \| "inversed"}` | Inverts motor rotation direction. |
| `POST` | `/api/tank-drive` | `{"left_port":"B", "right_port":"C", "left_speed":500, "right_speed":500}` | Drives left and right motors together. |
| `POST` | `/api/emergency-stop` | None | **Emergency Stop**: stops all motors immediately with brake action. |
| `POST` | `/api/shutdown` | None | **Power Off**: safely powers down the EV3 brick. |

---

## Project Structure

```
ev3_next/
├── Cargo.toml               # Cargo package configuration
├── ev3-web.service          # Systemd unit file for auto-start on boot
├── tools/
│   ├── bake-image.sh        # Appliance disk image baker with --ssid and --password
│   ├── flash-image.sh       # Command-line flash and post-write verification tool
│   └── update-assets.sh     # Boot-time web asset updater from GitHub
├── zig-lld-arm.sh           # Linux cross-linker wrapper
├── zig-linker.py            # Linker argument adapter for Zig LLD
├── archive/                 # Historical documents and legacy USB deploy scripts
│   ├── BUGS.md              # Historical bug tracking log (remediated)
│   ├── REPORT.md            # Early architectural feasibility report
│   ├── deploy.ps1           # Legacy USB deployment script for PowerShell
│   ├── deploy.sh            # Legacy USB deployment script for Bash
│   └── zig-lld-arm.cmd      # Legacy Windows batch linker
├── src/
│   ├── main.rs              # Program entry point and HTTP worker pool
│   ├── config.rs            # Command line argument parser
│   ├── controller.rs        # Hardware controller and telemetry cache
│   ├── sysfs/
│   │   ├── mod.rs           # Sysfs module exports
│   │   ├── motor.rs         # Real Linux sysfs tacho-motor driver
│   │   ├── mock.rs          # Simulated motor controller for testing
│   │   ├── led.rs           # Sysfs brick status LED driver
│   │   ├── display.rs       # Console /dev/tty1 LCD status renderer
│   │   └── wifi.rs          # wifi.txt credential auto-provisioning
│   └── web/
│       ├── mod.rs           # Web module exports
│       ├── router.rs        # HTTP request router and static asset handler
│       └── handlers.rs      # REST API request and response structures
└── web_assets/
    ├── index.html           # Web dashboard user interface
    ├── style.css            # Dark theme styles
    └── app.js               # Telemetry polling loop and user input handlers
```

---

## License
MIT / Apache 2.0
