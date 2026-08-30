# Project Plan: Web-Based Motor Control on LEGO Mindstorms EV3

> **Design Principle:** Assume errors will occur. Flash the SD card only one time. Test 100% of user interface and API logic on the Windows host using simulation first. Deploy to EV3 over USB in less than 5 seconds.

---

## Table of Contents
1. [Core Principles and Constraints](#1-core-principles-and-constraints)
2. [Potential Risks and Solutions](#2-potential-risks-and-solutions)
3. [Phase 0: Windows Host Cross-Compilation Setup](#3-phase-0-windows-host-cross-compilation-setup)
4. [Phase 1: One-Time SD Card Setup and USB Networking](#4-phase-1-one-time-sd-card-setup-and-usb-networking)
5. [Phase 2: EV3 Operating System Boot Optimization](#5-phase-2-ev3-operating-system-boot-optimization)
6. [Phase 3: Rust Server and Sysfs Driver Architecture](#6-phase-3-rust-server-and-sysfs-driver-architecture)
7. [Phase 4: Embedded Web Dashboard and Host Simulation](#7-phase-4-embedded-web-dashboard-and-host-simulation)
8. [Phase 5: Automated One-Click Deployment Script (`deploy.ps1`)](#8-phase-5-automated-one-click-deployment-script-deployps1)
9. [Phase 6: Verification and Failure Diagnostics Matrix](#9-phase-6-verification-and-failure-diagnostics-matrix)

---

## 1. Core Principles and Constraints

### Rule 1: Flash the SD Card Only Once
Moving the MicroSD card between the PC and the EV3 brick takes 3 minutes per cycle.
Follow this procedure instead:
- Flash the MicroSD card one time with `ev3dev-stretch`.
- Send all future program updates through the Mini-USB cable over SSH/SCP. This takes 4 seconds.

### Rule 2: Test in Host Simulation Before Hardware Deployment
Do not send untested code to the EV3 brick.
- Test all HTML, CSS, JavaScript, HTTP routes, and state logic on Windows first with `cargo run -- --mock`.
- Deploy to physical hardware only after host simulation passes all tests.

### Rule 3: Do Not Perform Synchronous File I/O on HTTP Request Threads
On a 300 MHz ARM9 processor:
- Opening and reading sysfs files takes 2 to 8 milliseconds per file.
- If four motors are polled 10 times per second on the HTTP thread, CPU usage reaches 100%.
- **Solution:** A background thread polls sysfs files every 50ms and updates an in-memory cache. HTTP requests read from memory in 0.05ms.

---

## 2. Potential Risks and Solutions

| Risk or Failure Mode | Cause | Solution |
| :--- | :--- | :--- |
| **Missing ARM Linker on Windows** | Windows does not have an ARM GNU linker in PATH. | Use the included `zig-linker.py` and Zig LLD linker in `.cargo/config.toml`. |
| **Windows Does Not Recognize EV3 USB** | Windows 10/11 does not load the default RNDIS driver. | Select "Microsoft USB Ethernet/RNDIS Gadget" in Windows Device Manager. |
| **SSH Lockout During Optimization** | Disabling network services before static IP setup stops SSH. | Test network services step-by-step before restarting the system. |
| **`Illegal instruction` Crash on EV3** | Binary compiled for ARMv6 or ARMv7 instead of ARMv5te. | Set target to `armv5te-unknown-linux-musleabi` and CPU to `arm926ej-s`. |
| **Out-Of-Memory (OOM) Process Termination** | Runtime uses more than 25 MB free RAM. | Statically link a native Rust executable (< 700 KB binary, < 3 MB RAM RSS). |
| **Motor Commands Dropped** | Writing parameters in wrong state sequence. | Use the state machine wrapper in `src/sysfs/motor.rs`. |

---

## 3. Phase 0: Windows Host Cross-Compilation Setup

The project uses `zig` as a standalone cross-linker. This allows direct compilation on Windows without Docker.

### Step 0.1: Install Zig
Run this command in PowerShell:
```powershell
winget install -e --id zig.zig
```

### Step 0.2: Add ARMv5te Target in Rust
```powershell
rustup target add armv5te-unknown-linux-musleabi
```

### Step 0.3: Configure Cargo Cross-Linker (`.cargo/config.toml`)
The project includes `.cargo/config.toml` pre-configured to use `zig-lld-arm.cmd`:
```toml
[target.armv5te-unknown-linux-musleabi]
linker = "zig-lld-arm.cmd"
rustflags = [
    "-C", "target-cpu=arm926ej-s",
    "-C", "linker-flavor=ld.lld"
]
```

### Step 0.4: Build the ARM Binary
```powershell
cargo build --target armv5te-unknown-linux-musleabi --release
```
*Expected Result:* Binary created at `target\armv5te-unknown-linux-musleabi\release\ev3-web-motor` (size: ~650 KB).

---

## 4. Phase 1: One-Time SD Card Setup and USB Networking

### Step 1.1: Flash the OS Image
1. Download `ev3dev-stretch-ev3-generic-2020-04-10.img.xz`.
2. Flash the image to a MicroSD card (4GB to 32GB) with BalenaEtcher.
3. Insert the card into the EV3 brick and turn on power.

### Step 1.2: Connect USB Cable and Configure Windows Driver
1. Connect the Mini-USB cable from EV3 PC port to Windows USB port.
2. Open Windows **Device Manager** (`Win + X` -> `Device Manager`).
3. If `RNDIS/Ethernet Gadget` shows a warning icon:
   - Right-click device -> **Update driver**.
   - Select **Browse my computer for drivers** -> **Let me pick from a list**.
   - Select **Network adapters** -> Manufacturer: **Microsoft** -> Model: **USB Ethernet/RNDIS Gadget**.
   - Click Next and complete driver setup.

### Step 1.3: Verify SSH Connection
```powershell
ping 192.168.2.2
ssh robot@192.168.2.2
# Default password: maker
```

---

## 5. Phase 2: EV3 Operating System Boot Optimization

Execute these commands over SSH on the EV3 brick to decrease boot time from 120 seconds to 10–15 seconds:

```bash
sudo su

# 1. Disable LCD GUI (saves 20MB RAM and 40% CPU)
systemctl disable --now brickman.service

# 2. Disable network wait blockers
systemctl mask connman-wait-online.service
systemctl mask systemd-networkd-wait-online.service

# 3. Disable filesystem check on slow SD card during boot
systemctl mask systemd-fsck-root.service

# 4. Disable package manager background timers
systemctl mask apt-daily.service apt-daily.timer
systemctl mask apt-daily-upgrade.service apt-daily-upgrade.timer
```

---

## 6. Phase 3: Rust Server and Sysfs Driver Architecture

### Step 3.1: Hardware Driver Options

1. **Custom Zero-Dependency Sysfs Driver (Default):**
   - Directly reads and writes `/sys/class/tacho-motor/`.
   - Has zero external crate dependencies.
   - Supports in-memory simulation on Windows.
   - Keeps binary size under 700 KB.

2. **`ev3dev-lang-rust` Crate (Alternative Option):**
   - Provides abstractions for motors, sensors (gyro, ultrasonic, color), LEDs, and sound.
   - Use this when adding sensor inputs to the project.

### Step 3.2: REST API Endpoints

| Method | Endpoint | Payload | Description |
| :--- | :--- | :--- | :--- |
| `GET` | `/` | None | Returns embedded HTML5 dashboard. |
| `GET` | `/api/status` | None | Returns telemetry for all four ports (A, B, C, D). |
| `POST` | `/api/motor/{port}/run-forever` | `{"speed": 500}` | Runs motor at set speed. |
| `POST` | `/api/motor/{port}/run-timed` | `{"speed": 500, "time_ms": 1000, "stop_action": "brake"}` | Runs motor for set duration in ms. |
| `POST` | `/api/motor/{port}/run-to-rel-pos` | `{"speed": 400, "position_sp": 360, "stop_action": "hold"}` | Steps motor by relative degree count. |
| `POST` | `/api/motor/{port}/stop` | `{"action": "coast" \| "brake" \| "hold"}` | Stops motor with configured mode. |
| `POST` | `/api/tank-drive` | `{"left_port":"B", "right_port":"C", "left_speed":500, "right_speed":500}` | Drives left and right motors together. |
| `POST` | `/api/emergency-stop` | None | **Emergency Stop**: stops all motors immediately. |

---

## 7. Phase 4: Embedded Web Dashboard and Host Simulation

The web interface is written in standard HTML5, CSS3, and JavaScript. The binary embeds these files using `include_str!`.

### Run Simulation on Windows
```powershell
cargo run -- --mock --port 8080
```
Open `http://localhost:8080` in your web browser.

**Features Available in Simulation:**
- 4 motor status cards with speed, position, and power gauges.
- Interactive Tank Drive D-Pad and keyboard shortcuts (`WASD`, Arrow keys).
- Global Emergency Stop button.
- Live telemetry update loop running every 100ms.

---

## 8. Phase 5: Automated One-Click Deployment Script (`deploy.ps1`)

Run this single command from PowerShell on Windows:
```powershell
.\deploy.ps1 -TargetIp 192.168.2.2
```

**Actions Executed by `deploy.ps1`:**
1. Cross-compiles the release binary for ARMv5te musl in 2 seconds.
2. Checks connection to the EV3 brick.
3. Stops the running service on the EV3 to prevent file locking.
4. Copies the binary to `/home/robot/ev3-web-motor` via SCP.
5. Restarts `ev3-web.service` on the EV3.

---

## 9. Phase 6: Verification and Failure Diagnostics Matrix

### Verification Sequence
1. **Unit Tests:** Run `cargo test` on host (all tests must pass).
2. **Local Simulation:** Run `cargo run -- --mock --port 8080` and test controls in browser.
3. **Cross-Build:** Run `cargo build --target armv5te-unknown-linux-musleabi --release` (must produce `< 700 KB` binary).
4. **Deploy:** Run `.\deploy.ps1` (deploys in 4 seconds).
5. **Hardware Test:** Connect motor to Port A, open `http://192.168.2.2/`, and click "Run Forward".
6. **Emergency Stop Test:** Click "STOP ALL" while motor runs; motor must stop immediately.

### Diagnostics Reference

| Problem | Cause | Solution |
| :--- | :--- | :--- |
| `scp: Text file busy` | Binary is running on EV3 during upload. | Run `sudo systemctl stop ev3-web.service` before upload (handled automatically by `deploy.ps1`). |
| Motor disconnected in UI | Cable not inserted completely into port. | Push RJ12 connector until it clicks into place. |
| Motor stops immediately | Target speed is higher than motor maximum speed. | Set speed between -1050 and +1050 for Large Motor, or -1560 and +1560 for Medium Motor. |
| UI stops updating | Browser tab throttled background JavaScript timer. | Focus the tab or click on the dashboard window. |
