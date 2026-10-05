# LEGO Mindstorms EV3 Web Motor Control

A fast, lightweight web server in Rust for the LEGO Mindstorms EV3 hardware (Texas Instruments Sitara AM1808, ARMv5te @ 300MHz, 64MB DRAM).

---

## 📍 Project Status and Development Stages

This project operates in two distinct stages:

### **Stage A: Active USB Development & Iteration** 👈 *WE ARE HERE*
- **Workflow:** The EV3 connects to the host PC with a Mini-USB cable.
- **Deployment:** You flash the SD card one time. You compile code on the PC and send updates to the brick over the USB network cable with `deploy.ps1` in ~4 seconds (**TARGET**).
- **Host Testing:** You verify all user interface and API changes on the PC first using `cargo run -- --mock`.
- **Goal:** Rapid iteration, feature development, and hardware driver validation with zero friction and no SD card swapping.

### **Stage B: Standalone Pre-Baked Disk Image**
- **Workflow:** The EV3 operates independently with no PC attached.
- **Deployment:** A single pre-baked MicroSD card image (`.img`) contains the optimized Linux kernel, root filesystem, auto-start systemd service, and web server binary.
- **Runtime:** Powering on the brick connects to Wi-Fi automatically (via `wifi.txt`) and starts the web server. Users control motors directly from smartphones or tablets over Wi-Fi.

---

## Key Features

- **Low Latency:** Direct communication with the Linux kernel `/sys/class/tacho-motor/` sysfs interface (< 5ms latency, **TARGET**).
- **Low Memory Footprint:** Statically linked Rust binary that uses less than 3 MB RAM (**TARGET**). This prevents Out-Of-Memory errors on the 64 MB EV3 brick.
- **Embedded Web User Interface:** Single-Page Application (HTML5 / CSS3 / JavaScript) embedded directly into the binary with `include_str!`.
- **Host Simulation Mode (`--mock`):** Run and test simulated motors, LEDs, battery telemetry, and web dashboard on Windows, macOS, or Linux without physical EV3 hardware.
- **No SD Card Swapping:** The `deploy.sh` (macOS/Linux) and `deploy.ps1` (Windows) scripts upload new builds over the USB or Wi-Fi network in ~4 seconds (**TARGET**).
- **Robot Drive Controls & Safety Watchdog:** Directional D-Pad, keyboard shortcuts (`WASD` and Arrow keys), per-port polarity inversion, and a two-tier safety watchdog (400 ms Tank Drive heartbeat timeout with brake action; 1000 ms browser disconnect timeout).
- **On-Brick LCD Status Display & Wi-Fi Auto-Provisioning:** Native 7-row compact status display on `/dev/tty1` refreshing every 5 seconds with live IP and battery voltage, plus automatic Wi-Fi setup via root `wifi.txt`.
- **Background Telemetry Thread:** Non-blocking 50ms polling thread caches motor encoder data in memory. HTTP requests read from memory in less than 0.05ms (**TARGET**).

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

## Step-by-Step Hardware Setup & Deployment (Stage A)

Follow this setup **one time**. After initial setup, you never remove the MicroSD card again.

### 1. Prepare the MicroSD Card
1. Use a **MicroSDHC card between 4 GB and 32 GB** (Class 10 recommended).
   > Do not use cards larger than 32 GB (SDXC). The EV3 hardware cannot read them.
2. Download the official operating system release:
   - **Release Page:** [ev3dev GitHub Releases (2020-04-10)](https://github.com/ev3dev/ev3dev/releases/tag/ev3dev-stretch-2020-04-10)
   - **Direct Download:** [`ev3dev-stretch-ev3-generic-2020-04-10.zip`](https://github.com/ev3dev/ev3dev/releases/download/ev3dev-stretch-2020-04-10/ev3dev-stretch-ev3-generic-2020-04-10.zip)
3. Download and open [BalenaEtcher](https://etcher.balena.io/).
4. Select the downloaded `.zip` file, select your MicroSD card drive, and click **Flash!**.
5. *(Optional for Automatic Wi-Fi)*: Create a text file named `wifi.txt` in the root of the flashed SD card. Line 1 is the Wi-Fi SSID; Line 2 is the Wi-Fi password.

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
  ./deploy.sh 192.168.2.2
  ```
- **Windows PowerShell:**
  ```powershell
  .\deploy.ps1 -TargetIp 192.168.2.2
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
│   └── bake-image.sh        # Appliance disk image baker with --ssid and --password
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
