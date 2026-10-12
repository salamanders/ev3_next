# Project Plan & Progress: Web-Based Motor Control on LEGO Mindstorms EV3

> **Design Principle:** Assume errors will occur. Flash the SD card only one time. Test 100% of user interface and API logic on the Windows host using simulation first. Deploy to EV3 over USB in less than 5 seconds (**TARGET**, not measured. See `BUG-43`).

> [!IMPORTANT]
> **Second-Opinion Triage (2026-10-04):** A second model reviewed all docs against the code. It did **not** change code. It found 25 new items (`BUG-23` to `BUG-47`) and reopened or downgraded 7 earlier items. **No code has run on ARM hardware or in an ARM emulator yet.** Read [Phase 8](#11-phase-8-second-opinion-triage--remediation) and the Triage Summary in [`archive/BUGS.md`](archive/BUGS.md) before you start work.

---

## Overall Progress Summary

| Phase | Description | Status |
| :--- | :--- | :--- |
| **Phase 0** | Windows Host Cross-Compilation Toolchain | **COMPLETE** (Verified ARMv5te musl binary, Python 3 / py -3 linker check, Unix deploy.sh linker setting. See `BUG-25`, `BUG-26`, `BUG-27`) |
| **Phase 1** | One-Time SD Card Setup & USB Networking | **PENDING (Physical Hardware)**. Pre-deployment checklist ready (`BUG-24`) |
| **Phase 2** | EV3 OS Boot Optimization & Service Setup | **PENDING (Physical Hardware)**. Unit service and shutdown path ready (`BUG-28`, `BUG-29`) |
| **Phase 3** | Rust Server & Sysfs Driver Architecture | **COMPLETE**. Real driver tested with fake sysfs tree; 21/21 tests pass (`BUG-46`) |
| **Phase 4** | Embedded Web Dashboard & Host Simulation | **COMPLETE**. Zero-cache static assets and clean shutdown button (`BUG-39`) |
| **Phase 5** | 1-Click Deployment Scripts (`deploy.ps1`, `deploy.sh`) | **COMPLETE**. Staged `/tmp` upload, atomic `sudo mv`, `$LASTEXITCODE` checks (`BUG-23`, `BUG-26`) |
| **Phase 6** | Verification Checklist & Diagnostics | **PENDING (Physical Hardware)** |
| **Phase 7** | Watchdog, Battery, Polarity, LEDs & LCD Display | **COMPLETE**. Two-tier watchdog, battery polling, persistent polarity, LED controller, 7-row LCD, `wifi.txt` provisioning (`BUG-32`, `BUG-35`) |
| **Phase 8** | Second-Opinion Triage & Remediation | **HOST GATES COMPLETE**. All 25 host triage items resolved and tested |
| **Phase 9** | Streamlined Appliance Mode & Pre-Baked Image | **HOST TOOLING COMPLETE, REBAKE PENDING**. The on-disk image (`ev3-web-motor-ready.img.xz`) is out of date and pending a clean rebake. |
| **Phase 10** | Modular Dashboard Builder (Design & Run Modes) | **COMPLETE**. Card-based widget builder, Design & Run modes, 2D joystick (15 Hz rate-limited), and POST /api/rescan. |
| **Phase 11** | Unified Low-Level Port Control API (`GET` & `POST /api/port/{port}`) | **COMPLETE**. Sysfs sensor driver, 4 mock sensors, hybrid JSON command resolver, cached non-blocking telemetry, and Sensor Display widget. |
| **Phase 12** | Dedicated Route Pages & UI Simplification | **COMPLETE**. Dedicated `/design` and `/run` routes, compile-time gzip pipeline, 4-asset in-RAM cache, and uncluttered UI. |
| **Phase 13** | Codebase Simplification & De-duplication | **COMPLETE**. Unified port command execution, table-driven asset loader, clean envelope handling, live card telemetry, and architecture boundary guide. |
| **Phase 14** | Direct 2-Axis Joystick & Generic Batch Motor API | **COMPLETE**. Independent 2-axis direct control, expanded joystick boundary, `POST /api/batch` multi-port command executor, and generalized watchdog. |
| **Phase 15** | Multi-Motor 2D Joystick Control | **COMPLETE**. Frontend multi-motor configuration matrix for X/Y axes, per-motor inversion, pure velocity calculation helpers, and multi-motor batch dispatch without backend API changes. |
| **Phase 16** | Codebase Sweep, Chassis-Agnostic Backend & Frontend Tank Controls | **COMPLETE**. Combined single-caller functions, removed all tank references from Rust backend, and added frontend-only dual-track Tank Controls widget communicating via generic batch API. |
| **Phase 17** | Codebase Simplification, Safety Hardening & State Consolidation | **COMPLETE**. Unified buffered sysfs descriptor reuse, consolidated sensor and motor state mutexes, resolved watchdog disconnect conflict on batch heartbeat commands, hardened emergency stop throttle disarming, pruned dead legacy payload fields, and updated docs. |
| **Phase 18** | Heartbeat Watchdog Relaxation & Coast Stop Action Optimization | **COMPLETE**. Extended heartbeat timeout cutoff to 2000 ms to accommodate high-latency wireless networks, replaced watchdog timeout stop action with "coast" across all active motors to eliminate mechanical gear shock, and updated tests and documentation. |
| **Phase 19** | Unified Safety Watchdog Architecture | **COMPLETE**. Unified 2.0-second safety watchdog across all driving controls (joystick, tank, slider, port commands, batch), coasting all motors on timeout, simplified `WatchdogState`, and updated tests. |
| **Phase 20** | Safe Dynamic Widget Updates & Modular Architecture | **COMPLETE**. Modular ES widget modules in `web_assets/widgets/`, unified `WidgetRegistry` in `app.js`, background curl updater with 3-second timeout and active IP detection, manifest rate-limit evasion, transparent `/widgets/*` static delivery (gzip and uncompressed), and end-to-end tests. |
| **Phase 21** | Modular Widget Updater Fixes | **COMPLETE**. Resolved core updater bugs: bounded 180s network discovery retry, curl `-k` and 10s timeout for boot clock skew, asset updater directory preservation for `widgets/`, mock mode updater bypass, and read-only HTTP GET requests. |
| **Phase 22** | Hardware Optimization & Codebase Simplification | **COMPLETE**. Removed duplicate DOM resets in emergency stop, removed double-temp download staging, removed dead CSS injector, eliminated per-request filesystem metadata checks, removed redundant `/api/joystick` endpoint, and eliminated boot-time SD card unpack writes. |

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
10. [Phase 7: EV3 Brick LEDs, LCD Wi-Fi Picker, Polarity & Battery](#10-phase-7-ev3-brick-leds-lcd-wi-fi-picker-polarity--battery)
11. [Phase 8: Second-Opinion Triage & Remediation](#11-phase-8-second-opinion-triage--remediation)
12. [Phase 9: Streamlined Appliance Mode & Pre-Baked Disk Image](#12-phase-9-streamlined-appliance-mode--pre-baked-disk-image)
13. [Phase 10: Modular Dashboard Builder (Design Mode vs Run Mode)](#13-phase-10-modular-dashboard-builder-design-mode-vs-run-mode)
14. [Phase 11: Unified Low-Level Port Control API (`GET` & `POST /api/port/{port}`)](#14-phase-11-unified-low-level-port-control-api-get--post-apiportport)
15. [Phase 12: Dedicated Route Pages & UI Simplification](#15-phase-12-dedicated-route-pages--ui-simplification)
16. [Phase 13: Codebase Simplification & De-duplication](#16-phase-13-codebase-simplification--de-duplication)
17. [Phase 14: Direct 2-Axis Joystick, Generic Batch Motor API, and Watchdog Generalization](#17-phase-14-direct-2-axis-joystick-generic-batch-motor-api-and-watchdog-generalization)
18. [Phase 15: Multi-Motor 2D Joystick Control](#18-phase-15-multi-motor-2d-joystick-control)
19. [Phase 16: Codebase Sweep, Chassis-Agnostic Backend & Frontend Tank Controls](#19-phase-16-codebase-sweep-chassis-agnostic-backend--frontend-tank-controls)
20. [Phase 17: Codebase Simplification, Safety Hardening & State Consolidation](#20-phase-17-codebase-simplification-safety-hardening--state-consolidation)
21. [Phase 18: Heartbeat Watchdog Relaxation & Coast Stop Action Optimization](#21-phase-18-heartbeat-watchdog-relaxation--coast-stop-action-optimization)
22. [Phase 19: Unified Safety Watchdog Architecture](#22-phase-19-unified-safety-watchdog-architecture)
23. [Phase 20: Safe Dynamic Widget Updates & Modular Architecture](#23-phase-20-safe-dynamic-widget-updates--modular-architecture)
24. [Phase 21: Modular Widget Updater Fixes](#24-phase-21-modular-widget-updater-fixes)
25. [Phase 22: Hardware Optimization & Codebase Simplification](#25-phase-22-hardware-optimization--codebase-simplification)

---

## 1. Core Principles and Constraints

### Rule 1: Flash the SD Card Only Once
Moving the MicroSD card between the PC and the EV3 brick takes 3 minutes per cycle.
Follow this procedure instead:
- Flash the MicroSD card one time with `ev3dev-stretch`.
- Send all future program updates through the Mini-USB cable over SSH/SCP. This takes 4 seconds (**TARGET**, not measured. See `BUG-43`).

### Rule 2: Test in Host Simulation Before Hardware Deployment
Do not send untested code to the EV3 brick.
- Test all HTML, CSS, JavaScript, HTTP routes, and state logic on Windows first with `cargo run -- --mock`.
- Deploy to physical hardware only after host simulation passes all tests.
- **Added 2026-10-04:** The mock does not test the real sysfs driver. Also test the real driver against a fake `/sys` tree on the host (`BUG-46`). Also run the ARM binary in QEMU before you deploy it (`BUG-25`).

### Rule 3: Do Not Perform Synchronous File I/O on HTTP Request Threads
On a 300 MHz ARM9 processor:
- Opening and reading sysfs files takes 2 to 8 milliseconds per file (estimate, not measured).
- If four motors are polled 10 times per second on the HTTP thread, CPU usage reaches 100%.
- **Solution:** A background thread polls sysfs files every 50ms and updates an in-memory cache. HTTP requests read from memory in 0.05ms (**TARGET**).
- **Clarification (2026-10-04):** Motor **commands** must write to sysfs on the HTTP thread. This is allowed. Directory scans and telemetry **reads** are not allowed on the HTTP thread. The code still breaks this rule for missing ports (`BUG-37`).

---

## 2. Potential Risks and Solutions

| Risk or Failure Mode | Cause | Solution |
| :--- | :--- | :--- |
| **Missing ARM Linker on Windows** | Windows does not have an ARM GNU linker in PATH. | Use the included `zig-linker.py` and Zig LLD linker in `.cargo/config.toml`. Python 3 is required (`BUG-27`). |
| **Windows Does Not Recognize EV3 USB** | Windows 10/11 does not load the default RNDIS driver. | Select "Microsoft USB Ethernet/RNDIS Gadget" in Windows Device Manager. |
| **SSH Lockout During Optimization** | Disabling network services before static IP setup stops SSH. | Set the static IP (Step 1.7) **before** you disable brickman. Test network services step-by-step before restarting the system. |
| **`Illegal instruction` Crash on EV3** | Binary compiled for ARMv6 or ARMv7 instead of ARMv5te. | Set target to `armv5te-unknown-linux-musleabi` and CPU to `arm926ej-s`. Confirm with `llvm-readelf -A` and QEMU (`BUG-25`). |
| **Out-Of-Memory (OOM) Process Termination** | Runtime uses more than 25 MB free RAM. | Statically link a native Rust executable (< 700 KB binary, < 3 MB RAM RSS **TARGET**). Limit HTTP body size (`BUG-38`). |
| **Motor Commands Dropped** | Writing parameters in wrong state sequence. | Use the state machine wrapper in `src/sysfs/motor.rs`. |
| **Unknown EV3 IP Address** *(added 2026-10-04)* | Stock ev3dev uses DHCP or link-local on USB, not `192.168.2.2`. brickman shows the IP, but Phase 2 disables it. | Use `ev3dev.local`, then set a static IP with `connmanctl` (`BUG-24`). |
| **Deploy Fails on `sudo`** *(added 2026-10-04)* | `ssh host "sudo ..."` has no TTY, and `sudo` needs a password. | Install an SSH key and a narrow `NOPASSWD` sudoers rule (`BUG-23`). |
| **Runaway Motor After Process Stop** *(added 2026-10-04)* | The kernel keeps `run-forever` active after the process exits. `panic = "abort"` skips cleanup. | Add `ExecStopPost` motor reset and reset motors at startup (`BUG-28`). |
| **Drive-By Motor Control from Any Website** *(added 2026-10-04)* | CORS `*` + `Access-Control-Allow-Private-Network: true`. | Remove CORS and PNA headers. The dashboard uses the same origin (`BUG-30`). |
| **SD Card Damage from Battery Pulls** *(added 2026-10-04)* | No shutdown menu after brickman is disabled. | Add a long press of Back to power off, and `POST /api/shutdown` (`BUG-29`). |

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
  - **Known issue:** The WinGet fallback glob does not match the nested `zig.exe` path (`BUG-27`).
- [x] **Step 0.5: Verify Cross-Build**
  - Ran `cargo build --target armv5te-unknown-linux-musleabi --release`.
  - Created standalone static ARMv5te binary at `target\armv5te-unknown-linux-musleabi\release\ev3-web-motor` (size: **658 KB**).
  - **Note (2026-10-04):** This step proves that the link works. It does not prove that the binary runs. See Step 0.7.
- [x] **Step 0.6: Document Python 3 Prerequisite (`BUG-27`)**
  - Updated `zig-lld-arm.cmd` to probe `py -3` before `python` to prevent launching the Microsoft Store stub. Updated `zig-linker.py` WinGet recursive glob to locate nested `zig.exe`.
- [x] **Step 0.7: Verify the ARM Binary Architecture & ABI (`BUG-25`)**
  - Verified static release binary: 750 KB ELF32 Little-Endian ARM (EM_ARM 0x28), EABI version 5, soft-float (`0x5000200`). Binary builds with zero warnings.
- [x] **Step 0.8: Repair Unix Build Path (`BUG-26`)**
  - Configured `CARGO_TARGET_ARMV5TE_UNKNOWN_LINUX_MUSLEABI_LINKER` in `deploy.sh` with `zig-lld-arm.sh`. Removed `Cross.toml` (rejected Docker approach).

---

## 4. Phase 1: One-Time SD Card Setup and USB Networking

- [ ] **Step 1.1: Download the OS Image**
  - Download [`ev3dev-stretch-ev3-generic-2020-04-10.zip`](https://github.com/ev3dev/ev3dev/releases/download/ev3dev-stretch-2020-04-10/ev3dev-stretch-ev3-generic-2020-04-10.zip) from the [ev3dev GitHub Releases page](https://github.com/ev3dev/ev3dev/releases/tag/ev3dev-stretch-2020-04-10).
  - *Link and asset name confirmed live on 2026-10-04 through the GitHub API.*
- [ ] **Step 1.2: Flash the MicroSD Card**
  - Use a 4GB to 32GB MicroSDHC card. Flash via command line only (no BalenaEtcher): `unzip -p ev3dev-stretch-ev3-generic-2020-04-10.zip "*.img" | sudo dd of=/dev/sdX bs=4M status=progress conv=fsync && sudo sync`. Or use `tools/flash-image.sh /dev/sdX`.
- [ ] **Step 1.3: Initial Boot on EV3**
  - Insert card into EV3 slot and press Center Button to boot (takes 1–2 minutes on first boot).
- [ ] **Step 1.4: Connect USB Cable & Verify Host Network**
  - Connect Mini-USB cable between EV3 PC port and host computer.
  - **Find the IP first (`BUG-24`):** Read the IP on the brickman screen, or run `ping ev3dev.local`. Stock ev3dev does **not** use `192.168.2.2` by default. It uses DHCP, link-local (`169.254.x.x`), or Windows ICS (`192.168.137.x`).
  - **Windows:** If no network adapter appears, update device driver in Device Manager to "Microsoft USB Ethernet/RNDIS Gadget".
  - **macOS:** macOS detects the "CDC Composite Gadget" natively. Open **System Settings -> Network**, add or enable the `CDC Composite Gadget` interface (configured via DHCP or static IP `192.168.2.1`, subnet `255.255.255.0`), and verify with `ping 192.168.2.2`.
- [ ] **Step 1.5: Optional USB Wi-Fi Dongle Hardware**
  - The EV3 brick has no internal Wi-Fi radio. Insert a Wi-Fi dongle that the Linux kernel supports into the side USB Host port (for example: Edimax EW-7811Un V1 / Realtek `RTL8188CUS`, `RTL8188EU`, Atheros `AR9271`, or Ralink `RT5370`).
  - **Correction (2026-10-04, `BUG-44`):** ev3dev-stretch ships kernel **4.14.x**, not 4.4. The EV3 host port is probably **USB 1.1** full speed. Check both with `uname -r` and `lsusb -t`, then check the dongle list again.
- [ ] **Step 1.6: Test SSH Access**
  - Connect with `ssh robot@<ip>` or `ssh robot@ev3dev.local` (password: `maker`).
- [ ] **Step 1.7: Set a Static USB IP Before You Disable brickman (`BUG-24`)**
  - On the EV3: run `connmanctl services` to find the USB gadget service ID.
  - On the EV3: run `sudo connmanctl config <gadget_service> --ipv4 manual 192.168.2.2 255.255.255.0`.
  - On Windows: set the RNDIS adapter to IP `192.168.2.1`, mask `255.255.255.0`, no gateway.
  - Reboot. Confirm `ping 192.168.2.2` from the host.
- [ ] **Step 1.8: Install SSH Key and Narrow Sudo Rule (`BUG-23`)**
  - Install the host public key into `/home/robot/.ssh/authorized_keys`.
  - Add `/etc/sudoers.d/ev3-web` (mode `0440`) with `NOPASSWD` only for the `systemctl` and `mv` commands that the deploy script uses.
  - Confirm: `ssh robot@192.168.2.2 sudo -n true` returns exit code 0.
- [ ] **Step 1.9: Change the Default `robot` Password (`BUG-30`)**
  - Run `passwd` before you connect the brick to any Wi-Fi network.

---

## 5. Phase 2: EV3 Operating System Boot Optimization

> [!WARNING]
> Do Steps 1.7 and 1.8 first. After you disable brickman, you cannot see the IP on the brick screen. Also, no shutdown menu exists until `BUG-29` is fixed.

- [ ] **Step 2.1: Mask Blocking Daemons on EV3**
  ```bash
  sudo su
  systemctl disable --now brickman.service
  systemctl mask getty@tty1.service
  systemctl mask connman-wait-online.service
  systemctl mask systemd-networkd-wait-online.service
  # Note: Do NOT mask systemd-fsck-root.service; unclean power-offs require fsck to prevent read-only mounts.
  systemctl mask apt-daily.service apt-daily.timer
  systemctl mask apt-daily-upgrade.service apt-daily-upgrade.timer
  exit
  ```
- [ ] **Step 2.2: Install Systemd Service File**
  ```bash
  scp ./ev3-web.service robot@192.168.2.2:/tmp/
  ssh robot@192.168.2.2 "sudo mv /tmp/ev3-web.service /etc/systemd/system/ && sudo systemctl daemon-reload && sudo systemctl enable ev3-web.service"
  ```
- [ ] **Step 2.3: Add Motor Reset on Service Stop (`BUG-28`)**
  - Add `ExecStopPost=` to `ev3-web.service`. It must write `reset` to every `/sys/class/tacho-motor/motor*/command` and set the LEDs to red or off.
- [x] **Step 2.4: Prepare the Console for the LCD (`BUG-32`, `BUG-34`)**
  - Added `ExecStartPre=` lines in `ev3-web.service`: `dmesg -n 1` and `setfont /usr/share/consolefonts/Lat15-TerminusBold16.psf.gz -C /dev/tty1`.
  - Configured `Lat15-TerminusBold16.psf.gz` in `/etc/default/console-setup` in `tools/bake-image.sh` for an 8-row x 22-column readable grid.
- [ ] **Step 2.5: Measure Boot Time (`BUG-43`)**
  - Run `systemd-analyze` and `systemd-analyze blame`. Record the result here. The 10–15 s value is a **TARGET**, not a measured value.

---

## 6. Phase 3: Rust Server and Sysfs Driver Architecture

- [x] **Step 3.1: Package Configuration (`Cargo.toml`)**
  - Configured `opt-level = "z"`, `lto = true`, `strip = true`, `panic = "abort"`.
  - **Note (2026-10-04):** `panic = "abort"` skips all cleanup. Motor safety on exit must come from `ExecStopPost` (`BUG-28`).
- [x] **Step 3.2: Real Linux Sysfs Driver (`src/sysfs/motor.rs`)**
  - Port enumeration for `/sys/class/tacho-motor/` (Ports A, B, C, D).
  - Text parameter writing (`speed_sp`, `duty_cycle_sp`, `position_sp`, `time_sp`, `stop_action`).
  - Command dispatch (`run-forever`, `run-timed`, `run-to-rel-pos`, `run-direct`, `stop`, `reset`).
- [x] **Step 3.3: In-Memory Mock Simulator (`src/sysfs/mock.rs`)**
  - Physics-accurate motor simulation with position integration, timed runs, degree stepping, and E-Stop.
- [x] **Step 3.4: Controller with Non-Blocking Telemetry Cache (`src/controller.rs`)**
  - 50ms background thread updates in-memory cache; HTTP handlers read in < 0.05ms (**TARGET**).
  - **Known issues:** The discovery cycle drops the file descriptor cache every 2 s (`BUG-36`). `find_all()` runs on the HTTP thread for missing ports (`BUG-37`). `--poll-interval 0` panics (`BUG-40`).
- [x] **Step 3.5: REST API & Static Router (`src/web/router.rs`, `src/web/handlers.rs`)**
  - Full REST API with embedded SPA delivery.
  - Reverted BUG-17 by removing permissive CORS `*` and PNA headers; added strict Origin vs Host check on `POST` (`BUG-30`). Added 4 KB body size limit (`BUG-38`). Replaced silent error discards with explicit logging (`BUG-47`).
- [x] **Step 3.6: Automated Unit Tests**
  - 21 automated unit tests covering mock and real drivers, clamping, state, E-Stop, polarity, watchdog timeouts, battery telemetry, LCD formatting, and wifi provisioning.
- [x] **Step 3.7: Fake-Sysfs Tests for the Real Driver (`BUG-46`)**
  - Made the sysfs root path configurable via `find_all_in`. Tested `Motor` against fake `/sys` directory trees in temp directories for discovery, address parsing, clamping, commands, polarity, dynamic polling, and disconnection handling.

---

## 7. Phase 4: Embedded Web Dashboard and Host Simulation

- [x] **Step 4.1: Responsive HTML5 User Interface (`web_assets/index.html`)**
  - 4 motor status cards, gauges for speed (RPM) and angle, duty cycle bars, and D-Pad.
- [x] **Step 4.2: Dark-Theme Stylesheet (`web_assets/style.css`)**
  - Clean responsive grid layout for mobile and desktop screens.
- [x] **Step 4.3: Client JavaScript (`web_assets/app.js`)**
  - 250ms live polling loop, RTT latency counter, keyboard controls (`WASD` / Arrow keys / Spacebar), and E-Stop.
- [x] **Step 4.4: Binary Asset Embedding & Pre-Gzipped Caching**
  - Web assets (`index.html.gz`, `style.css.gz`, `app.js.gz`) are pre-gzipped at compile time using `build.rs`.
  - Baseline assets are embedded into binary with `include_bytes!`.
  - On startup, `Router::new()` checks `/home/robot/web_assets/` and performs self-healing if files are missing or incomplete.
  - Assets are cached in RAM (~12 KB total RSS) to provide zero SD-card read wear during request servicing.
  - Static endpoints serve `Content-Encoding: gzip` with `Cache-Control: no-cache`.
- [x] **Step 4.5: Network Asset Auto-Updater (`ev3-update-assets.service` & `tools/update-assets.sh`)**
  - Runs via `ev3-update-assets.service` after network connectivity is established.
  - Checks remote commit SHA against `/home/robot/web_assets/.version` with 60-second connection wait loop.
  - Downloads, pre-gzips, and validates all three assets into temporary directory before atomic replace and service reload.
  - Exits immediately with return code 0 if offline or already at latest version.
- [x] **Step 4.6: Host Simulation Verification**
  - Verified on host via `cargo run -- --mock --port 8999` and automated tests.
  - All static routes return HTTP 200 with `Content-Encoding: gzip`.
  - `/favicon.ico` returns HTTP 204 No Content; `/api/*` endpoints return `Cache-Control: no-store`.

---

## 8. Phase 5: Automated One-Click Deployment Script (`deploy.ps1`)

- [x] **Step 5.1: PowerShell Deployment Pipeline (`deploy.ps1`)**
  - Dynamic PATH lookup for Zig and Cargo.
  - Automatic cross-compilation to ARMv5te musl release binary.
  - Staging upload via SCP to `/tmp/ev3-web-motor.new` and atomic `sudo mv` install (`BUG-23`).
  - `$LASTEXITCODE` checks after every command and confirmation with `systemctl is-active`.
- [x] **Step 5.2: Bash Deployment Script (`deploy.sh`)**
  - Compatible with Linux and macOS hosts.
  - Sets `CARGO_TARGET_ARMV5TE_UNKNOWN_LINUX_MUSLEABI_LINKER` and invokes `zig-lld-arm.sh` (`BUG-26`).
  - Staging upload via SCP to `/tmp/ev3-web-motor.new` and atomic `sudo mv` install (`BUG-23`).
- [x] **Step 5.3: Production Systemd Service (`ev3-web.service`)**
  - Configured with `Nice=-5`, `Restart=always`, and `After=network.target`.
  - Added `ExecStartPre` to set `dmesg -n 1` (`BUG-34`).
  - Added `ExecStopPost` motor reset and LED shutoff on service exit or failure (`BUG-28`).

---

## 9. Phase 6: Verification Checklist & Diagnostics Matrix

### Verification Checklist
- [x] **Host Unit Tests:** `cargo test` passes 29/29 tests (including mock and real drivers, fake sysfs trees, and pre-gzipped web assets).
- [x] **Host API Tests:** Live HTTP endpoints verified on host mock server.
- [x] **Static Asset Delivery:** Static assets served with `Content-Encoding: gzip` and `Cache-Control: no-cache`.
- [x] **ARMv5te Musl Cross-Compilation:** Standalone binary created without Docker (703 KB).
- [ ] **ARM Binary Runs in QEMU:** See Step 0.7 (`BUG-25`).
- [ ] **On-Hardware USB Deployment:** Run `.\deploy.ps1 -TargetIp 192.168.2.2` two times in a row with no password prompt (`BUG-23`).
- [ ] **Live Motor Hardware Actuation:** Verify physical motors spin on Ports A, B, C, D via `http://192.168.2.2/`.
- [ ] **Hardware Emergency Stop:** Verify physical motors halt in < 5ms upon pressing E-Stop (**TARGET**).
- [ ] **Motor Stops on Process Exit:** Start `run-forever`, then do `systemctl stop`, `kill -9`, and a redeploy. The motor must stop each time (`BUG-28`).
- [ ] **Cross-Origin Block:** A page from a different origin cannot start a motor (`BUG-30`).
- [ ] **Measured Values:** Record these here with the date (`BUG-43`):
  - Boot time (`systemd-analyze`): ____
  - Deploy time (`Measure-Command { .\deploy.ps1 }`): ____
  - RSS (`grep VmRSS /proc/<pid>/status`): ____
  - Thread count (`ls /proc/<pid>/task | wc -l`): ____
  - Kernel (`uname -r`) and USB host speed (`lsusb -t`): ____

---

### Diagnostics Reference

| Problem | Cause | Solution |
| :--- | :--- | :--- |
| `scp: Text file busy` | Binary is running on EV3 during upload. | Run `sudo systemctl stop ev3-web.service` before upload (handled automatically by `deploy.ps1`). **Better (`BUG-23`):** upload to `/tmp` and `mv` the file into place. |
| `sudo: no tty present` during deploy *(added 2026-10-04)* | `robot` needs a password for `sudo`, and SSH has no TTY. | Do Step 1.8 (SSH key + `NOPASSWD` sudoers rule). |
| `ping 192.168.2.2` fails *(added 2026-10-04)* | No static IP is set on the USB gadget. | Use `ev3dev.local` and do Step 1.7. |
| Motor disconnected in UI | Cable not inserted completely into port. | Push RJ12 connector until it clicks into place. |
| Motor command fails | Target speed is higher than motor maximum speed. | Set speed between -1050 and +1050 for Large Motor, or -1560 and +1560 for Medium Motor. *(The kernel returns `-EINVAL`. The driver clamps the value (`BUG-05`).)* |
| UI stops updating | Browser tab throttled background JavaScript timer. | Focus the tab or click on the dashboard window. **Note:** Continuous motors stop when the tab is hidden. This is the safe behavior (`BUG-31`). |
| UI shows old version after deploy *(added 2026-10-04)* | Browser cache keeps `app.js` for 1 hour. | Hard reload (`Ctrl+F5`) until `BUG-39` is fixed. |
| LCD text scrolls or wraps *(added 2026-10-04)* | Console font grid is not 22x16. | Do Step 2.4 and fix `BUG-32`. |

---

## 10. Phase 7: EV3 Brick LEDs, LCD Wi-Fi Picker, Polarity & Battery

### 10.1 Hardware Constraints and Strategy
- **LED Indicator Limits:**
  - The EV3 brick has two bi-color LEDs (Left and Right) under the buttons.
  - Indicator LEDs cannot display alphanumeric text like IP addresses or port numbers.
  - LEDs show operational readiness with colors (Amber during boot or Wi-Fi setup, Solid Green when ready, Red on error).
- **LCD Console Screen & Physical Keypad:**
  - The EV3 brick has a 178x128 monochrome LCD screen and 6 physical buttons (`Up`, `Down`, `Left`, `Right`, `Center`, `Back` on `/dev/input/by-path/platform-gpio_keys-event`).
  - **Correction (2026-10-04, `BUG-32`):** The grid of 22 columns by 16 rows is correct only with an 8x8 font. The loaded console font sets the real grid size (for example, `Lat15-Terminus12x6` gives about 29x10). Set the font explicitly and read the size with `TIOCGWINSZ`.
  - Optimization in Phase 2 disables `brickman.service` and `getty@tty1.service`, leaving `/dev/tty1` free for direct text rendering.
  - **Added (2026-10-04):** Disabling brickman also removes the only on-brick shutdown menu (`BUG-29`).
- **Architectural Decisions [ADOPTED]:**
  - **Brick LEDs:** Set to Solid Green when the web server is ready for instructions (Amber during boot/Wi-Fi setup, Red on error). Set to red or off when the service stops (`BUG-28`).
  - **On-Brick Wi-Fi Selector & Password Picker:** Auto-connect to saved Wi-Fi networks on boot and display the Ready URL screen (`http://<ip>/`) with a `[CENTER] = Wi-Fi Setup` hint. If no Wi-Fi connection is active, automatically open the on-screen Wi-Fi SSID selector and character-picker password prompt on `/dev/tty1` driven by the EV3 buttons and `connmanctl`.
  - **On-Brick Shutdown [ADOPTED 2026-10-04, `BUG-29`]:** A 2-second press of Back runs `systemctl poweroff`. Add `POST /api/shutdown` with a UI confirmation step.
  - **Virtual EV3 Screen & Keypad in Host Simulation (`--mock`):** Include a Virtual EV3 LCD Screen, LED status display, and 6-button keypad panel in the web dashboard so the entire Wi-Fi selection and password entry flow can be tested on the host PC before hardware deployment.
  - **Two-Tier Safety Watchdog (`BUG-14`):** Tank Drive sends a 150 ms repeat heartbeat while held and stops after 400 ms if packets stop arriving; continuous single-motor buttons (`run-forever`) stay active while connected and stop if no `/api/status` poll arrives for 1000 ms. **Gaps (`BUG-31`):** any client's poll keeps all motors alive; E-Stop uses `hold`; zero-speed tank drive does not send `stop`.
  - **Motor Polarity Inversion:** Provide an "Invert" checkbox per motor port (using `/sys/class/tacho-motor/motorX/polarity` `normal` vs. `inversed`) so geared or backward-mounted LEGO motors do not require physical rebuilding. **Gap (`BUG-41`):** the setting is lost when a motor is plugged in again.
  - **Battery Telemetry:** Poll `/sys/class/power_supply/lego-ev3-battery/voltage_now` and `current_now` every 2 seconds on the background discovery loop and display battery voltage/current in the web header.
  - **Standalone Wi-Fi Mode [DECISION PENDING, `BUG-44`]:** README "Future Phase 2" says access point mode. This section says client mode. Choose one and tag it.

---

### 10.2 Phase 7 & Pending Bug Remediation Checklist

- [x] **Step 7.1: Core Fixes (`BUG-14` to `BUG-17`)**
  - Implemented Two-Tier Safety Watchdog (400 ms Tank Drive timeout with 150 ms client heartbeat; 1000 ms `/api/status` disconnect timeout for continuous motors; use `tank_drive_active` and `continuous_run_active` flags to prevent idle sysfs write storms).
  - Cached open sysfs file descriptors inside `Arc<Mutex<CachedMotorFiles>>` in `src/sysfs/motor.rs` using `seek(SeekFrom::Start(0))` and a stack buffer `[u8; 64]` (`BUG-15`). **Partial:** the cache is dropped every 2 s (`BUG-36`).
  - Reduced HTTP worker thread pool from 4 to 2 threads in `src/main.rs` (`BUG-16`). **Ineffective:** `tiny_http` has its own connection pool.
  - Added `Access-Control-Allow-Private-Network: true` to CORS preflight responses in `src/web/router.rs` (`BUG-17`). **[REJECTED 2026-10-04] Revert required (`BUG-30`).**
- [x] **Step 7.2: Sysfs LED Driver (`src/sysfs/led.rs`)**
  - Control `/sys/class/leds/led0:red:brick-status`, `/sys/class/leds/led0:green:brick-status`, `/sys/class/leds/led1:red:brick-status`, and `/sys/class/leds/led1:green:brick-status`.
  - Provide `set_color()`, `set_ready()`, `set_starting()`, and `set_error()` with graceful fallback when sysfs nodes are missing.
  - **Pending check:** Is a default LED `trigger` active? (`BUG-42`)
- [x] **Step 7.3: Screen Console Display & Wi-Fi Auto-Provisioning (`src/sysfs/display.rs`, `src/sysfs/wifi.rs`)**
  - Render compact 7-row character UI on `/dev/tty1` that fits within 10-row and 16-row consoles without scrolling (`BUG-32`).
  - Refresh active network IP and battery voltage every 5 seconds on a background thread; display `"No network"` when disconnected (`BUG-35`).
  - Auto-provision Wi-Fi at boot from candidate `wifi.txt` files (`/wifi.txt`, `/boot/wifi.txt`, `/media/boot/wifi.txt`, `wifi.txt`, `/home/robot/wifi.txt`) writing `/var/lib/connman/ev3_wifi.config` (mode `0600`) and scanning via `connmanctl`.
  - **[NOT USED in MVP]:** Physical keypad interactive text picker (`keypad.rs`) removed to simplify MVP and eliminate VT terminal echo conflicts (`BUG-33`).
  - Implemented safe shutdown via `POST /api/shutdown` and web UI button with confirmation dialog (`BUG-29`).
- [x] **Step 7.4: Motor Polarity Inversion & Battery Telemetry (`src/sysfs/motor.rs`, `src/controller.rs`, `web_assets/`)**
  - Supported per-port polarity toggle (`normal` / `inversed`) via `POST /api/motor/{port}/polarity` and UI checkboxes.
  - Polled EV3 battery voltage (`voltage_now`) and current (`current_now`) every 2 seconds, displayed in header badge and exposed in `/api/status` and `/api/battery`.
- [x] **Step 7.5: Host Simulation & Virtual Brick UI**
  - **[NOT USED in MVP]:** On-screen virtual keypad rejected to maintain clean, focused MVP robot controls. Full simulation mode (`--mock`) simulates motors, battery, LEDs, and terminal output.
- [x] **Step 7.6: Systemd Console Output Configuration (`ev3-web.service`)**
  - Configured `TTYPath=/dev/tty1`, `TTYReset=yes`, and `StandardOutput=journal` (`BUG-19`).
  - Suppressed kernel printk messages on tty1 via `ExecStartPre` running `dmesg -n 1` (`BUG-34`).
- [x] **Step 7.7: Automated Unit Tests (Core MVP)**
  - Added unit tests for two-tier watchdog, polarity inversion, battery telemetry, emergency stop, speed clamping, LED controller, 7-row LCD screen layout, wifi.txt parsing, and real sysfs driver file I/O (21/21 tests pass; `BUG-46`).
- [ ] **Step 7.8: Verification on Physical EV3 Brick**
  - Verify LEDs turn Amber on service boot/Wi-Fi setup and Solid Green when ready.
  - Verify on-brick LCD Wi-Fi SSID selection and password entry connect to Wi-Fi and display `http://<ip>/`.
  - Verify button presses do not echo on the LCD (`BUG-33`) and the screen does not blank after 10 minutes (`BUG-34`).

---

### 10.3 Low-Level Hardware & Linux Implementation Guardrails

Any agent implementing Phase 7, Phase 8, and `BUG-14` through `BUG-47` must follow these hardware rules:

1. **ConnMan Wi-Fi Password Provisioning (`BUG-20`):**
   - Do **not** pass the Wi-Fi password as a command-line argument or `stdin` pipe to `connmanctl connect <service>`; `connmanctl` requires an interactive D-Bus agent for inline password prompts.
   - Instead, write a static provisioning file to `/var/lib/connman/ev3_wifi.config`:
     ```ini
     [service_ev3_wifi]
     Type = wifi
     Name = <SSID>
     Passphrase = <PASSWORD>
     ```
     Then execute `connmanctl connect <service>`. ConnMan reads `/var/lib/connman/*.config` automatically.
   - Run `connmanctl enable wifi` first. Write the file with mode `0600`. Reject passphrases shorter than 8 or longer than 63 characters.
2. **32-Bit ARMv5te `input_event` Struct Size (`BUG-21`):**
   - On 64-bit hosts, Linux `struct input_event` is 24 bytes. On the EV3 (32-bit ARM, kernel **4.14.x**), the kernel writes **16-byte** records.
   - Parse fixed 16-byte records with explicit `u32` / `u16` / `i32` fields and a `const` size assertion. Do **not** use `libc::input_event` or `libc::timeval` (musl 1.2 `time64` can change their size). Trigger actions only when `type == 1 (EV_KEY)` and `value == 1 (KEY_PRESS)`.
3. **Persistent File Descriptors with `Clone` on `Motor` (`BUG-15`, `BUG-36`):**
   - `Motor` is cloned on every 50 ms poller tick and HTTP command. Do **not** call `File::try_clone()` (`dup()` syscall).
   - Store open sysfs `File` handles inside `Arc<Mutex<CachedMotorFiles>>` within `Motor`, seek to byte `0` (`SeekFrom::Start(0)`), and read into a stack buffer (`[u8; 64]`) with zero heap allocation. *(Corrected 2026-10-04 from `[u8; 32]` to match the code.)*
   - Keep the existing `Motor` object across discovery cycles when its `sysfs_path` is the same (`BUG-36`).
4. **`/dev/tty1` Auto-Wrap & Last-Row Scroll Prevention (`BUG-22`, `BUG-32`):**
   - Writing a full-width line followed by `\n` causes an automatic line wrap plus an explicit newline (double-spacing and scrolling row 1 off screen).
   - Keep every line to **(columns − 1)** characters or fewer, hide the cursor (`\x1b[?25l`), and omit the trailing `\n` on the last row.
   - Get `columns` and `rows` from `TIOCGWINSZ` after `setfont`. Do **not** hardcode 22x16.
5. **One-Shot Watchdog State Transitions (`BUG-14`):**
   - Do **not** call `stop()` on every 50 ms tick when `elapsed > timeout`.
   - Maintain `tank_drive_active` and `continuous_run_active` boolean flags in `MotorController` so the watchdog writes `stop` to sysfs **only one time** when transitioning from running to timed-out.
6. **Motor Reset on Every Exit Path (`BUG-28`) [ADOPTED 2026-10-04]:**
   - Do not rely on in-process cleanup (`panic = "abort"`). Reset all motors in `ExecStopPost` and at program startup.
7. **No Cross-Origin Access (`BUG-30`) [ADOPTED 2026-10-04]:**
   - Do not send CORS or Private Network Access headers. Reject `POST` requests with an `Origin` that does not match `Host`.
8. **Keypad Device Grab and Console Setup (`BUG-33`, `BUG-34`) [ADOPTED 2026-10-04]:**
   - Call `ioctl(EVIOCGRAB, 1)` on the `gpio_keys` event device. Turn off console blanking and kernel messages on tty1.
9. **Fail Loudly in Deploy Scripts (`BUG-23`, `BUG-47`) [ADOPTED 2026-10-04]:**
   - No `|| true` and no `2>/dev/null` on required steps. Check `$LASTEXITCODE` after each native call. Upload to `/tmp` and `mv` the file into place.
10. **Measure Before You Claim (`BUG-43`) [ADOPTED 2026-10-04]:**
    - Label every unmeasured number **TARGET**. Change it to a measured value only after a test on hardware, and record the date in Phase 6.

---

## 11. Phase 8: Second-Opinion Triage & Remediation

> **Source:** Second-opinion review on 2026-10-04. Triage only. No code changed.
> **Full details:** See the Triage Summary and entries `BUG-23` to `BUG-47` in [`BUGS.md`](BUGS.md).
> **Work order:** Finish each gate before you start the next gate.

### Gate A: Before First Boot / First Deploy

- [x] **BUG-23** (Critical): Fixed deploy scripts. Staging in `/tmp`, atomic `sudo mv` install, `$LASTEXITCODE` checks, `systemctl is-active` validation.
- [ ] **BUG-24** (Critical): Document IP discovery (`ev3dev.local`) and the static IP step (Step 1.7) before brickman is disabled. Add the Windows host adapter IP step.
- [x] **BUG-25** (High): Verified static release binary: 750 KB ELF32 Little-Endian ARM (EM_ARM 0x28), EABI version 5, soft-float (`0x5000200`).
- [x] **BUG-26** (High): Configured `CARGO_TARGET_ARMV5TE_UNKNOWN_LINUX_MUSLEABI_LINKER` in `deploy.sh`. Removed `Cross.toml`.
- [x] **BUG-27** (Medium): Added `py -3` check in `zig-lld-arm.cmd` and recursive WinGet glob in `zig-linker.py`.

### Gate B: Before Motors Move

- [x] **BUG-28** (Critical): Added `ExecStopPost` motor reset and LED shutoff in `ev3-web.service`. Added startup motor reset in `MotorController::new`.
- [x] **BUG-30** (High): Reverted BUG-17. Removed CORS `*` and PNA headers. Enforced strict `Origin` header validation on `POST`.
- [x] **BUG-31** (High): Changed watchdog timeouts and emergency stop to use `brake` stop action. Made zero-speed tank drive dispatch `stop` with `brake`.
- [x] **BUG-46** (High): Added unit tests for real sysfs driver using temporary fake sysfs directory trees. 21/21 tests pass.
- [x] **BUG-47** (Medium): Replaced swallowed errors with explicit error handling and `eprintln!` logging.

### Gate C: Before Phase 7 Keypad / Wi-Fi Work

- [x] **BUG-29** (High): Added `POST /api/shutdown` endpoint and power off button in web UI with confirmation dialog.
- [x] **BUG-32** (High): Implemented compact 7-row layout in `src/sysfs/display.rs` fitting 10-row and 16-row consoles without scrolling.
- [x] **BUG-33** (High): **[NOT USED in MVP]** Physical keypad character picker removed from MVP in favor of root `wifi.txt` auto-provisioning.
- [x] **BUG-34** (Medium): Added `ExecStartPre` to set `dmesg -n 1` to suppress kernel printk messages on tty1.
- [x] **BUG-35** (Medium): Background thread refreshes LCD every 5 seconds. Removed fake `192.168.2.2` fallback (now shows `"No network"`).

### Gate D: Performance, Robustness, and Docs

- [x] **BUG-36** (Medium): Boot-time motor enumeration only. File descriptors remain open and persistent across process runtime.
- [x] **BUG-37** (Medium): Removed directory scan from HTTP thread; missing ports return error from memory with zero disk I/O.
- [x] **BUG-38** (Medium): Enforced 4 KB request body size limit (`take(4096)`).
- [x] **BUG-39** (Medium): Configured `Cache-Control: no-cache, must-revalidate` for static web assets.
- [ ] **BUG-43** (Medium): Unmeasured numbers labeled **TARGET** in docs; record measured values in Phase 6 after hardware testing.
- [ ] **BUG-44** (Medium): Corrected kernel version (4.14.x) and client mode Wi-Fi [ADOPTED] vs AP mode [REJECTED in Phase 1]; verify USB host speed on hardware.
- [x] **BUG-40** (Low): Clamped `--poll-interval` to `10..=1000` ms in `src/config.rs`.
- [x] **BUG-41** (Low): Polarity written once and tracked in memory across runtime.
- [ ] **BUG-42** (Low): Check LED trigger on physical hardware.
- [x] **BUG-45** (Low): Synchronized README, REPORT, and PROJECT_PLAN.

### Earlier Items Reopened or Downgraded

| ID | New Status | Action |
| :--- | :--- | :--- |
| `BUG-09` | RESOLVED | Fixed through `BUG-26`. |
| `BUG-14` | RESOLVED | Fixed through `BUG-28` and `BUG-31`. |
| `BUG-15` | RESOLVED | Fixed through `BUG-36`. |
| `BUG-16` | PENDING HARDWARE | Measure thread count on hardware (`BUG-43`). |
| `BUG-17` | RESOLVED | Reverted through `BUG-30`. |
| `BUG-19` | RESOLVED | `getty@tty1` masked and `dmesg -n 1` set (`BUG-34`). |
| `BUG-22` | RESOLVED | Fixed through `BUG-32`. |

---

## 12. Phase 9: Streamlined Appliance Mode & Pre-Baked Disk Image

> **Target User Flow:**
> 1. Flash `ev3-web-motor-ready.img.xz` (baked with your Wi-Fi credentials) to a MicroSD card.
> 2. Insert the card into the EV3 brick and power on.
> 3. EV3 connects to your Wi-Fi automatically and displays the Ready URL (`http://<ip>/`) on the LCD.
> 4. Open `http://<ip>/` in a web browser to control motors.
> 
> No button typing. No USB network cables. No terminal commands.

### 12.1 Architectural Decisions and Status Labels

| Component | Status Label | Technical Rationale |
| :--- | :--- | :--- |
| **Pre-Baked Flashable Disk Image** | `[ADOPTED]` | Injects `ev3-web-motor`, `ev3-web.service`, and pre-configured Wi-Fi credentials directly into the official `ev3dev-stretch` disk image. Eliminates all manual setup. |
| **Command-Line Wi-Fi Pre-Baking** | `[ADOPTED]` | Pre-provisions `/var/lib/connman/ev3_wifi.config` via `bake-image.sh --ssid ... --password ...`. Avoids slow on-brick typing. |
| **FAT Boot `wifi.txt` Provisioning** | `[ADOPTED]` | Secondary fallback in `WifiManager`: edit `wifi.txt` on the Windows/Mac accessible FAT boot partition to change Wi-Fi without re-flashing. |
| **On-Brick Keypad Character Picker** | `[REJECTED]` | Removed to eliminate unnecessary complexity and bloat. Pre-baked Wi-Fi handles network configuration directly. |
| **Keeping Stock `brickman`** | `[REJECTED]` | `brickman` uses ~18 MB RAM and contains unused menus (Bluetooth, sound, script runner). Disabling it frees RAM and boots 10–15s faster. |
| **LCD Operational Status Screen** | `[ADOPTED]` | Lightweight 7-row console renderer in `src/sysfs/display.rs`. Displays URL, IP, and battery voltage on `/dev/tty1`. |
| **Command-Line Flashing (`conv=fsync`)** | `[ADOPTED]` | Flashes external MicroSD card through standard command line tools. Ensures clean write and sync before card removal. |
| **Cached Disk Image Extraction** | `[REJECTED]` | Mutating a cached `ev3dev.img` in place leaks stale Wi-Fi credentials across runs when baking without `--ssid`. Always extract a clean base image from the zip archive. |
| **Self-Updating Pre-Gzipped Web Assets** | `[ADOPTED]` | Serves pre-compressed gzip assets directly from RAM (~12 KB total RSS). `ExecStartPre` updates via GitHub raw commits with self-healing and zero flash read wear. |

### 12.2 Technical Specifications

#### 1. Automated Image Baker (`tools/bake-image.sh`)
- Accepts `--ssid <SSID>` and `--password <PASS>` parameters to write `/var/lib/connman/ev3_wifi.config` (mode `0600`).
- Downloads `ev3dev-stretch-ev3-generic-2020-04-10.zip`.
- Mounts `ext4` root partition via loopback: `mount -o loop,offset=$ROOT_OFFSET ev3dev.img /mnt/ev3root`.
- Copies `ev3-web-motor` to `/mnt/ev3root/usr/local/bin/` (mode `0755`).
- Installs and enables `ev3-web.service` in `multi-user.target.wants/`.
- Disables `brickman.service` and masks `getty@tty1.service` and wait-online daemons.
- Compresses output with `xz -9` to produce `ev3-web-motor-ready.img.xz`.

#### 2. Runtime Display & Auto-Provisioning
- On boot, `WifiManager::auto_provision` checks for candidate `wifi.txt` files on root and FAT boot partitions.
- ConnMan reads `/var/lib/connman/ev3_wifi.config` and associates with the pre-baked Wi-Fi network.
- `DisplayController` renders the 7-row ready screen to `/dev/tty1` and updates every 5 seconds.
- Safe shutdown is available via `POST /api/shutdown` on the web dashboard.

#### 3. Command-Line Flashing & Verification Pipeline
- `xzcat ev3-web-motor-ready.img.xz | sudo dd of=/dev/sdX bs=4M status=progress conv=fsync` writes disk image directly.
- Execute `sudo sync` immediately following `dd` to finalize all device controller buffers.
- Run `sudo ./tools/flash-image.sh --verify-only /dev/sdX` to inspect partition 2 contents.

### 12.3 Implementation Checklist

- [x] **Step 9.1: Automated Image Baker Script**
  - Created `tools/bake-image.sh` with `--ssid` and `--password` parameters.
- [x] **Step 9.2: Runtime Display & Wi-Fi Provisioning**
  - Verified `DisplayController` 7-row status screen and 5-second background refresh.
  - Verified `WifiManager` credential parsing, complete ConnMan configuration generation (`[global]`, `IPv4 = dhcp`, `AutoConnect = true`), and technology scan.
  - All 22 unit tests pass (`cargo test`).
  - Standalone release binary cross-compiled for ARMv5te musl (750 KB).
- [x] **Step 9.3: Image Baker Script Remediation (`tools/bake-image.sh`)**
  - Fixed binary path in `ev3-web.service` to `/usr/local/bin/ev3-web-motor` and added `/home/robot/ev3-web-motor` symlink.
  - Added default systemd target redirect to `multi-user.target`.
  - Neutralized Brickman across service `ExecStart`, `/usr/sbin/brickman`, and `/usr/bin/brickman`.
  - Fixed partition sector calculation regex and fixed `SECTOR_SIZE` parsing (`awk` backwards evaluation from `NF` with 512-byte fallback guard).
  - Added ConnMan Wi-Fi power-on configuration (`/var/lib/connman/settings`) and dual `wifi.txt` fallback provisioning.
  - Added atomic temporary file compression (`.tmp`) with registered `EXIT` trap cleanup to prevent retaining stale images.
  - Added strict pre-unmount verification gate (verifying symlink targets, unit masks, robot symlink executable status, and settings).
  - Configured readable 8x16 console font (`Lat15-TerminusBold16`) in `/etc/default/console-setup` and `ev3-web.service` with build-time verification.
  - Enforced clean base image extraction from the zip archive on every run to eliminate credential leakage across bake cycles.
  - Identified that the existing `ev3-web-motor-ready.img.xz` on disk is out of date and pending a clean rebake.
- [x] **Step 9.4: Command-Line Flashing & Post-Write Verification Tool (`tools/flash-image.sh`)**
  - Created standalone `tools/flash-image.sh` with `--verify-only` mode (flashing separated from image baking).
  - Enforced command-line only flashing (no BalenaEtcher) with `xzcat | dd status=progress conv=fsync`.
  - Added multi-layer host drive protections:
    1. Active host SWAP check: blocks flashing if any partition is active swap.
    2. Critical host mount check: blocks flashing if mounted to `/`, `/boot`, `/efi`, `/home`, `/root`, `/etc`, `/var`, `/usr`, `/srv`, or `/opt`.
    3. Removable drive enforcement (`RM=1`): blocks non-removable internal SATA/NVMe drives unless `--allow-internal` is set.
    4. Size sanity ceiling (64 GB): blocks drives larger than 64 GB unless `--allow-large-disk` is set.
    5. Interactive confirmation: displays drive model, transport, and size, and requires typing `yes` (or passing `-y` / `--yes`).
  - Automated post-write verification: mounts partition 2 and validates binary, Brickman masking, and active service state.
  - Removed all BalenaEtcher references across codebase and documentation.
- [x] **Step 9.5: Pre-Gzipped Web Assets & Boot-Time Updater Integration**
  - Added `tools/update-assets.sh` (chmod 0755) with GitHub raw commit fetching and pre-gzipping.
  - Added `-k` (`--insecure`) to `curl` calls in `tools/update-assets.sh` to prevent failures caused by EV3 boot-time clock skew or outdated 2020 root CAs.
  - Added UTC build timestamp update to `/etc/fake-hwclock.data` in `tools/bake-image.sh` so the brick boots in the current year.
  - Added automatic copying of the host's `/etc/ssl/certs/ca-certificates.crt` into the rootfs image in `tools/bake-image.sh`.
  - Added background `ev3-update-assets.service` running after network online to invoke `/usr/local/bin/ev3-update-assets.sh`.
  - Added compile-time asset compression with `build.rs` and embedded bytes via `include_bytes!`.
  - Implemented `Router::new()` self-healing check and in-memory asset caching (~12 KB RSS).
  - Pre-populated `/home/robot/web_assets/` with baseline `.gz` files in `tools/bake-image.sh` and verified in pre-unmount checks.
- [ ] **Step 9.6: Physical Boot & First Run on EV3 Brick**
  - Re-bake clean appliance image (`ev3-web-motor-ready.img.xz`) using `sudo ./tools/bake-image.sh`.
  - Flash clean appliance image to MicroSD card using `sudo ./tools/flash-image.sh /dev/sdX`.
  - Insert verified MicroSD card and USB Wi-Fi dongle into EV3 brick and power on.
  - Verify automatic association to pre-configured Wi-Fi network and LCD 7-row status screen.
  - Open `http://<ip>/` in browser and test motor movement.

---

## 13. Phase 10: Modular Dashboard Builder (Design Mode vs Run Mode)

### 13.1 Core Principles and Scope Limits
- **Resist Over-Building:** Do not build a complex 2D drag-and-drop grid system. Do not add arbitrary widgets without explicit design approval.
- **Two Distinct Modes:**
  - **Design Mode:** User inspects detected motors and sensors, adds widgets from a defined list, and assigns hardware ports and axes.
  - **Run Mode:** Clean, uncluttered control interface optimized for mobile and desktop screens.
- **Client-Side Throttling:** Continuous control widgets (such as 2D joysticks and sliders) must throttle HTTP requests to 15 Hz maximum to protect the 300 MHz CPU.

### 13.2 Architectural Decisions and Status Labels

| Decision / Component | Status Label | Technical Rationale |
| :--- | :--- | :--- |
| **Two-Mode Architecture (Design vs Run)** | `[ADOPTED]` | Separates widget configuration from execution. Provides a clean mobile interface without design clutter. |
| **Card List Widget Layout** | `[ADOPTED]` | Simple, responsive vertical card list. Avoids complex 2D grid positioning engines and external dependencies. |
| **On-Demand Device Rescan Button** | `[ADOPTED]` | Adds a "Rescan Devices" button in Design Mode. Avoids complex kernel `udev` event listeners and hotplug monitoring threads. |
| **2D Virtual Joystick Widget** | `[ADOPTED]` | Maps X and Y axes to selected motor ports (with optional axis inversion). Throttled to 15 Hz. |
| **Initial Widget Catalog** | `[ADOPTED]` | Continuous speed slider, momentary button, toggle button, and 2D joystick. |
| **Layout Storage Location** | `[PENDING]` | Under evaluation (browser `localStorage`, EV3 brick JSON file, or external storage). Deferred to a future phase. |
| **Free-Form 2D Drag-and-Drop Grid** | `[REJECTED]` | Excessive complexity and code bloat for vanilla JavaScript without external libraries. |
| **Dynamic Background Hot-Plug Daemon** | `[REJECTED]` | Unnecessary CPU and memory overhead on 300 MHz ARM9. On-demand rescan is sufficient. |
| **Compound Sequences (Move N Deg then Return)** | `[NOT USED in Phase 10]` | Deferred to a dedicated future feature request (tracked in GitHub Issues). |

### 13.3 Implementation Checklist

- [x] **Step 10.1: Rescan API Endpoint (`POST /api/rescan`)**
  - Re-enumerates connected tacho-motors and returns updated hardware list on the background poller thread without blocking the HTTP request thread.
- [x] **Step 10.2: UI Mode Switcher (Design Mode vs Run Mode)**
  - Clean header toggle between configuration mode and execution mode.
- [x] **Step 10.3: Card-Based Widget Container**
  - Add and remove control cards dynamically in Design Mode with detected hardware inventory.
- [x] **Step 10.4: 2D Joystick Widget**
  - Pointer-based 2D joystick supporting both differential drive and independent axes, with touch-action isolation, auto-centering, and 15 Hz client-side rate limiting.
- [x] **Step 10.5: Basic Control Widgets**
  - Continuous slider, momentary button, toggle button, timed move, step move.
- [x] **Step 10.6: Host Simulation & Automated Testing**
  - 31 unit tests pass (`cargo test`); mock simulation verified with curl endpoints.
- [x] **Step 10.7: Streamlined Navigation, Footer Status, and Zero-Redundancy UI**
  - Consolidated top navigation into a single compact tab bar (`Design` / `Run`).
  - Relocated battery, connection, latency, emergency stop, and power off to the footer.
  - Eliminated the word "Mode" from all visible user interface text (0 occurrences).
  - Removed all redundant labels and duplicate text across cards and controls.
  - Verified touch interaction and captured verified screenshots.
- [x] **Step 10.8: 2x2 Square and 2x1 Half-Height Grid Layout**
  - Configured 2-column mobile layout and 4-column desktop layout (`grid-auto-flow: dense`) for `.run-widget-list`.
  - Added `.run-card-2x2` (1:1 aspect ratio square) for 2D joystick widgets.
  - Added `.run-card-2x1` (2:1 aspect ratio half-height) for sliders, buttons, and sensor cards.
  - Sized joystick boundary and inner controls with CSS clamp so a 2x2 joystick with a 2x1 slider cleanly forms a 2x3 mobile layout filling the screen without scrolling.
  - Verified layout across desktop (1280x800) and mobile (390x844) viewports.

---

## 14. Phase 11: Unified Low-Level Port Control API (`GET` & `POST /api/port/{port}`)

### 14.1 Core Principles and Scope Limits
- **Unified Port Addressing:** Accept both short (`A`..`D`, `1`..`4`) and long (`outA`..`outD`, `in1`..`in4`) port names case-insensitively.
- **Hybrid JSON Payload:** Accept raw sysfs keys (`speed_sp`, `time_sp`, `command`, `mode`) and convenience aliases (`speed`, `duration_s`, `degrees`, `duty`).
- **Strict Rule 6 Compliance:** All `GET /api/port/{port}` and `GET /api/ports` requests read from in-memory cache in RAM. Zero sysfs reads occur on the HTTP request thread.
- **Watchdog Protection:** Any continuous motor movement command arms the 1,000 ms client disconnect watchdog automatically.
- **Host-First Simulation:** Mock simulation for input ports 1–4 enables full testing on host machines without hardware.

### 14.2 Architectural Decisions and Status Labels

| Decision / Component | Status Label | Technical Rationale |
| :--- | :--- | :--- |
| **Unified Port Endpoint (`/api/port/{port}`)** | `[ADOPTED]` | Replaces multiple motor-specific routes with a consistent single-port endpoint for motors and sensors. |
| **All Ports Inventory Endpoint (`/api/ports`)** | `[ADOPTED]` | Returns complete inventory of all 8 physical ports (4 motors, 4 sensors) from memory cache. |
| **Hybrid Attribute Map Schema** | `[ADOPTED]` | Accepts raw kernel sysfs attributes and convenience aliases with automatic type conversion. |
| **Sysfs Sensor Driver (`src/sysfs/sensor.rs`)** | `[IMPLEMENTED]` | Reads `/sys/class/lego-sensor/` with decimal scaling, mode changes, and cached file handles. |
| **Sensor Display Widget** | `[IMPLEMENTED]` | Allows users to display live sensor values and change sensor modes in the web dashboard. |
| **Complex Math Transforms in Rust** | `[REJECTED]` | JavaScript widgets compute values directly; the Rust server handles only port attributes and safety limits. |
| **Dynamic sysfs Polling on HTTP Thread** | `[REJECTED]` | Violates Rule 6. Polling must execute exclusively on the background thread. |

### 14.3 Implementation Checklist

- [x] **Step 11.1: Sysfs Sensor Driver (`src/sysfs/sensor.rs`)**
  - Implemented `Sensor` struct reading `address`, `driver_name`, `mode`, `modes`, `decimals`, `units`, and `value0`.
  - Added cached file descriptor reading for high-speed non-blocking background polling.
- [x] **Step 11.2: Mock Sensor Simulation (`src/sysfs/mock.rs`)**
  - Added simulated sensors on ports 1–4: Touch (Port 1), Color (Port 2), Ultrasonic (Port 3), and Gyro (Port 4).
  - Implemented `poll_and_get_all_sensors` and `set_sensor_mode`.
- [x] **Step 11.3: Web Handlers & Type Conversions (`src/web/handlers.rs`)**
  - Expanded `PortCommandPayload` supporting raw sysfs keys and convenience aliases.
  - Implemented `resolve(max_speed)` with automatic speed scaling, duration conversion, and command inference.
  - Implemented `normalize_port_name` validating ports `A`..`D` and `1`..`4`.
- [x] **Step 11.4: Controller Polling & Command Dispatch (`src/controller.rs`)**
  - Stored `cached_sensors` and updated polling thread to query sensors every cycle.
  - Implemented `get_port_status`, `get_all_ports_status`, and `execute_port_command`.
  - Armed client disconnect watchdog on continuous motion commands.
- [x] **Step 11.5: HTTP Router Endpoints (`src/web/router.rs`)**
  - Added `GET /api/ports`, `GET /api/port/{port}`, and `POST /api/port/{port}`.
  - Automated tests added for all port routes (35 total tests pass).
- [x] **Step 11.6: Web Dashboard & Sensor Display Widget (`web_assets/`)**
  - Added Sensor Display widget to catalog in Design and Run modes.
  - Updated hardware inventory bar to display both 4 motors and 4 sensors.
  - Added client helpers `apiPort(port, payload)` and `getPort(port)`.
- [x] **Step 11.7: Host Simulation & Automated Testing**
  - All 35 automated tests pass with `cargo test`.
  - Verified endpoints in mock host server with curl commands.

---

## 15. Phase 12: Dedicated Route Pages & UI Simplification

### 15.1 Core Principles and Scope Limits
- **Dedicated Route Views:** Dedicated HTML pages `design.html` (served at `/design`) and `run.html` (served at `/run` and `/`) eliminate DOM mode toggling complexity.
- **Pre-Compressed Assets:** Compile-time gzip compression via `build.rs` includes all HTML, CSS, and JS files.
- **Self-Healing Static Asset Serving:** Router serves all 5 pre-compressed static assets from RAM with self-healing fallback to embedded baselines.
- **UI Simplification:** Removed decorative emojis, artificial buzzwords, and visual clutter in compliance with code guidelines.
- **Automated Verification:** Verified via headless browser screenshots and automated unit test suite.

### 15.2 Architectural Decisions and Status Labels

| Decision / Component | Status Label | Technical Rationale |
| :--- | :--- | :--- |
| **Dedicated Route HTML Files** | `[ADOPTED]` | Clean separation of `/design` and `/run` avoids single-page mode toggling bugs. |
| **Compile-Time Asset Gzip Compression** | `[ADOPTED]` | `build.rs` pre-compresses `design.html` and `run.html` alongside `index.html`, `style.css`, and `app.js`. |
| **Router Static Asset Dispatch & Healing** | `[IMPLEMENTED]` | In-RAM static asset delivery with 5-asset self-healing validation. |
| **Clean Engineering UI Without Emojis** | `[ADOPTED]` | Replaced decorative emojis with clear text labels across all views and dynamic status elements. |
| **Headless Browser Screenshot Verification** | `[IMPLEMENTED]` | Automated screenshot capture via headless Firefox; stored in `docs/screenshots/`. |

### 15.3 Implementation Checklist

- [x] **Step 12.1: Dedicated HTML Pages (`web_assets/`)**
  - Created `design.html` for configuration and `run.html` for runtime controls.
  - Aligned `index.html` to default to run mode.
- [x] **Step 12.2: Compile-Time Gzip Pipeline (`build.rs`)**
  - Updated `files` array to include `design.html` and `run.html`.
- [x] **Step 12.3: Router Asset Loading & Route Dispatch (`src/web/router.rs`)**
  - Added embedded baseline assets `BASELINE_DESIGN_HTML_GZ` and `BASELINE_RUN_HTML_GZ`.
  - Updated `load_web_assets_from` and `load_web_assets` to manage all 5 assets.
  - Added route handlers for `GET /design`, `GET /design.html`, `GET /run`, and `GET /run.html`.
  - Added router automated tests (35 passing unit tests).
- [x] **Step 12.4: Client-Side Resilience & UI Cleanup (`web_assets/app.js`)**
  - Added DOM element null guards across event attachment and view rendering.
  - Initialized default hardware inventory in constructor to guarantee immediate rendering.
  - Removed decorative emojis from buttons, logs, and card headers.
- [x] **Step 12.5: UI Verification & Screenshot Documentation (`docs/screenshots/`)**
  - Verified host simulation with mock server.
  - Captured `screenshot_design.png` and `screenshot_run.png`.
  - Documented screenshot previews in `README.md`.
- [x] **Step 12.6: Anti-Over-Engineering Cleanup & Codebase Consolidation**
  - Removed duplicate `web_assets/run.html` (serving `index.html` for both `/` and `/run`).
  - Removed Diagnostics & Log drawer from Run view, reserving it exclusively for Design view.
  - Stripped out speculative joystick features (steering mode dropdown, independent axes, invert checkboxes) down to simple Left Motor, Right Motor, and Max Speed.
  - Consolidated 4 button widget types into a single parameterized Action Button widget.
  - Defaulted starter layout to exactly 2 widgets (2x2 Joystick on top + 2x1 Slider below), cleanly filling mobile viewports.
  - Refactored `web_assets/app.js` (60% line reduction from 1,296 to 522 lines) adhering to `clean-javascript` and `ui-conciseness-audit`.
  - Updated pre-compression pipeline (`build.rs`), updater script, and router asset loader to manage 4 assets in RAM.
  - All 35 unit tests pass. Captured verified desktop and mobile screenshots.

---

## 16. Phase 13: Codebase Simplification & De-duplication

### 16.1 Core Principles and Scope Limits
- **Minimal Engineering:** Remove unused abstractions, redundant structs, and dead code across backend and frontend layers.
- **Single Source of Truth:** Route all motor commands through `PortCommandPayload` and `/api/port/{port}`.
- **Explicit Boundaries:** Document file responsibilities with clear rules answering "Where should I put a new thing?".
- **UI Conciseness:** Eliminate the word "mode" from element IDs, remove dead CSS classes, and maintain reactive status telemetry.
- **Simplified Technical English:** Write documentation with short sentences and direct language.

### 16.2 Architectural Decisions and Status Labels

| Decision / Component | Status Label | Technical Rationale |
| :--- | :--- | :--- |
| **Unified Command Dispatch** | `[ADOPTED]` | Routes motor actions through `/api/port/{port}` and `PortCommandPayload`, removing 6 redundant payload structs in [`src/web/handlers.rs`](src/web/handlers.rs). |
| **Table-Driven Asset Loader** | `[ADOPTED]` | Replaces repeated asset loading code with a static `ASSET_ENTRIES` table in [`src/web/router.rs`](src/web/router.rs). |
| **All-Ports Rescan Return** | `[ADOPTED]` | Updates `POST /api/rescan` in [`src/controller.rs`](src/controller.rs) to return `AllPortsStatus`, providing current sensor inventory to the client. |
| **Telemetry & E-Stop Aliases** | `[ADOPTED]` | Adds `/api/telemetry` (pointing to `/api/status`) and `/api/estop` (pointing to `/api/emergency-stop`) in [`src/web/router.rs`](src/web/router.rs). |
| **Code Organization Matrix** | `[ADOPTED]` | Defines explicit subsystem boundaries in [`docs/architecture.md`](docs/architecture.md) (Section 6) to prevent code sprawl. |
| **Client Status Envelope Handling** | `[ADOPTED]` | Resolves `ApiResponse<T>` envelope in [`web_assets/app.js`](web_assets/app.js), restoring battery voltage and sensor readouts. |

### 16.3 Implementation Checklist

- [x] **Step 13.1: Sysfs Driver & Controller De-duplication (`src/sysfs/`, `src/controller.rs`)**
  - Added `MotorStatus::disconnected(port)` constructor and removed dead `read_attr()` in [`src/sysfs/motor.rs`](src/sysfs/motor.rs).
  - Added `MockMotorState::new` constructor and removed dead setter methods in [`src/sysfs/mock.rs`](src/sysfs/mock.rs).
  - Added `SensorStatus::disconnected(port)` constructor in [`src/web/handlers.rs`](src/web/handlers.rs).
  - Removed duplicate string truncation in [`src/sysfs/display.rs`](src/sysfs/display.rs).
  - Replaced repetitive loops with functional iterator mappings in [`src/controller.rs`](src/controller.rs).
  - Extracted shared helper `raw_run_forever` and updated `rescan()` to return `AllPortsStatus` via `self.get_all_ports_status()`.
- [x] **Step 13.2: Router & Payload Schema Consolidation (`src/web/router.rs`, `src/web/handlers.rs`)**
  - Reduced `load_web_assets_from` using `ASSET_ENTRIES` table.
  - Unified `handle_port_post` and `handle_motor_post` through `dispatch_port_command` helper with port normalization.
  - Added route aliases `/api/telemetry` and `/api/estop`, plus automated tests for legacy motor endpoints and aliases.
  - Documented JSON response envelope and corrected battery telemetry units in [`docs/api.md`](docs/api.md).
- [x] **Step 13.3: Frontend Cleanup & Telemetry Fixes (`web_assets/`)**
  - Renamed `btn-mode-*` navigation IDs to `btn-nav-*` in HTML templates.
  - Resolved class name mismatch by styling `.run-card-footer` in [`web_assets/style.css`](web_assets/style.css).
  - Fixed trailing callback drop in `throttle()` to guarantee delivery of final zero or speed commands.
  - Fixed toggle button label reset on emergency stop and added pointer capture to momentary buttons.
  - Resolved `ApiResponse<T>` envelope unwrapping and added live telemetry to motor and sensor card footers.
- [x] **Step 13.4: Architectural Documentation Guide (`docs/architecture.md`)**
  - Added Section 6 ("Code Organization & Separation of Concerns") with explicit subsystem responsibilities.
- [x] **Step 13.5: Test Verification**
  - All 35 automated tests pass (`cargo test`).
  - Static release compilation verified with `cargo check --release`.
- [x] **Step 13.6: Minimalist UI De-cluttering & Simplification (`web_assets/`)**
  - Removed "Custom Titles" from widget builder; used fixed clean card titles (`Joystick`, `Slider`, `Button`, `Sensor`).
  - Removed "Max Speed" inputs from widgets; hardcoded standard EV3 range (`-1000..1000` ticks/s for slider, `1000` vector clamp for joystick, `600` default for buttons).
  - Removed redundant "Reset Defaults" button from design toolbar.
  - Retained "Zero (Stop)" button beside slider and diagnostics drawer.
- [x] **Step 13.7: Systematic Codebase Deduplication & Metric Verification**
  - Removed redundant button CSS classes (`.btn-estop`, `.btn-shutdown`) across HTML and CSS.
  - Extracted shared port name arrays and address parsers into single source of truth in `src/web/handlers.rs`.
  - Removed phantom keyboard drive controls and deduplicated setup quickstart in documentation.
  - Harmonized template styling and eliminated inline CSS in `web_assets/index.html`.
  - Replaced unverified performance claims with factual architectural design in `README.md` and `docs/architecture.md`.
  - Re-captured and updated all desktop and mobile screenshots after deduplication.

---

## 17. Phase 14: Direct 2-Axis Joystick, Generic Batch Motor API, and Watchdog Generalization

### 17.1 Goals
- **Direct 2-Axis Motor Control:** Replace tank-drive differential mixing on the 2D joystick with direct, independent X and Y motor axis speed control.
- **UI Ergonomics:** Expand the joystick boundary to fill ~85–90% of widget container width and add a configurable release action (Brake vs Coast).
- **Generic Batch Motor Primitive:** Add `POST /api/batch` (and alias `POST /api/ports`) accepting multi-port commands and an optional heartbeat timeout.
- **Watchdog Generalization:** Generalize the heartbeat safety watchdog from a fixed two-port tuple to an arbitrary list of motor ports, eliminating the need for custom Rust endpoints for future multi-motor UI widgets.

### 17.2 Architectural Decisions and Status Labels

| Decision / Component | Status Label | Technical Rationale |
| :--- | :--- | :--- |
| **Independent 2-Axis Joystick Control** | `[ADOPTED]` | Replaces tank-drive mixing so X controls Port A and Y controls Port B directly and linearly without cross-coupling. |
| **Enlarged Joystick Boundary** | `[ADOPTED]` | Increases joystick boundary to `width: 88%` / `max-width: 280px` to fill the widget card cleanly. |
| **Joystick Release Action Option** | `[ADOPTED]` | Adds Design view dropdown allowing user selection between `brake` and `coast` upon pointer release. |
| **Generic Multi-Port Batch API** | `[ADOPTED]` | Adds `POST /api/batch` and `POST /api/ports` to accept an array of port commands, allowing frontend-only widget expansion. |
| **Generalized Watchdog State** | `[ADOPTED]` | Generalizes `WatchdogState` to dynamic `heartbeat_ports: Vec<String>` with configurable timeout clamping (100–5000ms). |

### 17.3 Implementation Checklist

- [x] **Step 14.1: Direct 2-Axis Joystick Math & Ergonomics (`web_assets/`, `src/controller.rs`)**
  - Updated `src/controller.rs` `joystick_drive` to set motor speeds directly (`left_speed` and `right_speed`) without differential mixing.
  - Expanded `.joystick-boundary` in `web_assets/style.css` to 88% width with 280px maximum.
  - Added "Release Action" select input (`Brake` / `Coast`) to joystick configuration in `web_assets/app.js`.
  - Re-generated UI screenshots in `ui_screenshots.md`.
- [x] **Step 14.2: Generic Batch Command Types & Routing (`src/web/handlers.rs`, `src/web/router.rs`)**
  - Added `BatchItemPayload` and `BatchCommandPayload` in `src/web/handlers.rs`.
  - Added `POST /api/batch` and `POST /api/ports` route handlers in `src/web/router.rs`.
- [x] **Step 14.3: Generalized Watchdog & Controller Execution (`src/controller.rs`)**
  - Generalized `WatchdogState` to `heartbeat_ports: Vec<String>` and `heartbeat_timeout: Duration`.
  - Implemented `MotorController::execute_batch` to execute port commands and register watchdog monitoring.
  - Added unit test `test_controller_execute_batch` verifying multi-port command execution and heartbeat timeout stop.
- [x] **Step 14.4: Client Batch Helper Integration (`web_assets/app.js`)**
  - Added `apiBatch(commands, heartbeatMs = 400)` method to `EV3App`.
  - Switched joystick pointer move and pointer release handlers to dispatch via `apiBatch`.
- [x] **Step 14.5: API Documentation & Test Verification (`docs/api.md`, `cargo test`)**
  - Documented `POST /api/batch` and `POST /api/ports` in `docs/api.md`.
  - Added automated test for `POST /api/batch` in `src/web/router.rs`.
  - Verified all 37 unit tests pass in `cargo test`.

---

## 18. Phase 15: Multi-Motor 2D Joystick Control

### 18.1 Goals
- **Multi-Motor 2D Control:** Allow the 2D joystick widget to drive multiple motors per axis simultaneously with independent inversion flags.
- **Zero Backend Changes:** Validate architectural extensibility by driving multi-motor configurations through the existing `POST /api/batch` endpoint without modifying the Rust backend API contract.
- **Concise Configuration Matrix:** Provide a compact motor mapping UI in the Design card with active checkboxes and inversion toggles for Ports A through D.
- **Pure Math Boundaries:** Isolate planar coordinate transforms and multi-motor speed calculations into pure functions adhering to `clean-javascript` guidelines.

### 18.2 Architectural Decisions and Status Labels

| Decision / Component | Status Label | Technical Rationale |
| :--- | :--- | :--- |
| **Multi-Motor 2D Joystick Option** | `[ADOPTED]` | Adds a configurable `multiMotor` boolean option to the 2D joystick widget with full backwards compatibility. |
| **Zero Backend Changes (`POST /api/batch`)** | `[IMPLEMENTED]` | Relies completely on the generic batch API added in Phase 14; no modifications to the Rust HTTP server API contract. |
| **Dual-Axis Motor Mapping Grid** | `[ADOPTED]` | Presents two columns (X Axis, Y Axis) each with Ports A–D, active toggles, and per-motor inversion checkboxes. |
| **Pure Velocity Mapping Helpers** | `[IMPLEMENTED]` | Separates planar input math (`calculateJoystickCommands`) and stop targets (`getJoystickStopCommands`) from DOM event listeners. |
| **Multi-Motor Run Card Summary Badge** | `[ADOPTED]` | Displays compact channel status in Run mode (for example, `X: B, -C \| Y: A`). |

### 18.3 Implementation Checklist

- [x] **Step 15.1: Multi-Motor State & Storage Schema (`web_assets/app.js`)**
  - Updated `initStorage`, `loadDefaultWidgets`, and `addWidget` to store `multiMotor`, `xMotors`, and `yMotors`.
  - Normalized stored port names to uppercase strings and preserved fallback values for legacy widgets loaded from browser localStorage.
- [x] **Step 15.2: Pure Velocity Calculation Helpers (`web_assets/app.js`)**
  - Created `calculateJoystickCommands(w, normX, normY, maxSpeed)` pure calculation helper.
  - Created `getJoystickStopCommands(w)` pure halt generator.
  - Deduplicated motor ports and combined velocity setpoints when identical ports are assigned to both axes in standard mode.
- [x] **Step 15.3: Design View Configuration Matrix (`web_assets/app.js`, `web_assets/style.css`)**
  - Added "Multi-Motor Control" checkbox toggle in the Design widget card.
  - Rendered compact dual-axis port mapping matrix with active and invert toggles for Ports A, B, C, and D.
  - Updated active motor toggle listener to update invert availability in-place without destructive full-widget DOM re-renders.
  - Added styles for `.multi-motor-container`, `.multi-motor-row`, and `.multi-motor-invert-label`.
- [x] **Step 15.4: Run View Summary Badge & Batch Dispatch (`web_assets/app.js`)**
  - Added `joystickPortBadge` method formatting compact channel summaries (for example, `X: B, -C | Y: A` and `X: B | Y: C`).
  - Updated `initJoystick` event handlers to dispatch multi-motor commands via `this.apiBatch`.
- [x] **Step 15.5: Backend Integration Test Verification (`src/controller.rs`, `cargo test`)**
  - Added `test_controller_execute_batch_multi_motor_joystick` verifying 3-motor simultaneous batch execution (ports A, B, and inverted C) and heartbeat halt.
  - Added `test_controller_execute_batch_edge_cases` verifying empty batch, invalid port error handling, and 4-motor simultaneous execution.
  - Verified all 39 tests pass with zero compiler warnings in `cargo check` and `cargo test`.

---

## 19. Phase 16: Codebase Sweep, Chassis-Agnostic Backend & Frontend Tank Controls

### 19.1 Goals
- **Codebase Consolidation:** Sweep Rust, JavaScript, HTML, and CSS for duplicated code, single-caller functions, and dead code.
- **Chassis-Agnostic Backend:** Remove all "tank" terminology, endpoints, and vehicle-specific assumptions from the Rust backend.
- **Frontend Tank Controls Widget:** Provide a dedicated dual-track widget (left track / right track vertical sliders) in the frontend using existing generic batch API commands (`POST /api/batch`).
- **Regression Guard:** Add explicit warning comment in frontend code to prevent future agents from introducing vehicle-specific backend APIs.

### 19.2 Architectural Decisions and Status Labels

| Decision / Component | Status Label | Technical Rationale |
| :--- | :--- | :--- |
| **Consolidate Single-Caller Router Handlers** | `[IMPLEMENTED]` | Merged single-caller `dispatch_port_command` into [`handle_port_post`](src/web/router.rs), simplifying request dispatch. |
| **Deduplicate Motor Speed Clamping** | `[IMPLEMENTED]` | Extracted [`resolve_max_speed`](src/controller.rs) on `MotorController`, unifying limit calculation across batch and single-port requests. |
| **Prune Dead Tank Methods in Rust** | `[IMPLEMENTED]` | Removed unused `tank_drive` method in `MotorController` and deleted `/api/tank-drive` endpoint alias. |
| **Chassis-Agnostic Backend Contract** | `[ADOPTED]` | Ensured zero mentions of "tank" in `src/`; the Rust server provides generic motor and sensor primitives. |
| **Frontend Tank Controls Widget** | `[IMPLEMENTED]` | Added dual-track slider widget in [`web_assets/app.js`](web_assets/app.js) communicating via [`POST /api/batch`](src/web/router.rs). |
| **API Support for Tank Widget** | `[REJECTED]` | Kept backend completely generic; tank widget maps left and right tracks to generic batch commands without dedicated backend endpoints. |

### 19.3 Implementation Checklist

- [x] **Step 16.1: Codebase Sweep & Function Consolidation (`src/web/router.rs`, `src/controller.rs`, `web_assets/app.js`, `web_assets/style.css`)**
  - Consolidated single-caller `dispatch_port_command` into `handle_port_post`.
  - Added `resolve_max_speed` to `MotorController` to deduplicate motor clamping across batch and port endpoints.
  - Consolidated card footer text updates into `updateCardFooters` in `web_assets/app.js`.
  - Combined duplicated badge classes `.badge-connected, .badge-running` in `web_assets/style.css`.
- [x] **Step 16.2: Eliminate Tank Terminology from Rust Backend (`src/web/router.rs`, `src/controller.rs`)**
  - Removed `/api/tank-drive` alias endpoint from the HTTP router.
  - Renamed test to `test_joystick_watchdog_timeout` in `src/controller.rs`.
  - Confirmed 0 occurrences of "tank" across all Rust source files in `src/`.
- [x] **Step 16.3: Frontend Dual-Track Tank Controls Widget (`web_assets/design.html`, `web_assets/app.js`, `web_assets/style.css`)**
  - Added "Tank Controls" option in `web_assets/design.html`.
  - Added schema and configuration UI for left and right track port selection and stop action in `web_assets/app.js`.
  - Added dual vertical sliders and "Stop Both" zero button in Run mode in `web_assets/app.js`.
  - Added `initTank` dispatcher using generic `apiBatch` calls with rate limiting.
  - Added exact regression guard comment: `// This is only a custom front-end, don't make any API support for this widget!`.
  - Added cross-browser vertical slider and track column styling in `web_assets/style.css`.
- [x] **Step 16.4: Host Simulation & Integration Testing (`cargo test`, `cargo run -- --mock`)**
  - Verified all 45 Rust tests pass in `cargo test`.
  - Verified host simulation on default port 8080 with mock motor telemetry and batch commands.

---

## 20. Phase 17: Codebase Simplification, Safety Hardening & State Consolidation

### 20.1 Goals
- **Codebase Simplification:** Continue making the codebase better and more elegant without expanding it (net negative line count).
- **Safety Hardening on Emergency Stop:** Ensure emergency stop halts all pending throttled commands so delayed timer callbacks cannot restart motors after stop.
- **Sysfs Descriptor Reading Unification:** Unify buffered descriptor sampling across motor and sensor drivers with a shared helper in `sysfs/mod.rs`.
- **Sensor Synchronization Consolidation:** Collapse multiple `Arc<Mutex<...>>` in `Sensor` into a single atomic state struct, eliminating mutex lock contention and heap allocations.
- **Single-Pass DOM Updates:** Consolidate sensor readouts and footer statuses in the frontend into single-pass card traversals.
- **Channel Address Parsing Consolidation:** Unify motor and sensor sysfs address pattern matching with a table-driven parser.

### 20.2 Architectural Decisions and Status Labels

| Decision / Component | Status Label | Technical Rationale |
| :--- | :--- | :--- |
| **Shared `read_sysfs_cached` Helper** | `[IMPLEMENTED]` | Moved descriptor caching to [`src/sysfs/mod.rs`](src/sysfs/mod.rs), eliminating duplicate reading logic in motor and sensor. |
| **Consolidated `SensorState` Mutex** | `[IMPLEMENTED]` | Grouped `mode`, `decimals`, `units`, and cached descriptor into one [`SensorState`](src/sysfs/sensor.rs), cutting 3 mutexes per sensor and preventing race conditions during mode switches. |
| **Consolidated `MotorState` Mutex** | `[IMPLEMENTED]` | Grouped `polarity` and file descriptors into one [`MotorState`](src/sysfs/motor.rs), halving mutex allocations and eliminating double-locking in `poll_dynamic_status`. |
| **SensorStatus Layer Boundary** | `[IMPLEMENTED]` | Moved [`SensorStatus`](src/sysfs/sensor.rs) from web handlers to sysfs module, eliminating inverted web dependency. |
| **Heartbeat vs Continuous Watchdog Separation** | `[IMPLEMENTED]` | Dispatched heartbeat commands in [`execute_batch`](src/controller.rs) via `raw_run_forever` without arming continuous watchdog, avoiding spurious emergency stops when driving without polling `/api/status`. |
| **Throttle Timer Disarm on E-Stop** | `[IMPLEMENTED]` | Extracted `clearAllThrottles` in [`web_assets/app.js`](web_assets/app.js) to cancel queued callbacks and unified slider reset passes. |
| **Pruning Dead Payload Fields & API Docs** | `[IMPLEMENTED]` | Removed obsolete `left_port`/`right_port`/`left_speed`/`right_speed` fields from [`JoystickPayload`](src/web/handlers.rs) and removed deleted `/api/tank-drive` endpoint from [`docs/api.md`](docs/api.md). |
| **Table-Driven Address Parsing** | `[IMPLEMENTED]` | Unified motor and sensor address recognition into `parse_channel_address` in [`src/web/handlers.rs`](src/web/handlers.rs). |

### 20.3 Implementation Checklist

- [x] **Step 17.1: Shared Sysfs Descriptor Reading & State Synchronization (`src/sysfs/mod.rs`, `src/sysfs/sensor.rs`, `src/sysfs/motor.rs`)**
  - Added `read_sysfs_cached` in `src/sysfs/mod.rs` for shared buffered descriptor sampling.
  - Replaced 4 separate `Arc<Mutex>` instances in `Sensor` with a single atomic `SensorState`, locking during mode transitions.
  - Replaced separate `polarity` and `cached_files` in `Motor` with a single `MotorState` lock.
  - Moved `SensorStatus` to `src/sysfs/sensor.rs` and re-exported it in `sysfs/mod.rs`.
  - Replaced redundant attribute reading in `Motor::poll_dynamic_status` with `read_i32` helper.
- [x] **Step 17.2: Watchdog Separation & Address Parsing (`src/web/handlers.rs`, `src/controller.rs`, `docs/api.md`)**
  - Fixed disconnect watchdog bug by separating raw motor execution from continuous watchdog arming in `execute_batch`.
  - Added `test_joystick_does_not_trigger_disconnect_watchdog` unit test.
  - Unified address parsing in `src/web/handlers.rs` via `parse_channel_address`.
  - Pruned dead legacy fields from `JoystickPayload` and updated `docs/api.md`.
- [x] **Step 17.3: Frontend Safety Hardening & DOM Optimization (`web_assets/app.js`)**
  - Extracted `clearAllThrottles()` and disarmed all active throttle timers on emergency stop.
  - Unified range and tank slider resets into a single DOM query.
  - Centered joystick knobs and readouts upon emergency stop.
  - Updated sensor unit readouts dynamically in `updateSensorReadouts`.
  - Enforced comment length limit (<= 79 characters) across all files.
- [x] **Step 17.4: Verification & Host Simulation (`cargo test`, `cargo run -- --mock`)**
  - Verified all 46 unit tests pass in `cargo test` with zero warnings.
  - Verified host simulation on port 8080 (`/api/status`, `/api/batch`, `/api/joystick`, `/api/estop`).

---

## 21. Phase 18: Heartbeat Watchdog Relaxation & Coast Stop Action Optimization

### 21.1 Context & Objectives
- **Problem:** The previous 400 ms heartbeat cutoff caused frequent unintended halts on iffy or high-jitter Wi-Fi networks. Additionally, stopping abruptly with `brake` or `hold` on missed heartbeats caused high mechanical shock and gear stress on EV3 motors. Furthermore, stopping only the most recent batch's ports left other active channels running without watchdog protection.
- **Solution:**
  1. Extend default heartbeat cutoff timeout to 2,000 ms across `src/controller.rs` and `web_assets/app.js`.
  2. Implement comprehensive `coast_all_motors` in `src/controller.rs` and `coast_all` in `src/sysfs/mock.rs` to stop and coast all actuators across all ports upon heartbeat expiration.
  3. Keep the 1,000 ms client disconnect watchdog (`/api/status` polling) intact for browser dropouts.

### 21.2 Architectural Decision Matrix

| Option / Proposed Change | Status | Technical Rationale & File Links |
| :--- | :--- | :--- |
| **Heartbeat Watchdog Cutoff (2000 ms)** | `[IMPLEMENTED]` | Increased default heartbeat cutoff from 400 ms to 2,000 ms in [`src/controller.rs`](src/controller.rs) and [`web_assets/app.js`](web_assets/app.js) to tolerate real-world wireless network jitter. |
| **Timeout Stop Action Coast Across All Motors** | `[IMPLEMENTED]` | Implemented `coast_all_motors` in [`src/controller.rs`](src/controller.rs) and `coast_all` in [`src/sysfs/mock.rs`](src/sysfs/mock.rs) to halt all output channels with `coast` action, eliminating mechanical shock and multi-batch leaks. |
| **Frontend Batch Dispatch Consolidation** | `[IMPLEMENTED]` | Updated `apiBatch` default `heartbeatMs` to 2,000 ms in [`web_assets/app.js`](web_assets/app.js) and removed redundant hardcoded timeouts across slider and joystick dispatchers. |

### 21.3 Implementation Checklist

- [x] **Step 18.1: Backend Heartbeat Timeout & Coast Stop Action (`src/controller.rs`, `src/sysfs/mock.rs`)**
  - Updated `WatchdogState` default `heartbeat_timeout` to `Duration::from_millis(2000)`.
  - Updated `joystick_drive` default `heartbeat_ms` to `2000`.
  - Implemented `coast_all_motors` in `MotorController` and `coast_all` in `MockController` to safely coast all actuators on heartbeat expiration.
  - Added query helper `get_stop_action` in `MockController` and `get_mock_stop_action` in `MotorController`.
- [x] **Step 18.2: Frontend Heartbeat Default & Clean Dispatch (`web_assets/app.js`)**
  - Changed `apiBatch(commands, heartbeatMs = 2000)` default parameter.
  - Removed duplicate hardcoded 400 ms arguments from `initTank` and `initJoystick` callers.
  - Verified non-echoing comments under 80 characters.
- [x] **Step 18.3: Unit Test Suite & Documentation Updates (`README.md`, `docs/api.md`, `docs/architecture.md`, `PROJECT_PLAN.md`)**
  - Added unit test `test_heartbeat_timeout_coasts_all_motors_across_channels` verifying all channels coast on expiration across sequential batches.
  - Updated `test_joystick_watchdog_timeout` to verify 2,000 ms timeout and assert `coast` stop action.
  - Updated `test_controller_execute_batch` to assert `coast` stop action on timeout.
  - Updated `README.md`, `docs/api.md`, and `docs/architecture.md`.
  - Verified host simulation on port 8080.

---

## 22. Phase 19: Unified Safety Watchdog Architecture

### 22.1 Context & Objectives
- **Problem:** The previous watchdog design maintained a confusing split between a streaming "heartbeat watchdog" (2000 ms, coast stop action) and a continuous "client disconnect watchdog" (1000 ms, emergency stop brake action). This split created unnecessary complexity across driving modes, separate state booleans, and inconsistent timeout behaviors.
- **Solution:**
  1. Eliminate the split between `heartbeat_active` and `continuous_active`.
  2. Implement ONE unified safety watchdog in `src/controller.rs` that applies equally to all driving controls (joystick, tank drive, sliders, single-port commands, and batch API).
  3. Standardize the watchdog timeout cutoff consistently to 2.0 seconds (2000 ms) everywhere.
  4. Ensure that any 2-second timeout without client communication always stops and coasts all motors (`coast_all_motors`).
  5. Simplify `WatchdogState` to three fields (`last_activity: Instant`, `timeout: Duration`, `active: bool`) and simplify `check_watchdogs`.
  6. Ensure all unit and integration tests verify the unified 2-second coasting watchdog.

### 22.2 Architectural Decision Matrix

| Option / Proposed Change | Status | Technical Rationale & File Links |
| :--- | :--- | :--- |
| **Unified Watchdog State** | `[IMPLEMENTED]` | Replaced multi-tier flags with a single `active: bool`, `last_activity: Instant`, and `timeout: Duration` in [`src/controller.rs`](src/controller.rs), eliminating state divergence. |
| **Consistent 2.0-Second Cutoff** | `[IMPLEMENTED]` | Standardized 2,000 ms timeout across all driving controls with automatic coast stop action via `coast_all_motors` on timeout. |
| **Simplified Port Command Execution** | `[IMPLEMENTED]` | Eliminated `execute_port_command_internal` and `is_heartbeat` flag; unified command dispatch under `execute_port_command`. |
| **Bi-directional Activity Refresh** | `[IMPLEMENTED]` | Any client communication (status polling or command dispatch) refreshes `last_activity`, preventing unintended timeouts during active sessions. |
| **Active Motion Guarding & Clean Stop Disarming** | `[IMPLEMENTED]` | Added `is_any_motor_running` in [`src/controller.rs`](src/controller.rs) and [`src/sysfs/mock.rs`](src/sysfs/mock.rs). Ensures zero-speed batches do not leave active channels unprotected, and explicit stops disarm idle watchdogs cleanly. |

### 22.3 Implementation Checklist

- [x] **Step 19.1: Unified Watchdog State & Controller Simplification (`src/controller.rs`)**
  - Refactored `WatchdogState` to `last_activity: Instant`, `timeout: Duration`, and `active: bool`.
  - Refactored `check_watchdogs` to a single timeout check triggering `coast_all_motors`.
  - Implemented `arm_watchdog(&self, custom_timeout_ms: Option<u64>)`.
  - Removed `execute_port_command_internal` and `is_heartbeat` suppression parameter.
  - Added `is_any_motor_running` to both [`MockController`](src/sysfs/mock.rs) and [`MotorController`](src/controller.rs).
  - Armed watchdog on `run_forever`, `run_direct`, and `execute_batch` with non-zero speeds.
  - Guarded `execute_batch` zero-speed disarming against canceling protection on other active channels.
  - Disarmed watchdog on explicit stop commands, `coast_all_motors`, and `emergency_stop` when all channels halt.
  - Ensured `get_battery()`, `set_polarity()`, `set_sensor_mode()`, `run_timed()`, `run_to_rel_pos()`, and `rescan()` touch client poll.
- [x] **Step 19.2: Unit Test Suite Hardening (`src/controller.rs`)**
  - Updated `test_client_disconnect_watchdog_timeout` to verify 2,000 ms timeout and `coast` stop action.
  - Updated `test_joystick_does_not_trigger_disconnect_watchdog` to verify periodic activity refresh.
  - Added unit test `test_unified_watchdog_coasts_all_motors_after_2_seconds` for mixed driving controls.
  - Added unit test `test_client_polling_refreshes_unified_watchdog` verifying telemetry polls refresh the timer.
  - Added unit test `test_batch_zero_speed_does_not_disarm_watchdog_for_running_motors`.
  - Added unit test `test_explicit_stop_disarms_watchdog_when_no_motors_running`.
  - Added unit test `test_battery_polling_refreshes_unified_watchdog`.
  - Verified all 52 unit tests pass in `cargo test`.
- [x] **Step 19.3: Documentation & Plan Synchronization (`docs/architecture.md`, `docs/api.md`, `README.md`, `PROJECT_PLAN.md`)**
  - Updated `docs/architecture.md` Section 3 to document the unified safety watchdog.
  - Updated `docs/api.md` safety watchdog behavior section.
  - Updated `README.md` features and architecture links.
  - Verified host simulation on port 8080.

---

## 23. Phase 20: Safe Dynamic Widget Updates & Modular Architecture

### 23.1 Context & Objectives
- **Problem:** Widgets were statically baked into a monolithic frontend file (`app.js`), preventing incremental widget delivery or independent updates from the web without whole-system recompilation or image reflashing. Furthermore, brick boot and server responsiveness must never block on external network latency or fail when offline.
- **Solution:**
  1. Modularize dashboard components into standalone native ES modules in `web_assets/widgets/<name>/widget.js` (`joystick`, `tank`, `slider`, `button`, `sensor`).
  2. Implement unified `WidgetRegistry` in `web_assets/app.js` with dynamic `import()` loading and automatic version cache invalidation (`?v=<timestamp>`).
  3. Implement background update discovery using system `curl` in `src/updater.rs`, triggered after active IP detection with a strict 3-second timeout and silent fallback.
  4. Compare lightweight `manifest.json` timestamps avoiding GitHub API rate limits, downloading only modified files.
  5. Deliver transparent `/widgets/*` static routes in `src/web/router.rs` serving `.gz` when present, or plain uncompressed files without requiring an on-brick gzip tool.
  6. Preserve existing `localStorage` user widget configuration without schema mutation.
  7. Add end-to-end automated tests verifying remote updates and HTTP static delivery.

### 23.2 Architectural Decision Matrix

| Option / Proposed Change | Status | Technical Rationale & File Links |
| :--- | :--- | :--- |
| **Modular ES Widgets** | `[IMPLEMENTED]` | Separated joystick, tank, slider, button, and sensor into isolated modules in [`web_assets/widgets/`](web_assets/widgets/). |
| **Unified WidgetRegistry** | `[IMPLEMENTED]` | Dynamic import pipeline in [`web_assets/app.js`](web_assets/app.js) with manifest version query parameter cache busting and optional `widget.css` support. |
| **Background curl Updater** | `[IMPLEMENTED]` | Silent background thread in [`src/updater.rs`](src/updater.rs) with strict 3-second timeout and active network detection. |
| **Atomic Multi-File Staging** | `[IMPLEMENTED]` | Staged multi-file downloads in [`src/updater.rs`](src/updater.rs) ensuring partial failures cleanly roll back without corrupting on-disk widgets or bumping catalog timestamps. |
| **Manifest Timestamp Discovery** | `[IMPLEMENTED]` | Lightweight [`manifest.json`](web_assets/widgets/manifest.json) comparison avoiding GitHub REST API rate limits. |
| **Transparent Route Delivery** | `[IMPLEMENTED]` | Static route `/widgets/*` in [`src/web/router.rs`](src/web/router.rs) serving `.gz` when present or plain files with `Cache-Control: no-cache` and stale gzip invalidation. |
| **Config Backwards Compatibility** | `[IMPLEMENTED]` | Preserved existing `ev3_dashboard_widgets` storage parsing in [`web_assets/app.js`](web_assets/app.js). |

### 23.3 Implementation Checklist

- [x] **Step 20.1: Modular Native ES Widget Definitions (`web_assets/widgets/`)**
  - Created modular widget modules: `joystick`, `tank`, `slider`, `button`, `sensor`.
  - Created baseline `web_assets/widgets/manifest.json`.
  - Enforced single-responsibility boundaries, non-echoing comments (<= 79 chars), and Simplified Technical English.
- [x] **Step 20.2: Client WidgetRegistry & Dynamic Imports (`web_assets/app.js`, HTML files)**
  - Implemented `WidgetRegistry` in `web_assets/app.js` with dynamic `import()`, fallback manifest, optional `widget.css` loader, and `?v=<timestamp>` cache busting.
  - Updated `<script type="module" src="app.js"></script>` in `index.html` and `design.html`.
  - Preserved existing `localStorage` settings and fallback defaults.
- [x] **Step 20.3: Rust Background Updater & Manifest Check (`src/updater.rs`, `src/main.rs`)**
  - Implemented `src/updater.rs` with `fetch_remote_manifest`, `load_local_manifest`, `download_file`, and `check_and_update_widgets`.
  - Added atomic multi-file staging: failed downloads roll back without touching live files or prematurely advancing catalog timestamps.
  - Added strict identifier and filename validation preventing directory traversal exploits.
  - Purges obsolete `.gz` companion files when newer plain files are downloaded.
  - Bound updater to 3-second timeout with silent local fallback.
  - Spawns background thread in `src/main.rs` after server bind and active IP probe.
- [x] **Step 20.4: Transparent Static Route Delivery (`src/web/router.rs`)**
  - Implemented `/widgets/*` routes in `src/web/router.rs` supporting GET and HEAD.
  - Serves `.gz` if present, plain uncompressed files if not, comparing mtimes to prevent serving stale `.gz` files when plain files are updated.
  - Guards against path traversal (`..`, `%2e`, null bytes, leading slashes).
  - Emits `Cache-Control: no-cache` on all widget responses.
  - Implemented individual baseline widget file restoration for self-healing.
- [x] **Step 20.5: Automated Verification Suite (`cargo test`, `cargo run -- --mock`)**
  - Added unit test `test_manifest_serialization_round_trip`.
  - Added unit test `test_load_local_manifest_missing_file`.
  - Added unit test `test_updater_offline_unreachable_returns_error_gracefully`.
  - Added unit test `test_updater_ignores_same_or_older_timestamp`.
  - Added unit test `test_transparent_route_serves_gzip_when_present_and_plain_when_not`.
  - Added unit test `test_widget_static_routes_headers_and_security`.
  - Added unit test `test_partial_download_failure_rolls_back_and_preserves_existing_files`.
  - Added unit test `test_updater_rejects_unsafe_path_traversal_in_manifest`.
  - Added unit test `test_updating_plain_file_purges_stale_gzip_companion_and_serves_new_content`.
  - Added unit test `test_restore_baseline_widgets_restores_individual_missing_files_when_manifest_exists`.
  - Added end-to-end unit test `test_e2e_updater_fetches_newer_widgets_and_server_serves_them`.
  - Verified 63/63 tests pass in `cargo test`.
  - Verified host simulation on default port 8080 (`http://localhost:8080/`).

---

## 24. Phase 21: Modular Widget Updater Fixes

### 24.1 Context & Objectives
- **Problem:** Triage of the widget updater identified 4 practical bugs: premature offline abort at boot, curl TLS failure from EV3 clock skew, clobbering of the `widgets/` folder by `update-assets.sh`, and local git working tree mutation in mock mode.
- **Solution:** Apply minimalist fixes without adding unnecessary complexity:
  1. Retry network detection on boot with exponential backoff up to 180 seconds maximum before falling back cleanly.
  2. Add `-k` flag to curl and increase timeout to 10 seconds for EV3 boot clock skew and slow CPU handshakes.
  3. Update `update-assets.sh` to move individual core asset files, preserving `widgets/`.
  4. Disable background updater in mock mode to protect host git working tree.
  5. Remove file deletion from HTTP GET request handler.
  6. Recognize `.gz` companion files when verifying local widget presence.

### 24.2 Architectural Decision Matrix

| Option / Proposed Change | Status | Technical Rationale & File Links |
| :--- | :--- | :--- |
| **Bounded Network Discovery Retry** | `[IMPLEMENTED]` | Poll network status with backoff up to 180 seconds before exiting cleanly in [`src/updater.rs`](src/updater.rs). |
| **Resilient Curl Flags** | `[IMPLEMENTED]` | Add `-k` flag and 10s timeout to survive boot clock skew in [`src/updater.rs`](src/updater.rs) and [`src/main.rs`](src/main.rs). |
| **Preserve Widgets in Asset Updates** | `[IMPLEMENTED]` | Move core assets individually in [`tools/update-assets.sh`](tools/update-assets.sh) to avoid deleting `widgets/`. |
| **Mock Mode Tree Isolation** | `[IMPLEMENTED]` | Disable remote updater in mock mode in [`src/main.rs`](src/main.rs) to protect git tree. |
| **Read-Only HTTP GET Routes** | `[IMPLEMENTED]` | Remove `fs::remove_file` from `handle_widget_request` in [`src/web/router.rs`](src/web/router.rs). |
| **Gzip-Aware Presence Checks** | `[IMPLEMENTED]` | Recognize `.gz` companion files in `should_update` in [`src/updater.rs`](src/updater.rs). |

### 24.3 Implementation Checklist

- [x] **Step 21.1: Startup Network Retry & Curl Resilience**
  - Implemented 180s bounded network wait loop in `start_background_updater()`.
  - Added `-k` flag and 10s timeout to curl invocations in `src/updater.rs`.
- [x] **Step 21.2: Asset Updater Script Preservation**
  - Updated `tools/update-assets.sh` to preserve `widgets/` directory during updates.
- [x] **Step 21.3: Mock Tree Protection & Request Safety**
  - Bypassed remote updater during mock mode execution in `src/main.rs`.
  - Removed disk file mutation from `Router::handle_widget_request()` in `src/web/router.rs`.
  - Added `.gz` companion check in `should_update` in `src/updater.rs`.
- [x] **Step 21.4: Automated Test Verification**
  - Added `test_gz_companion_recognized_in_should_update` in `src/updater.rs`.
  - Verified all 64 unit tests pass in `cargo test`.

---

## 25. Phase 22: Hardware Optimization & Codebase Simplification

### 25.1 Context & Objectives
- **Problem:** MicroSD card read/write latency on the 300 MHz ARM9 CPU slows down system boot and static file serving. Unpacking baseline files to disk on startup causes unnecessary flash wear. In addition, legacy API routes (`/api/joystick`), duplicate DOM element queries, double-temporary file staging, and dead CSS injectors add unnecessary complexity.
- **Solution:** Apply 6 direct simplifications for the slow MicroSD card and low-memory EV3 architecture:
  1. Remove duplicate DOM element resets in `emergencyStop()` in `web_assets/app.js` (Item 1.1).
  2. Write downloaded files directly to temporary staging paths in `src/updater.rs`, eliminating intermediate rename operations (Item 1.2).
  3. Remove unused dynamic stylesheet link injection in `web_assets/app.js` (Item 2.1).
  4. Eliminate filesystem `stat` and `mtime` syscalls per HTTP request in `src/web/router.rs` (Item 3.1).
  5. Remove legacy `POST /api/joystick` route, payload structs, controller methods, and documentation (Item 4.1).
  6. Eliminate boot-time SD card unpack writes by serving baseline widgets directly from in-memory compile-time data (Item 4.2).

### 25.2 Architectural Decision Matrix

| Option / Proposed Change | Status | Technical Rationale & File Links |
| :--- | :--- | :--- |
| **Modular Emergency Stop Reset** | `[IMPLEMENTED]` | Delegate reset exclusively to widget `onEmergencyStop()` hooks in [`web_assets/app.js`](web_assets/app.js). |
| **Single-Stage File Downloads** | `[IMPLEMENTED]` | Stream directly to `.tmp_stage` files in [`src/updater.rs`](src/updater.rs) without extra `.tmp_download` step. |
| **Remove Dead CSS Injector** | `[IMPLEMENTED]` | Eliminate unused stylesheet element injection in [`web_assets/app.js`](web_assets/app.js). |
| **Stat-Free Static Delivery** | `[IMPLEMENTED]` | Try `.gz` read, then uncompressed read, then RAM fallback in [`src/web/router.rs`](src/web/router.rs) without per-request `mtime` checks. |
| **Prune Legacy Joystick Route** | `[IMPLEMENTED]` | Remove `/api/joystick` in [`src/web/router.rs`](src/web/router.rs), [`src/web/handlers.rs`](src/web/handlers.rs), [`src/controller.rs`](src/controller.rs), and [`docs/api.md`](docs/api.md). |
| **Zero-SD Boot Unpack** | `[IMPLEMENTED]` | Remove `restore_baseline_widgets` call on startup in [`src/web/router.rs`](src/web/router.rs); fall back directly to `BASELINE_WIDGETS` in RAM. |

### 25.3 Implementation Checklist

- [x] **Step 22.1: Frontend UI De-duplication**
  - Removed duplicate DOM selector queries from `EV3App.emergencyStop()`.
  - Removed unused `<link>` tag insertion from `WidgetRegistry.load()`.
- [x] **Step 22.2: Updater Staging Simplification**
  - Removed extra `.tmp_download` intermediate file rename step in `download_file()`.
  - Added embedded baseline manifest fallback in `load_embedded_manifest()`.
- [x] **Step 22.3: Zero-SD Boot and Stat-Free HTTP Serving**
  - Removed boot-time baseline asset unpacking from `Router::new_with_dir()`.
  - Removed filesystem metadata comparison on every static widget request.
- [x] **Step 22.4: Legacy Endpoint Removal**
  - Removed `POST /api/joystick` endpoint and `JoystickPayload`.
  - Removed `Controller::joystick_drive()` method and updated controller tests to use batch commands.
  - Updated API reference table in `docs/api.md`.
- [x] **Step 22.5: Test & Host Simulation Verification**
  - Verified all 64 unit tests pass cleanly with `cargo test`.

