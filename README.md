# LEGO Mindstorms EV3 Web Motor Control

A fast, lightweight web server in Rust for the LEGO Mindstorms EV3 hardware (Texas Instruments Sitara AM1808, ARMv5te @ 300MHz, 64MB DRAM).

---

## 📍 Project Status and Development Phases

This project operates in two distinct phases:

### **Current Phase: Phase 1 (Active USB Development & Iteration)** 👈 *WE ARE HERE*
- **Workflow:** The EV3 connects to the host PC with a Mini-USB cable.
- **Deployment:** You flash the SD card one time. You compile code on the PC and send updates to the brick over the USB network cable with `deploy.ps1` in 4 seconds.
- **Host Testing:** You verify all user interface and API changes on the PC first using `cargo run -- --mock`.
- **Goal:** Rapid iteration, feature development, and hardware driver validation with zero friction and no SD card swapping.

### **Future Phase: Phase 2 (Standalone Pre-Baked Disk Image)**
- **Workflow:** The EV3 operates independently with no PC attached.
- **Deployment:** A single pre-baked MicroSD card image (`.img`) contains the optimized Linux kernel, root filesystem, auto-start systemd service, and web server binary.
- **Runtime:** Powering on the brick automatically starts the Wi-Fi access point and web server in 10 to 15 seconds. Users control motors directly from smartphones or tablets over Wi-Fi.

---

## Key Features

- **Low Latency:** Direct communication with the Linux kernel `/sys/class/tacho-motor/` sysfs interface (< 5ms latency).
- **Low Memory Footprint:** Statically linked Rust binary that uses less than 3 MB RAM. This prevents Out-Of-Memory errors on the 64 MB EV3 brick.
- **Embedded Web User Interface:** Single-Page Application (HTML5 / CSS3 / JavaScript) embedded directly into the binary with `include_str!`.
- **Host Simulation Mode (`--mock`):** Run and test simulated motors on Windows, macOS, or Linux without physical EV3 hardware.
- **No SD Card Swapping:** The `deploy.ps1` script uploads new builds over the USB network cable in 4 seconds.
- **Robot Drive Controls:** Virtual joystick, directional D-Pad, and keyboard shortcuts (`WASD` and Arrow keys) for two-wheel robots.
- **Background Telemetry Thread:** Non-blocking 50ms polling thread caches motor encoder data in memory. HTTP requests read from memory in less than 0.05ms.

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
- <kbd>Spacebar</kbd> : **Emergency Stop (Stops all motors immediately)**

---

## Step-by-Step Hardware Setup & Deployment (Phase 1)

Follow this setup **one time**. After initial setup, you never remove the MicroSD card again.

### 1. Prepare the MicroSD Card
1. Use a **MicroSDHC card between 4 GB and 32 GB** (Class 10 recommended).
   > Do not use cards larger than 32 GB (SDXC). The EV3 hardware cannot read them.
2. Download the official operating system release:
   - **Release Page:** [ev3dev GitHub Releases (2020-04-10)](https://github.com/ev3dev/ev3dev/releases/tag/ev3dev-stretch-2020-04-10)
   - **Direct Download:** [`ev3dev-stretch-ev3-generic-2020-04-10.zip`](https://github.com/ev3dev/ev3dev/releases/download/ev3dev-stretch-2020-04-10/ev3dev-stretch-ev3-generic-2020-04-10.zip)
3. Download and open [BalenaEtcher](https://etcher.balena.io/).
4. Select the downloaded `.zip` file, select your MicroSD card drive, and click **Flash!**.

### 2. First Boot & USB Connection
1. Insert the MicroSD card into the EV3 brick.
2. Press the **Center Button** to power on. Wait 1 to 2 minutes for the initial boot.
3. Connect the Mini-USB cable between the EV3 **PC port** and your Windows PC.
4. Open Windows PowerShell and verify the network connection:
   ```powershell
   ping 192.168.2.2
   ```

> [!TIP]
> **Windows USB Driver Fix (if ping fails):**
> 1. Open Windows **Device Manager** (`Win + X` -> `Device Manager`).
> 2. Right-click `RNDIS/Ethernet Gadget` -> **Update driver**.
> 3. Select **Browse my computer for drivers** -> **Let me pick from a list**.
> 4. Select **Network adapters** -> Manufacturer: **Microsoft** -> Model: **USB Ethernet/RNDIS Gadget**.
> 5. Complete driver setup.

### 3. One-Time Boot Acceleration & Service Setup
Connect to the EV3 with SSH (default password is `maker`):
```powershell
ssh robot@192.168.2.2
```

Run these commands to decrease future boot times to **10–15 seconds** and free 20 MB of RAM:
```bash
sudo su

# 1. Disable the LCD GUI:
systemctl disable --now brickman.service

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
```powershell
scp .\ev3-web.service robot@192.168.2.2:/tmp/
ssh robot@192.168.2.2 "sudo mv /tmp/ev3-web.service /etc/systemd/system/ && sudo systemctl daemon-reload && sudo systemctl enable ev3-web.service"
```

### 4. Deploy the Application (1-Click)
Run this command from Windows PowerShell in the project directory:
```powershell
.\deploy.ps1 -TargetIp 192.168.2.2
```
*This cross-compiles for ARMv5te, uploads the binary to `/home/robot/ev3-web-motor`, and restarts the service in 4 seconds.*

### 5. Access the Web Dashboard
Open your web browser and navigate to:
```
http://192.168.2.2/
```

---

## REST API Reference

| Method | Endpoint | Payload | Description |
| :--- | :--- | :--- | :--- |
| `GET` | `/` | None | Returns the embedded web dashboard. |
| `GET` | `/api/status` | None | Returns telemetry data for all 4 motor ports. |
| `POST` | `/api/motor/{port}/run-forever` | `{"speed": 500}` | Runs motor continuously at target speed. |
| `POST` | `/api/motor/{port}/run-timed` | `{"speed": 500, "time_ms": 1000, "stop_action": "brake"}` | Runs motor for a specified duration in milliseconds. |
| `POST` | `/api/motor/{port}/run-to-rel-pos`| `{"speed": 400, "position_sp": 360, "stop_action": "hold"}` | Rotates motor by relative degree count. |
| `POST` | `/api/motor/{port}/run-direct` | `{"duty_cycle": 75}` | Sets direct PWM duty cycle (-100% to +100%). |
| `POST` | `/api/motor/{port}/stop` | `{"action": "coast" \| "brake" \| "hold"}` | Stops motor with specified stop mode. |
| `POST` | `/api/motor/{port}/reset` | None | Resets relative encoder count to 0. |
| `POST` | `/api/tank-drive` | `{"left_port":"B", "right_port":"C", "left_speed":500, "right_speed":500}` | Drives left and right motors together. |
| `POST` | `/api/emergency-stop` | None | **Emergency Stop**: stops all motors immediately. |

---

## Project Structure

```
ev3_next/
├── Cargo.toml               # Cargo package configuration
├── Cross.toml               # Cross-compilation container configuration
├── ev3-web.service          # Systemd unit file for auto-start on boot
├── deploy.ps1               # Deployment script for Windows PowerShell
├── deploy.sh                # Deployment script for Linux and macOS
├── zig-lld-arm.cmd          # Windows cross-linker script
├── zig-linker.py            # Windows linker argument adapter
├── src/
│   ├── main.rs              # Program entry point and HTTP worker pool
│   ├── config.rs            # Command line argument parser
│   ├── controller.rs        # Hardware controller and telemetry cache
│   ├── sysfs/
│   │   ├── mod.rs           # Sysfs module exports
│   │   ├── motor.rs         # Real Linux sysfs tacho-motor driver
│   │   └── mock.rs          # Simulated motor controller for testing
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
