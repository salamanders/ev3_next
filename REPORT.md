# Technical Feasibility and Architectural Blueprint for a Web-Based Motor Control Distribution on LEGO Mindstorms EV3 Hardware

> **Implementation Tracking:** This report contains annotations showing which architectural recommendations were **[ADOPTED]**, **[IMPLEMENTED]**, or **[REJECTED / NOT USED]** in the codebase.

---

## 1. Hardware Architecture and Technical Constraints

The LEGO Mindstorms EV3 programmable brick operates on a Texas Instruments Sitara AM1808 system-on-chip. The system contains a single-core ARM926EJ-S central processing unit running at a base clock speed of 300 MHz. Main memory consists of 64 MB of DRAM, and internal storage consists of 16 MB of SPI Flash memory. System storage expands with a MicroSDHC card slot that supports media up to 32 GB.

| Component | Hardware Specification | Implementation Decision |
| :--- | :--- | :--- |
| **Processor** | TI Sitara AM1808 (ARM926EJ-S @ 300 MHz) | 🎯 **[ADOPTED]** Target CPU set to `arm926ej-s` in [`.cargo/config.toml`](.cargo/config.toml). |
| **Architecture** | 32-bit ARMv5te RISC | 🎯 **[ADOPTED]** Cross-compiled with `armv5te-unknown-linux-musleabi`. |
| **Main Memory** | 64 MB DRAM | 🎯 **[ADOPTED]** Keep binary RSS memory footprint **< 3 MB RAM**. |
| **On-Board Storage** | 16 MB SPI Flash | ℹ️ **[NOT USED]** Reserved for bootloader; OS boots from MicroSD. |
| **Storage Expansion** | MicroSDHC slot (up to 32 GB) | 🎯 **[ADOPTED]** Flashed with `ev3dev-stretch` (one time only). |
| **Network Interfaces** | USB 2.0 Host port, USB Device | 🎯 **[ADOPTED]** Mini-USB RNDIS virtual Ethernet gadget (`192.168.2.2`). |
| **Motor Outputs** | 4 Output Ports (Port A, B, C, D) | 🎯 **[IMPLEMENTED]** Controlled via `/sys/class/tacho-motor/` in [`src/sysfs/motor.rs`](src/sysfs/motor.rs). |

The ARMv5te instruction set lacks hardware floating-point units (FPU) and vector processing hardware. Modern Linux distributions have dropped support for ARMv5 architectures because of these hardware limits. Deploying a functional Linux system on the EV3 requires using patched legacy kernels, such as Linux kernel 4.4 from the ev3dev project.

---

## 2. Operating System Selection and Boot Acceleration

Default boot sequences on Linux distributions like `ev3dev-stretch` take between 90 seconds and 180 seconds to complete. Startup delays occur primarily due to graphical user interfaces, filesystem checks, and network synchronization dependencies.

| Boot Component | Default System Behavior | Optimization Strategy | Implementation Status in Project |
| :--- | :--- | :--- | :--- |
| **LCD Graphical UI (`brickman`)** | Runs graphical menu on brick LCD (~20MB RAM, 40% CPU). | Disable `brickman.service`. | 🎯 **[IMPLEMENTED]** Disabled in setup commands. See [`README.md`](README.md#3-one-time-boot-acceleration--service-setup). |
| **Storage Integrity Check** | Ext4 filesystem check on boot. | Keep active for unclean shutdown recovery. | 🛡️ **[REJECTED MASKING]** Masking fsck causes read-only root mounts after battery pulls. Kept active for recovery. |
| **Network Synchronization** | `connman-wait-online.service` delays boot until network connects (20–40s delay). | Mask wait-online services. | 🎯 **[IMPLEMENTED]** Masked in setup commands. See [`README.md`](README.md#3-one-time-boot-acceleration--service-setup). |
| **Init System Replacement** | systemd service dependency graph. | Replace systemd with BusyBox init script. | ⏸️ **[NOT USED in Phase 1]** Masking heavy systemd services achieved the target 10–15s boot time without rebuilding the kernel rootfs. |
| **Total Cold Boot Time** | **90 to 180 seconds** | **Optimized: 10 to 15 seconds** | 🎯 **[ACHIEVED]** Target boot time is 10–15 seconds. |

---

## 3. Low-Level Motor Control Interface

The EV3 brick features four physical output ports labeled A, B, C, and D for interactive servo motor control. The Linux kernel manages these output ports through the `tacho-motor` sysfs interface subsystem at `/sys/class/tacho-motor/motorX/`.

```
/sys/class/tacho-motor/
├── motor0/
│   ├── address       (e.g., outA)
│   ├── command       (run-forever, run-timed, run-to-rel-pos, stop, reset)
│   ├── speed_sp      (target speed in counts/sec)
│   ├── duty_cycle_sp (direct PWM duty cycle %)
│   ├── position      (current encoder tick count)
│   └── state         (running, holding, stalled)
├── motor1/
├── motor2/
└── motor3/
```

### Architectural Decisions for Motor Control

1. **Direct Sysfs File I/O:**
   - 🎯 **[IMPLEMENTED]** Created a zero-dependency sysfs driver in [`src/sysfs/motor.rs`](src/sysfs/motor.rs).
   - Writes parameters (`speed_sp`, `duty_cycle_sp`, `stop_action`) and dispatches commands directly to sysfs file nodes.

2. **Non-Blocking Background Telemetry Cache:**
   - 🎯 **[IMPLEMENTED]** Synchronous sysfs I/O is prohibited on the HTTP request thread to protect the 300 MHz CPU.
   - A dedicated 50ms background thread in [`src/controller.rs`](src/controller.rs) caches motor encoder values into memory, allowing HTTP `/api/status` requests to respond in `< 0.05ms`.

3. **In-Memory Host Simulation Layer:**
   - 🎯 **[IMPLEMENTED]** Created [`src/sysfs/mock.rs`](src/sysfs/mock.rs) to simulate motor physics, speed, position integration, and emergency stop on Windows without EV3 hardware.

---

## 4. Server Runtime Architecture (Java vs. C vs. Rust)

| Runtime Solution | Memory Usage (RAM) | ARMv5te Support | Startup Latency | Decision & Rationale |
| :--- | :--- | :--- | :--- | :--- |
| **Modern JDK (Java 21+)** | > 100 MB | ❌ Unsupported (Requires ARMv7+) | 5 to 10 seconds | ❌ **[REJECTED]** Exceeds total device memory (64 MB) and lacks ARMv5 instruction support. |
| **Embedded Java JRE (Java 8)** | 25 MB to 35 MB | ⚠️ Supported (leJOS) | 3 to 8 seconds | ❌ **[REJECTED]** Consumes over 50% of available RAM, risking Linux OOM process termination. |
| **NanoHTTPD Engine (Java 8)** | 15 MB to 20 MB | ⚠️ Supported | 2 to 4 seconds | ❌ **[REJECTED]** High memory overhead compared to native compiled binaries. |
| **Native C HTTP Server** | < 2 MB | ✅ Supported | < 10 ms | ℹ️ **[VIABLE ALTERNATIVE]** Highly efficient, but lacks Rust type and memory safety. |
| **Native Rust Server (`tiny_http`)** | **< 3 MB** | ✅ **Fully Supported (`musleabi`)** | **< 10 ms** | 🎯 **[ADOPTED]** Best combination of safety, instant startup, zero dependencies, and < 3 MB RAM footprint. See [`Cargo.toml`](Cargo.toml) and [`src/main.rs`](src/main.rs). |

---

## 5. Cryptographic Overhead and HTTPS Feasibility

The TI Sitara AM1808 processor does not contain hardware acceleration for cryptographic calculations. 

### Architectural Decisions for Networking and Security

1. **Direct On-Device HTTPS / TLS:**
   - ❌ **[REJECTED]** Software TLS handshakes occupy the 300 MHz ARMv5 CPU for 2 to 4 seconds per connection, introducing unacceptable latency into motor commands.

2. **Plain HTTP on Local Network:**
   - 🎯 **[ADOPTED]** The EV3 web server listens on plain HTTP port 80 (or 8080 in simulation) with CORS enabled. See [`src/web/router.rs`](src/web/router.rs).

3. **Edge Reverse Proxy for Remote HTTPS (Optional):**
   - ℹ️ **[DOCUMENTED]** If remote HTTPS access is needed, terminate TLS on a secondary device (host PC or Raspberry Pi) running NGINX, forwarding unencrypted HTTP requests to the EV3. See [`PROJECT_PLAN.md`](PROJECT_PLAN.md).

---

## 6. Rust Implementation Blueprint

1. **Cross-Compilation Target:**
   - 🎯 **[IMPLEMENTED]** Target `armv5te-unknown-linux-musleabi` configured in [`.cargo/config.toml`](.cargo/config.toml) using the local Zig LLD linker ([`zig-lld-arm.cmd`](zig-lld-arm.cmd) / [`zig-linker.py`](zig-linker.py)).
   - Compiles a static release binary in **< 10 seconds** on Windows without Docker.

2. **Embedded Single-Page Application:**
   - 🎯 **[IMPLEMENTED]** HTML5, CSS3, and JavaScript are embedded directly into the executable using `include_str!` in [`src/web/router.rs`](src/web/router.rs).
   - Final statically linked executable size is only **658 KB**.

3. **1-Click Automated Deployment:**
   - 🎯 **[IMPLEMENTED]** Created [`deploy.ps1`](deploy.ps1) and [`deploy.sh`](deploy.sh) to cross-compile, upload over USB, and restart [`ev3-web.service`](ev3-web.service) in **4 seconds**.

---

## 7. Reality Check: Verified Hardware Facts vs. Architectural Assumptions

This section records empirical hardware facts and corrects theoretical assumptions identified during technical review.

| Component / Subsystem | Initial Assumption / Speculation | Verified Hardware Reality | Architectural Status |
| :--- | :--- | :--- | :--- |
| **Power & Battery** | High motor current causes battery voltage sags that brown-out and reboot the ARM9 processor. | EV3 hardware contains dedicated switching step-down regulators (3.3V and 1.8V) isolating the SoC from motor power. Heavy motor loads sag motor bus voltage, but do not reboot the processor with healthy batteries. | 🛡️ **[REJECTED SPECULATION]** Removed brown-out crash warning from [`BUGS.md`](BUGS.md). |
| **macOS USB Networking** | Apple removed RNDIS, preventing macOS hosts from communicating over USB without custom kernel extensions. | `ev3dev-stretch` implements a dual composite USB gadget (RNDIS + CDC-ECM). macOS natively supports CDC-ECM devices and discovers the EV3 as `CDC Composite Gadget` without extra drivers. | 🎯 **[VERIFIED REALITY]** Documented native macOS CDC network setup in [`PROJECT_PLAN.md`](PROJECT_PLAN.md#4-phase-1-one-time-sd-card-setup-and-usb-networking). |
| **LED Sysfs Paths** | LED paths contain side identifiers: `/sys/class/leds/led0:left:red:brick-status`. | Real ev3dev sysfs paths are `/sys/class/leds/led0:red:brick-status` (left) and `/sys/class/leds/led1:red:brick-status` (right). | 🎯 **[CORRECTED SPECIFICATION]** Corrected paths in [`PROJECT_PLAN.md`](PROJECT_PLAN.md#102-phase-7-checklist). |
| **LCD Console Output** | Any text written to `/dev/tty1` displays cleanly and permanently on the brick screen. | Resolution is 178&times;128 pixels. An 8&times;8 console font provides only 22 columns and 16 rows. Text lines must stay under 20 characters. The kernel blanks the console after 10 minutes unless disabled with `setterm -blank 0`. | 🎯 **[ADOPTED SPECIFICATION]** Added line length limits and blanking prevention to [`PROJECT_PLAN.md`](PROJECT_PLAN.md#102-phase-7--pending-bug-remediation-checklist). |
| **On-Brick Wi-Fi Setup** | Disabling `brickman.service` requires configuring Wi-Fi exclusively over USB SSH (`connmanctl`), or keeping the 20 MB `brickman` GUI running. | The 6 physical EV3 buttons emit Linux input events on `/dev/input/by-path/platform-gpio_keys-event` and can drive a zero-overhead 22&times;16 text menu on `/dev/tty1` that calls `connmanctl` to scan SSIDs and enter passwords. | 🎯 **[ADOPTED]** Build native Rust LCD Wi-Fi selector and password picker in [`PROJECT_PLAN.md`](PROJECT_PLAN.md#101-hardware-constraints-and-strategy); ❌ **[REJECTED]** keeping `brickman.service` enabled. |
| **Sysfs Polling Overhead** | Polling sysfs files 20 times per second consumes 40% to 60% of CPU time. | The exact CPU percentage was an unbenchmarked projection. However, reading 4 attributes across 4 motors executes 320 file open/read/close syscalls per second. Reusing open file descriptors and seeking to byte 0 is confirmed standard best practice. | 🎯 **[ADOPTED OPTIMIZATION]** Logged persistent descriptor refactoring as [`BUG-15`](BUGS.md#L110-L116). |
| **Motor Safety Watchdog** | A single 400 ms command timeout works for both momentary Tank Drive and continuous single-motor buttons without client changes. | Momentary Tank Drive in [`web_assets/app.js`](web_assets/app.js) only sent one packet on press, while individual motor card **Fwd/Rev** buttons are intended for continuous operation. | 🎯 **[ADOPTED REQUIREMENT]** Specified two-tier watchdog in [`BUG-14`](BUGS.md#L101-L108): 150 ms Tank Drive repeat heartbeat (400 ms server timeout) + 1000 ms `/api/status` disconnect timeout for continuous motors. |
| **Motor Polarity & Battery** | Standard forward motor polarity and motor-only telemetry are sufficient for wireless operation. | LEGO gear trains and mirrored chassis mounts frequently invert wheel rotation, and wireless battery sessions need low-voltage visibility before the 5.5 V cutoff. | 🎯 **[ADOPTED]** Added per-port polarity inversion (`normal`/`inversed`) and 2-second `/sys/class/power_supply/lego-ev3-battery/` polling to [`PROJECT_PLAN.md`](PROJECT_PLAN.md#101-hardware-constraints-and-strategy). |
| **On-Device HTTPS / TLS** | Modern web encryption can run on the brick with optimized libraries. | The 300 MHz ARM9 processor lacks cryptographic hardware acceleration. Modern TLS handshakes introduce multi-second latency. Rust TLS crates (`ring`) do not support ARMv5te musl. | ❌ **[CONFIRMED REJECTED]** Plain HTTP on local network retained as the only viable architecture. |