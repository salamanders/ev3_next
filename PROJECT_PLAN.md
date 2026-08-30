# Project Plan & Progress: Web-Based Motor Control on LEGO Mindstorms EV3

> **Design Principle:** Assume errors will occur. Flash the SD card only one time. Test 100% of user interface and API logic on the Windows host using simulation first. Deploy to EV3 over USB in less than 5 seconds.

---

## 📊 Overall Progress Summary

| Phase | Description | Status |
| :--- | :--- | :--- |
| **Phase 0** | Windows Host Cross-Compilation Toolchain | ✅ **COMPLETE** |
| **Phase 1** | One-Time SD Card Setup & USB Networking | ⏳ **PENDING (Physical Hardware)** |
| **Phase 2** | EV3 OS Boot Optimization & Service Setup | ⏳ **PENDING (Physical Hardware)** |
| **Phase 3** | Rust Server & Sysfs Driver Architecture | ✅ **COMPLETE** |
| **Phase 4** | Embedded Web Dashboard & Host Simulation | ✅ **COMPLETE** |
| **Phase 5** | 1-Click Deployment Scripts (`deploy.ps1`) | ✅ **COMPLETE** |
| **Phase 6** | End-to-End Verification & Benchmarking | 🔄 **HOST VERIFIED / HARDWARE PENDING** |

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
9. [Phase 6: Verification Checklist & Diagnostics Matrix](#9-phase-6-verification-checklist--diagnostics-matrix)

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

- [x] **Step 0.1: Install Zig Compiler** (`winget install -e --id zig.zig`)
- [x] **Step 0.2: Add ARMv5te Target in Rust** (`rustup target add armv5te-unknown-linux-musleabi`)
- [x] **Step 0.3: Configure Cargo Cross-Linker (`.cargo/config.toml`)**
  ```toml
  [target.armv5te-unknown-linux-musleabi]
  linker = "zig-lld-arm.cmd"
  rustflags = [
      "-C", "target-cpu=arm926ej-s",
      "-C", "linker-flavor=ld.lld"
  ]
  ```
- [x] **Step 0.4: Dynamic Linker Adapter (`zig-linker.py` / `zig-lld-arm.cmd`)**
  - Adapts Rust LLD arguments for `zig ld.lld -m armelf_linux_eabi`.
- [x] **Step 0.5: Verify Cross-Build**
  - Ran `cargo build --target armv5te-unknown-linux-musleabi --release`.
  - Created standalone static ARMv5te binary at `target\armv5te-unknown-linux-musleabi\release\ev3-web-motor` (size: **658 KB**).

---

## 4. Phase 1: One-Time SD Card Setup and USB Networking

- [ ] **Step 1.1: Download the OS Image**
  - Download [`ev3dev-stretch-ev3-generic-2020-04-10.zip`](https://github.com/ev3dev/ev3dev/releases/download/ev3dev-stretch-2020-04-10/ev3dev-stretch-ev3-generic-2020-04-10.zip) from the [ev3dev GitHub Releases page](https://github.com/ev3dev/ev3dev/releases/tag/ev3dev-stretch-2020-04-10).
- [ ] **Step 1.2: Flash the MicroSD Card**
  - Use a 4GB to 32GB MicroSDHC card with BalenaEtcher.
- [ ] **Step 1.3: Initial Boot on EV3**
  - Insert card into EV3 slot and press Center Button to boot (takes 1–2 minutes on first boot).
- [ ] **Step 1.4: Connect USB Cable & Verify Windows Driver**
  - Connect Mini-USB cable between EV3 PC port and Windows PC.
  - Test ping in PowerShell: `ping 192.168.2.2`.
  - If ping fails, update device driver in Device Manager to "Microsoft USB Ethernet/RNDIS Gadget".
- [ ] **Step 1.5: Test SSH Access**
  - Connect with `ssh robot@192.168.2.2` (password: `maker`).

---

## 5. Phase 2: EV3 Operating System Boot Optimization

- [ ] **Step 2.1: Mask Blocking Daemons on EV3**
  ```bash
  sudo su
  systemctl disable --now brickman.service
  systemctl mask connman-wait-online.service
  systemctl mask systemd-networkd-wait-online.service
  systemctl mask systemd-fsck-root.service
  systemctl mask apt-daily.service apt-daily.timer
  systemctl mask apt-daily-upgrade.service apt-daily-upgrade.timer
  exit
  ```
- [ ] **Step 2.2: Install Systemd Service File**
  ```powershell
  scp .\ev3-web.service robot@192.168.2.2:/tmp/
  ssh robot@192.168.2.2 "sudo mv /tmp/ev3-web.service /etc/systemd/system/ && sudo systemctl daemon-reload && sudo systemctl enable ev3-web.service"
  ```

---

## 6. Phase 3: Rust Server and Sysfs Driver Architecture

- [x] **Step 3.1: Package Configuration (`Cargo.toml`)**
  - Configured `opt-level = "z"`, `lto = true`, `strip = true`, `panic = "abort"`.
- [x] **Step 3.2: Real Linux Sysfs Driver (`src/sysfs/motor.rs`)**
  - Port enumeration for `/sys/class/tacho-motor/` (Ports A, B, C, D).
  - Text parameter writing (`speed_sp`, `duty_cycle_sp`, `position_sp`, `time_sp`, `stop_action`).
  - Command dispatch (`run-forever`, `run-timed`, `run-to-rel-pos`, `run-direct`, `stop`, `reset`).
- [x] **Step 3.3: In-Memory Mock Simulator (`src/sysfs/mock.rs`)**
  - Physics-accurate motor simulation with position integration, timed runs, degree stepping, and E-Stop.
- [x] **Step 3.4: Controller with Non-Blocking Telemetry Cache (`src/controller.rs`)**
  - 50ms background thread updates in-memory cache; HTTP handlers read in < 0.05ms.
- [x] **Step 3.5: REST API & Static Router (`src/web/router.rs`, `src/web/handlers.rs`)**
  - Full REST API with CORS preflights and embedded SPA delivery.
- [x] **Step 3.6: Automated Unit Tests**
  - 4 automated tests in `src/sysfs/mock.rs` covering clamping, state, E-Stop, and position integration.

---

## 7. Phase 4: Embedded Web Dashboard and Host Simulation

- [x] **Step 4.1: Responsive HTML5 User Interface (`web_assets/index.html`)**
  - 4 motor status cards, gauges for speed (RPM) and angle, duty cycle bars, and D-Pad.
- [x] **Step 4.2: Dark-Theme Stylesheet (`web_assets/style.css`)**
  - Clean responsive grid layout for mobile and desktop screens.
- [x] **Step 4.3: Client JavaScript (`web_assets/app.js`)**
  - 100ms live polling loop, RTT latency counter, keyboard controls (`WASD` / Arrow keys / Spacebar), and E-Stop.
- [x] **Step 4.4: Binary Asset Embedding**
  - All web assets embedded into executable with `include_str!`.
- [x] **Step 4.5: Host Simulation Verification**
  - Verified on Windows host via `cargo run -- --mock --port 8888` using PowerShell automated HTTP tests.

---

## 8. Phase 5: Automated One-Click Deployment Script (`deploy.ps1`)

- [x] **Step 5.1: PowerShell Deployment Pipeline (`deploy.ps1`)**
  - Dynamic PATH lookup for Zig and Cargo.
  - Automatic cross-compilation to ARMv5te musl in 2 seconds.
  - Remote service shutdown to prevent Linux `text file busy` lockouts.
  - SCP upload to `/home/robot/ev3-web-motor`.
  - Service restart and verification output.
- [x] **Step 5.2: Bash Deployment Script (`deploy.sh`)**
  - Compatible with Linux and macOS hosts.
- [x] **Step 5.3: Production Systemd Service (`ev3-web.service`)**
  - Configured with `CPUSchedulingPolicy=rr` and `Restart=always`.

---

## 9. Phase 6: Verification Checklist & Diagnostics Matrix

### Verification Checklist
- [x] **Host Unit Tests:** `cargo test` passes 4/4 tests.
- [x] **Host API Tests:** Live HTTP endpoints verified on Windows mock server.
- [x] **Static Asset Delivery:** HTML, CSS, JS served correctly with proper Content-Type headers.
- [x] **ARMv5te Musl Cross-Compilation:** Standalone binary created without Docker (658 KB).
- [ ] **On-Hardware USB Deployment:** Run `.\deploy.ps1 -TargetIp 192.168.2.2`.
- [ ] **Live Motor Hardware Actuation:** Verify physical motors spin on Ports A, B, C, D via `http://192.168.2.2/`.
- [ ] **Hardware Emergency Stop:** Verify physical motors halt in < 5ms upon pressing E-Stop.

---

### Diagnostics Reference

| Problem | Cause | Solution |
| :--- | :--- | :--- |
| `scp: Text file busy` | Binary is running on EV3 during upload. | Run `sudo systemctl stop ev3-web.service` before upload (handled automatically by `deploy.ps1`). |
| Motor disconnected in UI | Cable not inserted completely into port. | Push RJ12 connector until it clicks into place. |
| Motor stops immediately | Target speed is higher than motor maximum speed. | Set speed between -1050 and +1050 for Large Motor, or -1560 and +1560 for Medium Motor. |
| UI stops updating | Browser tab throttled background JavaScript timer. | Focus the tab or click on the dashboard window. |
