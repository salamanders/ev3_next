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

## Phase 1: USB Cross-Compilation and Deployment

### 1. Initial SD Card Setup (One Time Only)
1. Flash `ev3dev-stretch` onto a MicroSD card (4GB to 32GB).
2. Insert the card into the EV3 brick and turn on the power.
3. Connect the EV3 to the PC with the Mini-USB cable.
4. Verify the SSH connection:
   ```bash
   ssh robot@192.168.2.2  # (password: maker)
   ```

### 2. Boot Acceleration (10 to 15 Second Boot Time)
Run these commands on the EV3 over SSH:
```bash
sudo su
# Disable heavy services and GUI to free 20MB RAM:
systemctl disable --now brickman.service
systemctl mask connman-wait-online.service
systemctl mask systemd-fsck-root.service
systemctl mask apt-daily.service apt-daily.timer
```

### 3. Install Systemd Service
Copy `ev3-web.service` to the EV3:
```bash
scp ev3-web.service robot@192.168.2.2:/tmp/
ssh robot@192.168.2.2 "sudo mv /tmp/ev3-web.service /etc/systemd/system/ && sudo systemctl daemon-reload && sudo systemctl enable ev3-web.service"
```

### 4. 1-Click Deploy from Windows
Run this command from the project root on Windows:
```powershell
.\deploy.ps1 -TargetIp 192.168.2.2
```
*This command cross-compiles for ARMv5te, uploads the binary, and restarts the service in 4 seconds.*

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
